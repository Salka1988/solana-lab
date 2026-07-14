use application::Event;
use sqlx::{migrate::Migrator, postgres::PgPoolOptions, PgPool, Row};

use crate::{EventRecord, PersistenceError};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone)]
pub struct PostgresEventJournal {
    pool: PgPool,
}

impl PostgresEventJournal {
    pub async fn connect(database_url: &str) -> Result<Self, PersistenceError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        Ok(Self { pool })
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn migrate(&self) -> Result<(), PersistenceError> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    pub async fn append(&self, event: &Event) -> Result<(), PersistenceError> {
        let record = EventRecord::try_from(event)?;
        sqlx::query(
            r#"
            INSERT INTO event_journal (command_id, event_type, payload)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(record.command_id.get().to_string())
        .bind(record.event_type)
        .bind(record.payload)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn read_all(&self) -> Result<Vec<Event>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT command_id::TEXT, event_type, payload
            FROM event_journal
            ORDER BY event_id ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let command_id: String = row.try_get("command_id")?;
                let event_type: String = row.try_get("event_type")?;
                let payload: serde_json::Value = row.try_get("payload")?;
                let command_id = command_id
                    .parse::<u128>()
                    .map_err(|error| PersistenceError::Serde(error.to_string()))?;
                EventRecord {
                    command_id: application::CommandId::new(command_id)?,
                    event_type,
                    payload,
                }
                .into_event()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use application::{CommandId, Event};
    use domain::{AssetId, BalanceAmount, TraderId};

    use super::*;

    fn deposit_event(command_id: u128) -> Event {
        Event::DepositCredited {
            command_id: CommandId::new(command_id).unwrap(),
            trader_id: TraderId::new(1).unwrap(),
            asset_id: AssetId::new(2).unwrap(),
            amount: BalanceAmount::new(3),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires DATABASE_URL"]
    async fn postgres_journal_appends_and_reads_events_in_order(pool: PgPool) {
        let journal = PostgresEventJournal::from_pool(pool);
        let first = deposit_event(1);
        let second = deposit_event(2);

        journal.append(&first).await.unwrap();
        journal.append(&second).await.unwrap();

        assert_eq!(journal.read_all().await.unwrap(), vec![first, second]);
    }

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires DATABASE_URL"]
    async fn postgres_journal_rejects_duplicate_command_id(pool: PgPool) {
        let journal = PostgresEventJournal::from_pool(pool);
        let event = deposit_event(1);

        journal.append(&event).await.unwrap();

        assert!(matches!(
            journal.append(&event).await,
            Err(PersistenceError::Sql(_))
        ));
    }
}
