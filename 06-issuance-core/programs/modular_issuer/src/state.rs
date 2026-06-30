use anchor_lang::prelude::*;

#[account]
pub struct ProtocolConfig {
    pub admin: Pubkey,
    pub pending_admin: Pubkey,
    pub stablecoin_mint: Pubkey,
    pub global_supply_cap: u64,
    pub paused: bool,
    pub bump: u8,
}

impl ProtocolConfig {
    pub const LEN: usize = 32 + 32 + 32 + 8 + 1 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct StablecoinMintConfig {
    pub protocol_config: Pubkey,
    pub mint: Pubkey,
    pub mint_authority: Pubkey,
    pub supply_stats: Pubkey,
    pub decimals: u8,
    pub mint_authority_bump: u8,
    pub bump: u8,
}

impl StablecoinMintConfig {
    pub const LEN: usize = 32 + 32 + 32 + 32 + 1 + 1 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct GlobalSupplyStats {
    pub protocol_config: Pubkey,
    pub mint: Pubkey,
    pub current_supply: u64,
    pub total_minted: u64,
    pub total_burned: u64,
    pub bump: u8,
}

impl GlobalSupplyStats {
    pub const LEN: usize = 32 + 32 + 8 + 8 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct IssuerConfig {
    pub protocol_config: Pubkey,
    pub authority: Pubkey,
    pub stats: Pubkey,
    pub mint_limit: u64,
    pub paused: bool,
    pub bump: u8,
}

impl IssuerConfig {
    pub const LEN: usize = 32 + 32 + 32 + 8 + 1 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct IssuerStats {
    pub protocol_config: Pubkey,
    pub issuer_config: Pubkey,
    pub authority: Pubkey,
    pub current_outstanding: u64,
    pub total_minted: u64,
    pub total_burned: u64,
    pub bump: u8,
}

impl IssuerStats {
    pub const LEN: usize = 32 + 32 + 32 + 8 + 8 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}
