use anchor_lang::prelude::*;

use crate::{error::ErrorCode, VaultConfig, VAULT_SEED};

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [VAULT_SEED, user.key().as_ref()],
        bump = vault_config.bump,
        has_one = user
    )]
    pub vault_config: Account<'info, VaultConfig>,
}

pub fn withdraw_handler(ctx: Context<Withdraw>, amount_lamports: u64) -> Result<()> {
    let vault_config = &mut ctx.accounts.vault_config;

    require!(
        amount_lamports <= vault_config.total_deposited_lamports,
        ErrorCode::InsufficientVaultBalance
    );

    **vault_config.to_account_info().try_borrow_mut_lamports()? -= amount_lamports;
    **ctx
        .accounts
        .user
        .to_account_info()
        .try_borrow_mut_lamports()? += amount_lamports;

    vault_config.total_deposited_lamports -= amount_lamports;

    msg!(
        "Withdrew lamports from vault config: {}",
        vault_config.key()
    );
    Ok(())
}
