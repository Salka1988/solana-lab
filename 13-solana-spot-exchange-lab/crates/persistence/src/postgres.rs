use anchor_lang::prelude::Pubkey;
use application::Event;
use relayer::SignedSettlementRequest;
use serde::{Deserialize, Serialize};
use sqlx::{migrate::Migrator, postgres::PgPoolOptions, PgPool, Row};
use std::str::FromStr;

use crate::{EventRecord, PersistenceError};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, Clone)]
pub struct PostgresEventJournal {
    pool: PgPool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementOutboxItem {
    pub outbox_id: i64,
    pub attempts: i32,
    pub max_attempts: i32,
    pub request: SignedSettlementRequest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementOutboxRow {
    pub outbox_id: i64,
    pub status: String,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
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
            VALUES ($1::NUMERIC, $2, $3)
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

    pub async fn enqueue_settlement_requests(
        &self,
        requests: &[SignedSettlementRequest],
    ) -> Result<(), PersistenceError> {
        for request in requests {
            sqlx::query(
                r#"
                INSERT INTO settlement_outbox (request_payload)
                VALUES ($1)
                "#,
            )
            .bind(serde_json::to_value(
                SettlementRequestPayload::from_request(*request),
            )?)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    pub async fn settlement_pending_count(&self) -> Result<usize, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) AS pending
            FROM settlement_outbox
            WHERE status = 'pending'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;
        let pending: i64 = row.try_get("pending")?;
        Ok(usize::try_from(pending).unwrap_or(usize::MAX))
    }

    pub async fn recent_settlement_outbox(
        &self,
        limit: i64,
    ) -> Result<Vec<SettlementOutboxRow>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            SELECT outbox_id,
                   status,
                   attempts,
                   max_attempts,
                   last_error,
                   created_at::TEXT AS created_at,
                   updated_at::TEXT AS updated_at
            FROM settlement_outbox
            ORDER BY outbox_id DESC
            LIMIT $1
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(SettlementOutboxRow {
                    outbox_id: row.try_get("outbox_id")?,
                    status: row.try_get("status")?,
                    attempts: row.try_get("attempts")?,
                    max_attempts: row.try_get("max_attempts")?,
                    last_error: row.try_get("last_error")?,
                    created_at: row.try_get("created_at")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect()
    }

    pub async fn claim_pending_settlements(
        &self,
        limit: i64,
    ) -> Result<Vec<SettlementOutboxItem>, PersistenceError> {
        let rows = sqlx::query(
            r#"
            UPDATE settlement_outbox
            SET attempts = attempts + 1,
                updated_at = now()
            WHERE outbox_id IN (
                SELECT outbox_id
                FROM settlement_outbox
                WHERE status = 'pending'
                ORDER BY outbox_id ASC
                LIMIT $1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING outbox_id, attempts, max_attempts, request_payload
            "#,
        )
        .bind(limit.max(1))
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let outbox_id: i64 = row.try_get("outbox_id")?;
                let attempts: i32 = row.try_get("attempts")?;
                let max_attempts: i32 = row.try_get("max_attempts")?;
                let payload: serde_json::Value = row.try_get("request_payload")?;
                let payload: SettlementRequestPayload = serde_json::from_value(payload)?;
                Ok(SettlementOutboxItem {
                    outbox_id,
                    attempts,
                    max_attempts,
                    request: payload.into_request()?,
                })
            })
            .collect()
    }

    pub async fn keep_settlement_pending(
        &self,
        outbox_id: i64,
        error: &str,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            UPDATE settlement_outbox
            SET status = 'pending',
                last_error = $2,
                updated_at = now()
            WHERE outbox_id = $1
            "#,
        )
        .bind(outbox_id)
        .bind(error)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_settlement_submitted(
        &self,
        outbox_id: i64,
        signature: relayer::TransactionSignature,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            UPDATE settlement_outbox
            SET status = 'submitted',
                transaction_signature = $2,
                last_error = NULL,
                updated_at = now()
            WHERE outbox_id = $1
            "#,
        )
        .bind(outbox_id)
        .bind(signature.to_vec())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_settlement_failed(
        &self,
        outbox_id: i64,
        error: &str,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            UPDATE settlement_outbox
            SET status = 'failed',
                last_error = $2,
                updated_at = now()
            WHERE outbox_id = $1
            "#,
        )
        .bind(outbox_id)
        .bind(error)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SettlementRequestPayload {
    settlement_authority: String,
    base_mint: String,
    quote_mint: String,
    buyer: String,
    seller: String,
    payer: String,
    args: SignedFillPayload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SignedFillPayload {
    settlement_id: u64,
    fill_price: u64,
    fill_quantity: u64,
    buyer_order_hash: [u8; 32],
    seller_order_hash: [u8; 32],
    buyer_order: SignedOrderPayload,
    buyer_signature: Vec<u8>,
    seller_order: SignedOrderPayload,
    seller_signature: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SignedOrderPayload {
    order_id: u64,
    market_config: String,
    trader: String,
    side: String,
    price: u64,
    quantity: u64,
    nonce: u64,
    expiry_slot: u64,
}

impl SettlementRequestPayload {
    fn from_request(request: SignedSettlementRequest) -> Self {
        Self {
            settlement_authority: request.settlement_authority.to_string(),
            base_mint: request.base_mint.to_string(),
            quote_mint: request.quote_mint.to_string(),
            buyer: request.buyer.to_string(),
            seller: request.seller.to_string(),
            payer: request.payer.to_string(),
            args: SignedFillPayload::from_args(request.args),
        }
    }

    fn into_request(self) -> Result<SignedSettlementRequest, PersistenceError> {
        Ok(SignedSettlementRequest {
            settlement_authority: parse_pubkey(&self.settlement_authority)?,
            base_mint: parse_pubkey(&self.base_mint)?,
            quote_mint: parse_pubkey(&self.quote_mint)?,
            buyer: parse_pubkey(&self.buyer)?,
            seller: parse_pubkey(&self.seller)?,
            payer: parse_pubkey(&self.payer)?,
            args: self.args.into_args()?,
        })
    }
}

impl SignedFillPayload {
    fn from_args(args: spot_settlement::SignedFillArgs) -> Self {
        Self {
            settlement_id: args.settlement_id,
            fill_price: args.fill_price,
            fill_quantity: args.fill_quantity,
            buyer_order_hash: args.buyer_order_hash,
            seller_order_hash: args.seller_order_hash,
            buyer_order: SignedOrderPayload::from_order(args.buyer_order),
            buyer_signature: args.buyer_signature.to_vec(),
            seller_order: SignedOrderPayload::from_order(args.seller_order),
            seller_signature: args.seller_signature.to_vec(),
        }
    }

    fn into_args(self) -> Result<spot_settlement::SignedFillArgs, PersistenceError> {
        Ok(spot_settlement::SignedFillArgs {
            settlement_id: self.settlement_id,
            fill_price: self.fill_price,
            fill_quantity: self.fill_quantity,
            buyer_order_hash: self.buyer_order_hash,
            seller_order_hash: self.seller_order_hash,
            buyer_order: self.buyer_order.into_order()?,
            buyer_signature: vec_to_signature(self.buyer_signature)?,
            seller_order: self.seller_order.into_order()?,
            seller_signature: vec_to_signature(self.seller_signature)?,
        })
    }
}

impl SignedOrderPayload {
    fn from_order(order: spot_settlement::SignedOrderPayload) -> Self {
        Self {
            order_id: order.order_id,
            market_config: order.market_config.to_string(),
            trader: order.trader.to_string(),
            side: match order.side {
                spot_settlement::SignedOrderSide::Bid => "bid",
                spot_settlement::SignedOrderSide::Ask => "ask",
            }
            .to_owned(),
            price: order.price,
            quantity: order.quantity,
            nonce: order.nonce,
            expiry_slot: order.expiry_slot,
        }
    }

    fn into_order(self) -> Result<spot_settlement::SignedOrderPayload, PersistenceError> {
        Ok(spot_settlement::SignedOrderPayload {
            order_id: self.order_id,
            market_config: parse_pubkey(&self.market_config)?,
            trader: parse_pubkey(&self.trader)?,
            side: match self.side.as_str() {
                "bid" => spot_settlement::SignedOrderSide::Bid,
                "ask" => spot_settlement::SignedOrderSide::Ask,
                side => {
                    return Err(PersistenceError::Serde(format!(
                        "unknown order side: {side}"
                    )))
                }
            },
            price: self.price,
            quantity: self.quantity,
            nonce: self.nonce,
            expiry_slot: self.expiry_slot,
        })
    }
}

fn parse_pubkey(value: &str) -> Result<Pubkey, PersistenceError> {
    Pubkey::from_str(value).map_err(|error| PersistenceError::Serde(error.to_string()))
}

fn vec_to_signature(value: Vec<u8>) -> Result<[u8; 64], PersistenceError> {
    value
        .try_into()
        .map_err(|_| PersistenceError::Serde("signature must be 64 bytes".to_owned()))
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
