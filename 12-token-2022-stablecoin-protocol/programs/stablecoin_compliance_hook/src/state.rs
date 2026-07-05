use anchor_lang::prelude::*;

#[account]
pub struct ComplianceConfig {
    pub admin: Pubkey,
    pub mint: Pubkey,
    pub max_transfer_amount: u64,
    pub daily_transfer_limit: u64,
    pub paused: bool,
    pub bump: u8,
}

impl ComplianceConfig {
    pub const LEN: usize = 32 + 32 + 8 + 8 + 1 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct UserCompliance {
    pub config: Pubkey,
    pub mint: Pubkey,
    pub user: Pubkey,
    pub allowlisted: bool,
    pub blocked: bool,
    pub issuer_active: bool,
    pub transferred_today: u64,
    pub current_day: i64,
    pub bump: u8,
}

impl UserCompliance {
    pub const LEN: usize = 32 + 32 + 32 + 1 + 1 + 1 + 8 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}
