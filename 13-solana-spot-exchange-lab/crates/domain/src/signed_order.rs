use crate::{newtype::non_zero_newtype, Error, MarketId, OrderId, Price, Quantity, Side, TraderId};
use core::num::NonZeroU64;
use sha2::{Digest, Sha256};

pub type SignerKey = [u8; 32];
pub type Signature = [u8; 64];

non_zero_newtype!(OrderNonce, u64, NonZeroU64);
non_zero_newtype!(OrderExpirySlot, u64, NonZeroU64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderHash([u8; 32]);

impl OrderHash {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderIntentParams {
    pub order_id: OrderId,
    pub trader_id: TraderId,
    pub market_id: MarketId,
    pub side: Side,
    pub price: Price,
    pub quantity: Quantity,
    pub nonce: OrderNonce,
    pub expiry_slot: OrderExpirySlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrderIntent {
    order_id: OrderId,
    trader_id: TraderId,
    market_id: MarketId,
    side: Side,
    price: Price,
    quantity: Quantity,
    nonce: OrderNonce,
    expiry_slot: OrderExpirySlot,
}

impl OrderIntent {
    pub const fn new(params: OrderIntentParams) -> Self {
        Self {
            order_id: params.order_id,
            trader_id: params.trader_id,
            market_id: params.market_id,
            side: params.side,
            price: params.price,
            quantity: params.quantity,
            nonce: params.nonce,
            expiry_slot: params.expiry_slot,
        }
    }

    pub const fn order_id(self) -> OrderId {
        self.order_id
    }

    pub const fn trader_id(self) -> TraderId {
        self.trader_id
    }

    pub const fn market_id(self) -> MarketId {
        self.market_id
    }

    pub const fn side(self) -> Side {
        self.side
    }

    pub const fn price(self) -> Price {
        self.price
    }

    pub const fn quantity(self) -> Quantity {
        self.quantity
    }

    pub const fn nonce(self) -> OrderNonce {
        self.nonce
    }

    pub const fn expiry_slot(self) -> OrderExpirySlot {
        self.expiry_slot
    }

    pub fn signing_preimage(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(78);
        bytes.extend_from_slice(b"solana-spot-exchange-lab/order/v1");
        bytes.extend_from_slice(&self.order_id.get().to_le_bytes());
        bytes.extend_from_slice(&self.trader_id.get().to_le_bytes());
        bytes.extend_from_slice(&self.market_id.get().to_le_bytes());
        bytes.push(side_tag(self.side));
        bytes.extend_from_slice(&self.price.get().to_le_bytes());
        bytes.extend_from_slice(&self.quantity.get().to_le_bytes());
        bytes.extend_from_slice(&self.nonce.get().to_le_bytes());
        bytes.extend_from_slice(&self.expiry_slot.get().to_le_bytes());
        bytes
    }

    pub fn hash(self) -> OrderHash {
        OrderHash(sha256_32(&self.signing_preimage()))
    }

    pub fn is_expired(self, current_slot: u64) -> bool {
        current_slot > self.expiry_slot.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedOrder {
    intent: OrderIntent,
    signer: SignerKey,
    signature: Signature,
}

impl SignedOrder {
    pub const fn new(intent: OrderIntent, signer: SignerKey, signature: Signature) -> Self {
        Self {
            intent,
            signer,
            signature,
        }
    }

    pub const fn intent(self) -> OrderIntent {
        self.intent
    }

    pub const fn signer(self) -> SignerKey {
        self.signer
    }

    pub const fn signature(self) -> Signature {
        self.signature
    }

    pub fn hash(self) -> OrderHash {
        self.intent.hash()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedFill {
    buyer_order_hash: OrderHash,
    seller_order_hash: OrderHash,
    buyer_trader_id: TraderId,
    seller_trader_id: TraderId,
    market_id: MarketId,
    price: Price,
    quantity: Quantity,
    quote_amount: u128,
}

impl SignedFill {
    pub const fn buyer_order_hash(self) -> OrderHash {
        self.buyer_order_hash
    }

    pub const fn seller_order_hash(self) -> OrderHash {
        self.seller_order_hash
    }

    pub const fn buyer_trader_id(self) -> TraderId {
        self.buyer_trader_id
    }

    pub const fn seller_trader_id(self) -> TraderId {
        self.seller_trader_id
    }

    pub const fn market_id(self) -> MarketId {
        self.market_id
    }

    pub const fn price(self) -> Price {
        self.price
    }

    pub const fn quantity(self) -> Quantity {
        self.quantity
    }

    pub const fn quote_amount(self) -> u128 {
        self.quote_amount
    }
}

pub fn validate_signed_fill(
    bid: SignedOrder,
    ask: SignedOrder,
    fill_price: Price,
    fill_quantity: Quantity,
    current_slot: u64,
) -> Result<SignedFill, Error> {
    let bid_intent = bid.intent();
    let ask_intent = ask.intent();

    if bid_intent.side() != Side::Bid || ask_intent.side() != Side::Ask {
        return Err(Error::SignedOrderWrongSide);
    }
    if bid_intent.market_id() != ask_intent.market_id() {
        return Err(Error::SignedOrderMarketMismatch);
    }
    if bid_intent.trader_id() == ask_intent.trader_id() {
        return Err(Error::SignedOrderSelfTrade);
    }
    if bid_intent.is_expired(current_slot) || ask_intent.is_expired(current_slot) {
        return Err(Error::SignedOrderExpired);
    }
    if bid_intent.price() < ask_intent.price() {
        return Err(Error::SignedOrderPricesDoNotCross);
    }
    if fill_price > bid_intent.price() || fill_price < ask_intent.price() {
        return Err(Error::FillPriceOutsideSignedOrder);
    }
    if fill_quantity > bid_intent.quantity() || fill_quantity > ask_intent.quantity() {
        return Err(Error::FillQuantityExceedsSignedOrder);
    }

    Ok(SignedFill {
        buyer_order_hash: bid.hash(),
        seller_order_hash: ask.hash(),
        buyer_trader_id: bid_intent.trader_id(),
        seller_trader_id: ask_intent.trader_id(),
        market_id: bid_intent.market_id(),
        price: fill_price,
        quantity: fill_quantity,
        quote_amount: fill_price.quote_cost(fill_quantity)?,
    })
}

fn side_tag(side: Side) -> u8 {
    match side {
        Side::Bid => 0,
        Side::Ask => 1,
    }
}

fn sha256_32(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(side: Side, trader_id: u64, price: u64, quantity: u64) -> OrderIntent {
        OrderIntent::new(OrderIntentParams {
            order_id: OrderId::new(u128::from(trader_id)).unwrap(),
            trader_id: TraderId::new(trader_id).unwrap(),
            market_id: MarketId::new(1).unwrap(),
            side,
            price: Price::new(price).unwrap(),
            quantity: Quantity::new(quantity).unwrap(),
            nonce: OrderNonce::new(trader_id).unwrap(),
            expiry_slot: OrderExpirySlot::new(100).unwrap(),
        })
    }

    fn signed(intent: OrderIntent) -> SignedOrder {
        SignedOrder::new(intent, [7; 32], [9; 64])
    }

    #[test]
    fn signing_preimage_is_deterministic_and_domain_separated() {
        let bid = intent(Side::Bid, 1, 100, 5);
        let same_bid = intent(Side::Bid, 1, 100, 5);
        let ask = intent(Side::Ask, 1, 100, 5);

        assert_eq!(bid.signing_preimage(), same_bid.signing_preimage());
        assert_eq!(bid.hash(), same_bid.hash());
        assert_ne!(bid.signing_preimage(), ask.signing_preimage());
        assert_ne!(bid.hash(), ask.hash());
        assert!(bid
            .signing_preimage()
            .starts_with(b"solana-spot-exchange-lab/order/v1"));
    }

    #[test]
    fn compatible_signed_orders_validate_to_fill() {
        let fill = validate_signed_fill(
            signed(intent(Side::Bid, 1, 105, 10)),
            signed(intent(Side::Ask, 2, 100, 8)),
            Price::new(101).unwrap(),
            Quantity::new(7).unwrap(),
            50,
        )
        .unwrap();

        assert_eq!(fill.buyer_trader_id(), TraderId::new(1).unwrap());
        assert_eq!(fill.seller_trader_id(), TraderId::new(2).unwrap());
        assert_eq!(fill.price(), Price::new(101).unwrap());
        assert_eq!(fill.quantity(), Quantity::new(7).unwrap());
        assert_eq!(fill.quote_amount(), 707);
    }

    #[test]
    fn wrong_sides_are_rejected() {
        assert_eq!(
            validate_signed_fill(
                signed(intent(Side::Ask, 1, 100, 10)),
                signed(intent(Side::Bid, 2, 100, 10)),
                Price::new(100).unwrap(),
                Quantity::new(1).unwrap(),
                50,
            ),
            Err(Error::SignedOrderWrongSide)
        );
    }

    #[test]
    fn expired_orders_are_rejected() {
        assert_eq!(
            validate_signed_fill(
                signed(intent(Side::Bid, 1, 105, 10)),
                signed(intent(Side::Ask, 2, 100, 10)),
                Price::new(100).unwrap(),
                Quantity::new(1).unwrap(),
                101,
            ),
            Err(Error::SignedOrderExpired)
        );
    }

    #[test]
    fn non_crossing_prices_are_rejected() {
        assert_eq!(
            validate_signed_fill(
                signed(intent(Side::Bid, 1, 99, 10)),
                signed(intent(Side::Ask, 2, 100, 10)),
                Price::new(100).unwrap(),
                Quantity::new(1).unwrap(),
                50,
            ),
            Err(Error::SignedOrderPricesDoNotCross)
        );
    }

    #[test]
    fn fill_price_must_be_inside_signed_limits() {
        assert_eq!(
            validate_signed_fill(
                signed(intent(Side::Bid, 1, 105, 10)),
                signed(intent(Side::Ask, 2, 100, 10)),
                Price::new(99).unwrap(),
                Quantity::new(1).unwrap(),
                50,
            ),
            Err(Error::FillPriceOutsideSignedOrder)
        );
    }

    #[test]
    fn fill_quantity_must_not_exceed_signed_quantity() {
        assert_eq!(
            validate_signed_fill(
                signed(intent(Side::Bid, 1, 105, 10)),
                signed(intent(Side::Ask, 2, 100, 8)),
                Price::new(100).unwrap(),
                Quantity::new(9).unwrap(),
                50,
            ),
            Err(Error::FillQuantityExceedsSignedOrder)
        );
    }
}
