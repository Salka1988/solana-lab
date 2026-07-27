#![forbid(unsafe_code)]

use core::{future::Future, pin::Pin};

use application::{CommandId, ExchangeApplication};
use domain::{AssetId, BalanceAmount, Fill, MarketSpec, Order, TraderId};
use tokio::sync::{mpsc, oneshot};

pub trait EventJournal: Send + 'static {
    fn append<'a>(
        &'a mut self,
        event: &'a application::Event,
    ) -> Pin<Box<dyn Future<Output = application::Result<()>> + Send + 'a>>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopEventJournal;

impl EventJournal for NoopEventJournal {
    fn append<'a>(
        &'a mut self,
        _event: &'a application::Event,
    ) -> Pin<Box<dyn Future<Output = application::Result<()>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarketSnapshot {
    pub event_count: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MarketReply {
    DepositCredited,
    OrderPlaced { fills: Vec<Fill> },
    Snapshot(MarketSnapshot),
    Shutdown,
}

#[derive(Debug)]
enum MarketCommand {
    CreditDeposit {
        command_id: CommandId,
        trader_id: TraderId,
        asset_id: AssetId,
        amount: BalanceAmount,
        reply_to: oneshot::Sender<application::Result<MarketReply>>,
    },
    PlaceOrder {
        command_id: CommandId,
        order: Order,
        reply_to: oneshot::Sender<application::Result<MarketReply>>,
    },
    Snapshot {
        reply_to: oneshot::Sender<application::Result<MarketReply>>,
    },
    Shutdown {
        reply_to: oneshot::Sender<application::Result<MarketReply>>,
    },
}

#[derive(Debug, Clone)]
pub struct MarketActorHandle {
    sender: mpsc::Sender<MarketCommand>,
}

impl MarketActorHandle {
    pub fn spawn(market: MarketSpec, mailbox_capacity: usize) -> Self {
        Self::spawn_with_journal(market, mailbox_capacity, NoopEventJournal)
    }

    pub fn spawn_with_journal<J: EventJournal>(
        market: MarketSpec,
        mailbox_capacity: usize,
        journal: J,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(mailbox_capacity);
        tokio::spawn(run_market_actor(
            ExchangeApplication::new(market),
            receiver,
            journal,
        ));
        Self { sender }
    }

    pub async fn credit_deposit(
        &self,
        command_id: CommandId,
        trader_id: TraderId,
        asset_id: AssetId,
        amount: BalanceAmount,
    ) -> application::Result<MarketReply> {
        let (reply_to, reply_rx) = oneshot::channel();
        let command = MarketCommand::CreditDeposit {
            command_id,
            trader_id,
            asset_id,
            amount,
            reply_to,
        };
        self.sender
            .send(command)
            .await
            .map_err(|_| application::Error::ActorClosed)?;
        reply_rx
            .await
            .map_err(|_| application::Error::ActorClosed)?
    }

    pub async fn place_order(
        &self,
        command_id: CommandId,
        order: Order,
    ) -> application::Result<MarketReply> {
        let (reply_to, reply_rx) = oneshot::channel();
        let command = MarketCommand::PlaceOrder {
            command_id,
            order,
            reply_to,
        };
        self.sender
            .send(command)
            .await
            .map_err(|_| application::Error::ActorClosed)?;
        reply_rx
            .await
            .map_err(|_| application::Error::ActorClosed)?
    }

    pub async fn snapshot(&self) -> application::Result<MarketReply> {
        let (reply_to, reply_rx) = oneshot::channel();
        self.sender
            .send(MarketCommand::Snapshot { reply_to })
            .await
            .map_err(|_| application::Error::ActorClosed)?;
        reply_rx
            .await
            .map_err(|_| application::Error::ActorClosed)?
    }

    pub async fn shutdown(&self) -> application::Result<MarketReply> {
        let (reply_to, reply_rx) = oneshot::channel();
        self.sender
            .send(MarketCommand::Shutdown { reply_to })
            .await
            .map_err(|_| application::Error::ActorClosed)?;
        reply_rx
            .await
            .map_err(|_| application::Error::ActorClosed)?
    }
}

async fn run_market_actor(
    mut app: ExchangeApplication,
    mut receiver: mpsc::Receiver<MarketCommand>,
    mut journal: impl EventJournal,
) {
    while let Some(command) = receiver.recv().await {
        let should_shutdown = handle_command(&mut app, &mut journal, command).await;
        if should_shutdown {
            break;
        }
    }
}

async fn handle_command(
    app: &mut ExchangeApplication,
    journal: &mut impl EventJournal,
    command: MarketCommand,
) -> bool {
    match command {
        MarketCommand::CreditDeposit {
            command_id,
            trader_id,
            asset_id,
            amount,
            reply_to,
        } => match app.credit_deposit(command_id, trader_id, asset_id, amount) {
            Ok(()) => {
                let should_shutdown = persist_last_event(app, journal).await.is_err();
                let reply = if should_shutdown {
                    Err(application::Error::JournalAppendFailed)
                } else {
                    Ok(MarketReply::DepositCredited)
                };
                let _ = reply_to.send(reply);
                should_shutdown
            }
            Err(error) => {
                let _ = reply_to.send(Err(error));
                false
            }
        },
        MarketCommand::PlaceOrder {
            command_id,
            order,
            reply_to,
        } => match app.place_order(command_id, order) {
            Ok(fills) => {
                let should_shutdown = persist_last_event(app, journal).await.is_err();
                let reply = if should_shutdown {
                    Err(application::Error::JournalAppendFailed)
                } else {
                    Ok(MarketReply::OrderPlaced { fills })
                };
                let _ = reply_to.send(reply);
                should_shutdown
            }
            Err(error) => {
                let _ = reply_to.send(Err(error));
                false
            }
        },
        MarketCommand::Snapshot { reply_to } => {
            let reply = Ok(MarketReply::Snapshot(MarketSnapshot {
                event_count: app.events().len(),
            }));
            let _ = reply_to.send(reply);
            false
        }
        MarketCommand::Shutdown { reply_to } => {
            let _ = reply_to.send(Ok(MarketReply::Shutdown));
            true
        }
    }
}

async fn persist_last_event(
    app: &ExchangeApplication,
    journal: &mut impl EventJournal,
) -> application::Result<()> {
    let event = app
        .last_event()
        .ok_or(application::Error::JournalAppendFailed)?;
    journal.append(event).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{LotSize, MarketId, OrderId, OrderSequence, Price, Quantity, Side, TickSize};
    use std::sync::{Arc, Mutex};

    fn command(id: u128) -> CommandId {
        CommandId::new(id).unwrap()
    }

    fn base_asset() -> AssetId {
        AssetId::new(10).unwrap()
    }

    fn quote_asset() -> AssetId {
        AssetId::new(20).unwrap()
    }

    fn trader(id: u64) -> TraderId {
        TraderId::new(id).unwrap()
    }

    fn market() -> MarketSpec {
        MarketSpec::new(
            base_asset(),
            quote_asset(),
            TickSize::new(1).unwrap(),
            LotSize::new(1).unwrap(),
        )
        .unwrap()
    }

    fn order(id: u128, trader_id: TraderId, side: Side, price: u64, quantity: u64) -> Order {
        Order::new(
            OrderId::new(id).unwrap(),
            trader_id,
            MarketId::new(1).unwrap(),
            side,
            Price::new(price).unwrap(),
            Quantity::new(quantity).unwrap(),
            OrderSequence::new(id.try_into().unwrap()).unwrap(),
        )
    }

    #[derive(Debug, Clone, Default)]
    struct RecordedEvents {
        events: Arc<Mutex<Vec<application::Event>>>,
    }

    impl RecordedEvents {
        fn snapshot(&self) -> Vec<application::Event> {
            self.events.lock().unwrap().clone()
        }
    }

    impl EventJournal for RecordedEvents {
        fn append<'a>(
            &'a mut self,
            event: &'a application::Event,
        ) -> Pin<Box<dyn Future<Output = application::Result<()>> + Send + 'a>> {
            let event = event.clone();
            let events = Arc::clone(&self.events);
            Box::pin(async move {
                events.lock().unwrap().push(event);
                Ok(())
            })
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct FailingJournal;

    impl EventJournal for FailingJournal {
        fn append<'a>(
            &'a mut self,
            _event: &'a application::Event,
        ) -> Pin<Box<dyn Future<Output = application::Result<()>> + Send + 'a>> {
            Box::pin(async { Err(application::Error::JournalAppendFailed) })
        }
    }

    #[tokio::test]
    async fn deposit_command_works() {
        let actor = MarketActorHandle::spawn(market(), 8);

        let reply = actor
            .credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(10))
            .await
            .unwrap();

        assert_eq!(reply, MarketReply::DepositCredited);
        assert_eq!(
            actor.snapshot().await.unwrap(),
            MarketReply::Snapshot(MarketSnapshot { event_count: 1 })
        );
    }

    #[tokio::test]
    async fn place_order_command_works() {
        let actor = MarketActorHandle::spawn(market(), 8);

        actor
            .credit_deposit(
                command(1),
                trader(1),
                quote_asset(),
                BalanceAmount::new(700),
            )
            .await
            .unwrap();
        let reply = actor
            .place_order(command(2), order(1, trader(1), Side::Bid, 100, 7))
            .await
            .unwrap();

        assert_eq!(reply, MarketReply::OrderPlaced { fills: Vec::new() });
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_senders_are_serialized_by_mailbox() {
        let actor = MarketActorHandle::spawn(market(), 8);
        let first = actor.clone();
        let second = actor.clone();

        let first_task = tokio::spawn(async move {
            first
                .credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(7))
                .await
        });
        let second_task = tokio::spawn(async move {
            second
                .credit_deposit(
                    command(2),
                    trader(2),
                    quote_asset(),
                    BalanceAmount::new(700),
                )
                .await
        });

        assert_eq!(
            first_task.await.unwrap().unwrap(),
            MarketReply::DepositCredited
        );
        assert_eq!(
            second_task.await.unwrap().unwrap(),
            MarketReply::DepositCredited
        );
        assert_eq!(
            actor.snapshot().await.unwrap(),
            MarketReply::Snapshot(MarketSnapshot { event_count: 2 })
        );
    }

    #[tokio::test]
    async fn try_send_reports_full_mailbox() {
        let (sender, _receiver) = mpsc::channel(1);
        let (first_reply, _first_rx) = oneshot::channel();
        let (second_reply, _second_rx) = oneshot::channel();

        sender
            .try_send(MarketCommand::Snapshot {
                reply_to: first_reply,
            })
            .unwrap();
        let result = sender.try_send(MarketCommand::Snapshot {
            reply_to: second_reply,
        });

        assert!(matches!(result, Err(mpsc::error::TrySendError::Full(_))));
    }

    #[tokio::test]
    async fn shutdown_stops_actor() {
        let actor = MarketActorHandle::spawn(market(), 8);

        assert_eq!(actor.shutdown().await.unwrap(), MarketReply::Shutdown);
        assert_eq!(actor.snapshot().await, Err(application::Error::ActorClosed));
    }

    #[tokio::test]
    async fn successful_commands_are_persisted_before_reply() {
        let journal = RecordedEvents::default();
        let recorded = journal.clone();
        let actor = MarketActorHandle::spawn_with_journal(market(), 8, journal);

        assert_eq!(
            actor
                .credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(7))
                .await
                .unwrap(),
            MarketReply::DepositCredited
        );
        assert_eq!(
            actor
                .place_order(command(2), order(1, trader(1), Side::Ask, 100, 7))
                .await
                .unwrap(),
            MarketReply::OrderPlaced { fills: Vec::new() }
        );

        let events = recorded.snapshot();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].command_id(), command(1));
        assert_eq!(events[1].command_id(), command(2));
    }

    #[tokio::test]
    async fn journal_failure_rejects_reply_and_closes_actor() {
        let actor = MarketActorHandle::spawn_with_journal(market(), 8, FailingJournal);

        assert_eq!(
            actor
                .credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(7))
                .await,
            Err(application::Error::JournalAppendFailed)
        );
        assert_eq!(actor.snapshot().await, Err(application::Error::ActorClosed));
    }
}
