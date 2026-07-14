use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum DepositAsset {
    Base,
    Quote,
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
    pub const LEN: usize = 32 + 32 + 32 + 32 + 32 + 32 + 32 + 1 + 1 + 1;
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
