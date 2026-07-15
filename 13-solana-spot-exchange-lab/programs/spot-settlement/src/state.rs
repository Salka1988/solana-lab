use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustodyAsset {
    Base,
    Quote,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum SignedOrderSide {
    Bid,
    Ask,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignedOrderPayload {
    pub order_id: u64,
    pub market_config: Pubkey,
    pub trader: Pubkey,
    pub side: SignedOrderSide,
    pub price: u64,
    pub quantity: u64,
    pub nonce: u64,
    pub expiry_slot: u64,
}

impl SignedOrderPayload {
    pub fn signing_preimage(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(106);
        bytes.extend_from_slice(b"solana-spot-exchange-lab/order/v1");
        bytes.extend_from_slice(&self.order_id.to_le_bytes());
        bytes.extend_from_slice(self.trader.as_ref());
        bytes.extend_from_slice(self.market_config.as_ref());
        bytes.push(match self.side {
            SignedOrderSide::Bid => 0,
            SignedOrderSide::Ask => 1,
        });
        bytes.extend_from_slice(&self.price.to_le_bytes());
        bytes.extend_from_slice(&self.quantity.to_le_bytes());
        bytes.extend_from_slice(&self.nonce.to_le_bytes());
        bytes.extend_from_slice(&self.expiry_slot.to_le_bytes());
        bytes
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignedFillArgs {
    pub settlement_id: u64,
    pub fill_price: u64,
    pub fill_quantity: u64,
    pub buyer_order_hash: [u8; 32],
    pub seller_order_hash: [u8; 32],
    pub buyer_order: SignedOrderPayload,
    pub buyer_signature: [u8; 64],
    pub seller_order: SignedOrderPayload,
    pub seller_signature: [u8; 64],
}

#[account]
pub struct ProtocolConfig {
    pub admin: Pubkey,
    pub bump: u8,
}

impl ProtocolConfig {
    pub const LEN: usize = 32 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct MarketConfig {
    pub protocol_config: Pubkey,
    pub admin: Pubkey,
    pub settlement_authority: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub base_vault: Pubkey,
    pub quote_vault: Pubkey,
    pub vault_authority: Pubkey,
    pub vault_authority_bump: u8,
    pub paused: bool,
    pub bump: u8,
}

impl MarketConfig {
    pub const LEN: usize = 32 + 32 + 32 + 32 + 32 + 32 + 32 + 32 + 1 + 1 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct TraderMarketBalance {
    pub market_config: Pubkey,
    pub trader: Pubkey,
    pub available_base: u64,
    pub available_quote: u64,
    pub bump: u8,
}

impl TraderMarketBalance {
    pub const LEN: usize = 32 + 32 + 8 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct SettlementReceipt {
    pub market_config: Pubkey,
    pub settlement_id: u64,
    pub buyer: Pubkey,
    pub seller: Pubkey,
    pub buyer_order_hash: [u8; 32],
    pub seller_order_hash: [u8; 32],
    pub base_amount: u64,
    pub quote_amount: u64,
    pub bump: u8,
}

impl SettlementReceipt {
    pub const LEN: usize = 32 + 8 + 32 + 32 + 32 + 32 + 8 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct OrderFillState {
    pub market_config: Pubkey,
    pub order_hash: [u8; 32],
    pub filled_quantity: u64,
    pub cancelled: bool,
    pub bump: u8,
}

impl OrderFillState {
    pub const LEN: usize = 32 + 32 + 8 + 1 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}
