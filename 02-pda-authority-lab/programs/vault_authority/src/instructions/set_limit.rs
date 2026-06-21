use anchor_lang::prelude::*;

use crate::{VaultConfig, VAULT_SEED};

#[derive(Accounts)]
pub struct SetVaultLimit<'info> {
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [VAULT_SEED, user.key().as_ref()],
        bump = vault_config.bump,
        has_one = user
    )]
    pub vault_config: Account<'info, VaultConfig>,
}

pub fn set_vault_limit_handler(ctx: Context<SetVaultLimit>, limit_lamports: u64) -> Result<()> {
    let vault_config = &mut ctx.accounts.vault_config;

    vault_config.limit_lamports = limit_lamports;

    msg!("Updated vault limit: {}", vault_config.key());
    Ok(())
}
