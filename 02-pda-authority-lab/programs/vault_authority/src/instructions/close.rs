use anchor_lang::prelude::*;

use crate::{error::ErrorCode, VaultConfig, VAULT_SEED};

#[derive(Accounts)]
pub struct CloseVault<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [VAULT_SEED, user.key().as_ref()],
        bump = vault_config.bump,
        has_one = user,
        close = user
    )]
    pub vault_config: Account<'info, VaultConfig>,
}

pub fn close_vault_handler(ctx: Context<CloseVault>) -> Result<()> {
    require!(
        ctx.accounts.vault_config.total_deposited_lamports == 0,
        ErrorCode::VaultNotEmpty
    );

    msg!("Closed vault config: {}", ctx.accounts.vault_config.key());
    Ok(())
}
