use anchor_lang::prelude::Pubkey;
use application::SettlementBatch;
use domain::{MarketId, OrderId, TraderId};

use crate::SignedSettlementRequest;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementSignedOrder {
    pub trader_id: TraderId,
    pub order_hash: [u8; 32],
    pub order: spot_settlement::SignedOrderPayload,
    pub signature: [u8; 64],
}

pub trait SettlementSignedOrderSource {
    fn signed_order(&self, order_id: OrderId) -> Option<SettlementSignedOrder>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplicationSettlementBridge {
    market_id: MarketId,
    settlement_authority: Pubkey,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    payer: Pubkey,
}

impl ApplicationSettlementBridge {
    pub const fn new(
        market_id: MarketId,
        settlement_authority: Pubkey,
        base_mint: Pubkey,
        quote_mint: Pubkey,
        payer: Pubkey,
    ) -> Self {
        Self {
            market_id,
            settlement_authority,
            base_mint,
            quote_mint,
            payer,
        }
    }

    pub fn requests_from_batch(
        self,
        batch: &SettlementBatch,
        signed_orders: &impl SettlementSignedOrderSource,
        first_settlement_id: u64,
    ) -> Result<Vec<SignedSettlementRequest>, ApplicationSettlementBridgeError> {
        let mut requests = Vec::with_capacity(batch.intents().len());

        for (index, intent) in batch.intents().iter().copied().enumerate() {
            if intent.market_id() != self.market_id {
                return Err(ApplicationSettlementBridgeError::MarketMismatch);
            }

            let settlement_id = first_settlement_id
                .checked_add(
                    u64::try_from(index)
                        .map_err(|_| ApplicationSettlementBridgeError::SettlementIdOverflow)?,
                )
                .ok_or(ApplicationSettlementBridgeError::SettlementIdOverflow)?;
            let maker = signed_orders.signed_order(intent.maker_order_id()).ok_or(
                ApplicationSettlementBridgeError::MissingSignedOrder(intent.maker_order_id()),
            )?;
            let taker = signed_orders.signed_order(intent.taker_order_id()).ok_or(
                ApplicationSettlementBridgeError::MissingSignedOrder(intent.taker_order_id()),
            )?;
            let (buyer, seller) = bid_ask_orders(maker, taker)?;

            validate_order(intent.maker_order_id(), maker, self.market_config())?;
            validate_order(intent.taker_order_id(), taker, self.market_config())?;
            validate_fill(intent, buyer, seller)?;

            requests.push(SignedSettlementRequest {
                settlement_authority: self.settlement_authority,
                base_mint: self.base_mint,
                quote_mint: self.quote_mint,
                buyer: buyer.order.trader,
                seller: seller.order.trader,
                payer: self.payer,
                args: spot_settlement::SignedFillArgs {
                    settlement_id,
                    fill_price: intent.price().get(),
                    fill_quantity: intent.quantity().get(),
                    buyer_order_hash: buyer.order_hash,
                    seller_order_hash: seller.order_hash,
                    buyer_order: buyer.order,
                    buyer_signature: buyer.signature,
                    seller_order: seller.order,
                    seller_signature: seller.signature,
                },
            });
        }

        Ok(requests)
    }

    fn market_config(self) -> Pubkey {
        settlement_client::market_config_pda(self.base_mint, self.quote_mint).0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplicationSettlementBridgeError {
    MissingSignedOrder(OrderId),
    OrderIdOutOfRange(OrderId),
    SignedOrderMismatch,
    MarketMismatch,
    InvalidSignedOrderSides,
    FillOutsideSignedOrders,
    SettlementIdOverflow,
}

fn validate_order(
    order_id: OrderId,
    signed_order: SettlementSignedOrder,
    market_config: Pubkey,
) -> Result<(), ApplicationSettlementBridgeError> {
    let order_id_u64 = u64::try_from(order_id.get())
        .map_err(|_| ApplicationSettlementBridgeError::OrderIdOutOfRange(order_id))?;

    if signed_order.order.order_id != order_id_u64
        || signed_order.order.market_config != market_config
    {
        return Err(ApplicationSettlementBridgeError::SignedOrderMismatch);
    }

    Ok(())
}

fn bid_ask_orders(
    lhs: SettlementSignedOrder,
    rhs: SettlementSignedOrder,
) -> Result<(SettlementSignedOrder, SettlementSignedOrder), ApplicationSettlementBridgeError> {
    match (lhs.order.side, rhs.order.side) {
        (spot_settlement::SignedOrderSide::Bid, spot_settlement::SignedOrderSide::Ask) => {
            Ok((lhs, rhs))
        }
        (spot_settlement::SignedOrderSide::Ask, spot_settlement::SignedOrderSide::Bid) => {
            Ok((rhs, lhs))
        }
        _ => Err(ApplicationSettlementBridgeError::InvalidSignedOrderSides),
    }
}

fn validate_fill(
    intent: application::SettlementIntent,
    buyer: SettlementSignedOrder,
    seller: SettlementSignedOrder,
) -> Result<(), ApplicationSettlementBridgeError> {
    if buyer.trader_id != intent.buyer_trader_id()
        || seller.trader_id != intent.seller_trader_id()
        || intent.price().get() > buyer.order.price
        || intent.price().get() < seller.order.price
        || intent.quantity().get() > buyer.order.quantity
        || intent.quantity().get() > seller.order.quantity
    {
        return Err(ApplicationSettlementBridgeError::FillOutsideSignedOrders);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::CommandId;
    use domain::{Fill, Order, OrderSequence, Price, Quantity, Side};

    struct VecSignedOrders(Vec<(OrderId, SettlementSignedOrder)>);

    impl SettlementSignedOrderSource for VecSignedOrders {
        fn signed_order(&self, order_id: OrderId) -> Option<SettlementSignedOrder> {
            self.0
                .iter()
                .find(|(candidate, _)| *candidate == order_id)
                .map(|(_, signed_order)| *signed_order)
        }
    }

    fn pubkey(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn market_id() -> MarketId {
        MarketId::new(7).unwrap()
    }

    fn trader(id: u64) -> TraderId {
        TraderId::new(id).unwrap()
    }

    fn order_id(id: u128) -> OrderId {
        OrderId::new(id).unwrap()
    }

    fn domain_order(id: u128, trader_id: u64, side: Side) -> Order {
        Order::new(
            order_id(id),
            trader(trader_id),
            market_id(),
            side,
            Price::new(100).unwrap(),
            Quantity::new(5).unwrap(),
            OrderSequence::new(id.try_into().unwrap()).unwrap(),
        )
    }

    fn signed_order(
        order_id: u64,
        trader_id: u64,
        trader: Pubkey,
        side: spot_settlement::SignedOrderSide,
        market_config: Pubkey,
    ) -> SettlementSignedOrder {
        SettlementSignedOrder {
            trader_id: TraderId::new(trader_id).unwrap(),
            order_hash: [order_id as u8; 32],
            order: spot_settlement::SignedOrderPayload {
                order_id,
                market_config,
                trader,
                side,
                price: 100,
                quantity: 5,
                nonce: order_id,
                expiry_slot: u64::MAX,
            },
            signature: [trader_id as u8; 64],
        }
    }

    fn bridge() -> ApplicationSettlementBridge {
        ApplicationSettlementBridge::new(market_id(), pubkey(8), pubkey(11), pubkey(12), pubkey(9))
    }

    #[test]
    fn builds_signed_settlement_requests_from_application_batch() {
        let bridge = bridge();
        let market_config = bridge.market_config();
        let maker = signed_order(
            1,
            10,
            pubkey(10),
            spot_settlement::SignedOrderSide::Ask,
            market_config,
        );
        let taker = signed_order(
            2,
            20,
            pubkey(20),
            spot_settlement::SignedOrderSide::Bid,
            market_config,
        );
        let source = VecSignedOrders(vec![(order_id(1), maker), (order_id(2), taker)]);
        let fill = Fill::from_parts(
            order_id(1),
            order_id(2),
            trader(10),
            trader(20),
            Price::new(100).unwrap(),
            Quantity::new(3).unwrap(),
        );
        let batch = SettlementBatch::from_order_fills(
            CommandId::new(1).unwrap(),
            domain_order(2, 20, Side::Bid),
            &[fill],
        );

        let requests = bridge.requests_from_batch(&batch, &source, 40).unwrap();

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].buyer, pubkey(20));
        assert_eq!(requests[0].seller, pubkey(10));
        assert_eq!(requests[0].args.settlement_id, 40);
        assert_eq!(requests[0].args.fill_quantity, 3);
        assert_eq!(requests[0].args.buyer_order_hash, [2; 32]);
        assert_eq!(requests[0].args.seller_order_hash, [1; 32]);
    }

    #[test]
    fn rejects_missing_signed_order() {
        let batch = SettlementBatch::from_order_fills(
            CommandId::new(1).unwrap(),
            domain_order(2, 20, Side::Bid),
            &[Fill::from_parts(
                order_id(1),
                order_id(2),
                trader(10),
                trader(20),
                Price::new(100).unwrap(),
                Quantity::new(3).unwrap(),
            )],
        );

        assert_eq!(
            bridge().requests_from_batch(&batch, &VecSignedOrders(Vec::new()), 1),
            Err(ApplicationSettlementBridgeError::MissingSignedOrder(
                order_id(1)
            ))
        );
    }

    #[test]
    fn rejects_same_side_signed_orders() {
        let bridge = bridge();
        let market_config = bridge.market_config();
        let source = VecSignedOrders(vec![
            (
                order_id(1),
                signed_order(
                    1,
                    10,
                    pubkey(10),
                    spot_settlement::SignedOrderSide::Bid,
                    market_config,
                ),
            ),
            (
                order_id(2),
                signed_order(
                    2,
                    20,
                    pubkey(20),
                    spot_settlement::SignedOrderSide::Bid,
                    market_config,
                ),
            ),
        ]);
        let batch = SettlementBatch::from_order_fills(
            CommandId::new(1).unwrap(),
            domain_order(2, 20, Side::Bid),
            &[Fill::from_parts(
                order_id(1),
                order_id(2),
                trader(10),
                trader(20),
                Price::new(100).unwrap(),
                Quantity::new(3).unwrap(),
            )],
        );

        assert_eq!(
            bridge.requests_from_batch(&batch, &source, 1),
            Err(ApplicationSettlementBridgeError::InvalidSignedOrderSides)
        );
    }
}
