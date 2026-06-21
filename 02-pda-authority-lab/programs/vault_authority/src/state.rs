use anchor_lang::prelude::*;

#[account]
pub struct VaultConfig {
    pub user: Pubkey,
    pub bump: u8,
    pub limit_lamports: u64,
    pub total_deposited_lamports: u64,
}

impl VaultConfig {
    pub const LEN: usize = 32 + 1 + 8 + 8;
}

#[account]
pub struct UserVault {
    pub user: Pubkey,
    pub vault_config: Pubkey,
    pub bump: u8,
    pub balance_lamports: u64,
}

impl UserVault {
    pub const LEN: usize = 32 + 32 + 1 + 8;
}
