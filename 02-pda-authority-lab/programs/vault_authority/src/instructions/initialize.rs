use anchor_lang::prelude::*;

use crate::{VaultConfig, VAULT_SEED};

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        init,
        payer = user,
        space = 8 + VaultConfig::LEN,
        seeds = [VAULT_SEED, user.key().as_ref()],
        bump
    )]
    pub vault_config: Account<'info, VaultConfig>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_vault_handler(ctx: Context<InitializeVault>, limit_lamports: u64) -> Result<()> {
    let vault_config = &mut ctx.accounts.vault_config;

    vault_config.user = ctx.accounts.user.key();
    vault_config.bump = ctx.bumps.vault_config;
    vault_config.limit_lamports = limit_lamports;
    vault_config.total_deposited_lamports = 0;

    msg!("Initialized vault config: {}", vault_config.key());
    Ok(())
}
