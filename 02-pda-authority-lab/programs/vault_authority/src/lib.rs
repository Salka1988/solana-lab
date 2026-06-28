pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("21irkKqwRXGWaSMkqL64sVjUbvxYPaGXmkdZX2Wo6xQn");

#[program]
pub mod vault_authority {
    use super::*;

    pub fn initialize_vault(ctx: Context<InitializeVault>, limit_lamports: u64) -> Result<()> {
        initialize::initialize_vault_handler(ctx, limit_lamports)
    }

    pub fn set_vault_limit(ctx: Context<SetVaultLimit>, limit_lamports: u64) -> Result<()> {
        set_limit::set_vault_limit_handler(ctx, limit_lamports)
    }

    pub fn deposit(ctx: Context<Deposit>, amount_lamports: u64) -> Result<()> {
        deposit::deposit_handler(ctx, amount_lamports)
    }

    pub fn withdraw(ctx: Context<Withdraw>, amount_lamports: u64) -> Result<()> {
        withdraw::withdraw_handler(ctx, amount_lamports)
    }

    pub fn close_vault(ctx: Context<CloseVault>) -> Result<()> {
        close::close_vault_handler(ctx)
    }
}
