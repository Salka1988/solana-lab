#![forbid(unsafe_code)]

use application::{CommandId, ExchangeApplication};
use async_trait::async_trait;
use domain::{AssetId, BalanceAmount, Fill, MarketSpec, Order, TraderId};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

#[async_trait]
pub trait EventJournal: Send + 'static {
    async fn append(&mut self, event: &application::Event) -> application::Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopEventJournal;

#[async_trait]
impl EventJournal for NoopEventJournal {
    async fn append(&mut self, _event: &application::Event) -> application::Result<()> {
        Ok(())
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
        request_id: Option<String>,
        command_id: CommandId,
        trader_id: TraderId,
        asset_id: AssetId,
        amount: BalanceAmount,
        reply_to: oneshot::Sender<application::Result<MarketReply>>,
    },
    PlaceOrder {
        request_id: Option<String>,
        command_id: CommandId,
        order: Order,
        reply_to: oneshot::Sender<application::Result<MarketReply>>,
    },
    Snapshot {
        request_id: Option<String>,
        reply_to: oneshot::Sender<application::Result<MarketReply>>,
    },
    Shutdown {
        request_id: Option<String>,
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
        Self::spawn_from_app(ExchangeApplication::new(market), mailbox_capacity, journal)
    }

    pub fn spawn_from_app<J: EventJournal>(
        app: ExchangeApplication,
        mailbox_capacity: usize,
        journal: J,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(mailbox_capacity);
        tokio::spawn(run_market_actor(app, receiver, journal));
        Self { sender }
    }

    pub async fn credit_deposit(
        &self,
        command_id: CommandId,
        trader_id: TraderId,
        asset_id: AssetId,
        amount: BalanceAmount,
    ) -> application::Result<MarketReply> {
        self.credit_deposit_with_request_id(None, command_id, trader_id, asset_id, amount)
            .await
    }

    pub async fn credit_deposit_with_request_id(
        &self,
        request_id: Option<String>,
        command_id: CommandId,
        trader_id: TraderId,
        asset_id: AssetId,
        amount: BalanceAmount,
    ) -> application::Result<MarketReply> {
        let (reply_to, reply_rx) = oneshot::channel();
        let command = MarketCommand::CreditDeposit {
            request_id,
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
        self.place_order_with_request_id(None, command_id, order)
            .await
    }

    pub async fn place_order_with_request_id(
        &self,
        request_id: Option<String>,
        command_id: CommandId,
        order: Order,
    ) -> application::Result<MarketReply> {
        let (reply_to, reply_rx) = oneshot::channel();
        let command = MarketCommand::PlaceOrder {
            request_id,
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
        self.snapshot_with_request_id(None).await
    }

    pub async fn snapshot_with_request_id(
        &self,
        request_id: Option<String>,
    ) -> application::Result<MarketReply> {
        let (reply_to, reply_rx) = oneshot::channel();
        self.sender
            .send(MarketCommand::Snapshot {
                request_id,
                reply_to,
            })
            .await
            .map_err(|_| application::Error::ActorClosed)?;
        reply_rx
            .await
            .map_err(|_| application::Error::ActorClosed)?
    }

    pub async fn shutdown(&self) -> application::Result<MarketReply> {
        self.shutdown_with_request_id(None).await
    }

    pub async fn shutdown_with_request_id(
        &self,
        request_id: Option<String>,
    ) -> application::Result<MarketReply> {
        let (reply_to, reply_rx) = oneshot::channel();
        self.sender
            .send(MarketCommand::Shutdown {
                request_id,
                reply_to,
            })
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
    info!(event_count = app.events().len(), "market actor started");
    while let Some(command) = receiver.recv().await {
        let should_shutdown = handle_command(&mut app, &mut journal, command).await;
        if should_shutdown {
            break;
        }
    }
    info!(event_count = app.events().len(), "market actor stopped");
}

async fn handle_command(
    app: &mut ExchangeApplication,
    journal: &mut impl EventJournal,
    command: MarketCommand,
) -> bool {
    match command {
        MarketCommand::CreditDeposit {
            request_id,
            command_id,
            trader_id,
            asset_id,
            amount,
            reply_to,
        } => {
            info!(
                request_id = request_id.as_deref().unwrap_or(""),
                command_id = command_id.get(),
                trader_id = trader_id.get(),
                asset_id = asset_id.get(),
                amount = amount.get(),
                "deposit command received"
            );
            match app.credit_deposit_event(command_id, trader_id, asset_id, amount) {
                Ok(event) => {
                    let (reply, should_shutdown) = persist_and_apply(
                        app,
                        journal,
                        event,
                        MarketReply::DepositCredited,
                        request_id.as_deref(),
                    )
                    .await;
                    match &reply {
                        Ok(_) => info!(
                            request_id = request_id.as_deref().unwrap_or(""),
                            command_id = command_id.get(),
                            event_count = app.events().len(),
                            "deposit command accepted"
                        ),
                        Err(error) => warn!(
                            request_id = request_id.as_deref().unwrap_or(""),
                            command_id = command_id.get(),
                            error = ?error,
                            "deposit command rejected"
                        ),
                    }
                    let _ = reply_to.send(reply);
                    should_shutdown
                }
                Err(error) => {
                    warn!(
                        request_id = request_id.as_deref().unwrap_or(""),
                        command_id = command_id.get(),
                        trader_id = trader_id.get(),
                        asset_id = asset_id.get(),
                        error = ?error,
                        "deposit command rejected"
                    );
                    let _ = reply_to.send(Err(error));
                    false
                }
            }
        }
        MarketCommand::PlaceOrder {
            request_id,
            command_id,
            order,
            reply_to,
        } => {
            info!(
                request_id = request_id.as_deref().unwrap_or(""),
                command_id = command_id.get(),
                order_id = order.id().get(),
                trader_id = order.trader_id().get(),
                "order command received"
            );
            match app.place_order_event(command_id, order) {
                Ok((event, fills)) => {
                    let fill_count = fills.len();
                    let (reply, should_shutdown) = persist_and_apply(
                        app,
                        journal,
                        event,
                        MarketReply::OrderPlaced { fills },
                        request_id.as_deref(),
                    )
                    .await;
                    match &reply {
                        Ok(_) => info!(
                            request_id = request_id.as_deref().unwrap_or(""),
                            command_id = command_id.get(),
                            order_id = order.id().get(),
                            fill_count,
                            event_count = app.events().len(),
                            "order command accepted"
                        ),
                        Err(error) => warn!(
                            request_id = request_id.as_deref().unwrap_or(""),
                            command_id = command_id.get(),
                            order_id = order.id().get(),
                            error = ?error,
                            "order command rejected"
                        ),
                    }
                    let _ = reply_to.send(reply);
                    should_shutdown
                }
                Err(error) => {
                    warn!(
                        request_id = request_id.as_deref().unwrap_or(""),
                        command_id = command_id.get(),
                        order_id = order.id().get(),
                        trader_id = order.trader_id().get(),
                        error = ?error,
                        "order command rejected"
                    );
                    let _ = reply_to.send(Err(error));
                    false
                }
            }
        }
        MarketCommand::Snapshot {
            request_id,
            reply_to,
        } => {
            info!(
                request_id = request_id.as_deref().unwrap_or(""),
                event_count = app.events().len(),
                "snapshot command received"
            );
            let reply = Ok(MarketReply::Snapshot(MarketSnapshot {
                event_count: app.events().len(),
            }));
            let _ = reply_to.send(reply);
            false
        }
        MarketCommand::Shutdown {
            request_id,
            reply_to,
        } => {
            info!(
                request_id = request_id.as_deref().unwrap_or(""),
                event_count = app.events().len(),
                "shutdown command received"
            );
            let _ = reply_to.send(Ok(MarketReply::Shutdown));
            true
        }
    }
}

async fn persist_and_apply(
    app: &mut ExchangeApplication,
    journal: &mut impl EventJournal,
    event: application::Event,
    success_reply: MarketReply,
    request_id: Option<&str>,
) -> (application::Result<MarketReply>, bool) {
    if journal.append(&event).await.is_err() {
        error!(
            request_id = request_id.unwrap_or(""),
            command_id = event.command_id().get(),
            "journal append failed"
        );
        return (Err(application::Error::JournalAppendFailed), false);
    }

    match app.apply_event(event) {
        Ok(()) => (Ok(success_reply), false),
        Err(error) => {
            error!(
                request_id = request_id.unwrap_or(""),
                error = ?error,
                "event apply failed after journal append"
            );
            (Err(error), true)
        }
    }
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

    #[async_trait]
    impl EventJournal for RecordedEvents {
        async fn append(&mut self, event: &application::Event) -> application::Result<()> {
            self.events.lock().unwrap().push(event.clone());
            Ok(())
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct FailingJournal;

    #[async_trait]
    impl EventJournal for FailingJournal {
        async fn append(&mut self, _event: &application::Event) -> application::Result<()> {
            Err(application::Error::JournalAppendFailed)
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
                request_id: None,
                reply_to: first_reply,
            })
            .unwrap();
        let result = sender.try_send(MarketCommand::Snapshot {
            request_id: None,
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
    async fn actor_started_from_replay_rejects_duplicate_command() {
        let mut source = ExchangeApplication::new(market());
        source
            .credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(7))
            .unwrap();
        let replayed = ExchangeApplication::replay(market(), source.events().iter().cloned())
            .expect("replayed app");
        let actor = MarketActorHandle::spawn_from_app(replayed, 8, NoopEventJournal);

        assert_eq!(
            actor.snapshot().await.unwrap(),
            MarketReply::Snapshot(MarketSnapshot { event_count: 1 })
        );
        assert_eq!(
            actor
                .credit_deposit(command(1), trader(2), base_asset(), BalanceAmount::new(7))
                .await,
            Err(application::Error::DuplicateCommand)
        );
        assert_eq!(
            actor
                .credit_deposit(command(2), trader(2), base_asset(), BalanceAmount::new(7))
                .await
                .unwrap(),
            MarketReply::DepositCredited
        );
    }

    #[tokio::test]
    async fn journal_failure_rejects_reply_without_mutating_actor() {
        let actor = MarketActorHandle::spawn_with_journal(market(), 8, FailingJournal);

        assert_eq!(
            actor
                .credit_deposit(command(1), trader(1), base_asset(), BalanceAmount::new(7))
                .await,
            Err(application::Error::JournalAppendFailed)
        );
        assert_eq!(
            actor.snapshot().await.unwrap(),
            MarketReply::Snapshot(MarketSnapshot { event_count: 0 })
        );
    }
}
