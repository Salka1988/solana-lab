use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction},
};

use crate::{error::ErrorCode, VaultConfig, VAULT_SEED};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [VAULT_SEED, user.key().as_ref()],
        bump = vault_config.bump,
        has_one = user
    )]
    pub vault_config: Account<'info, VaultConfig>,

    pub system_program: Program<'info, System>,
}

pub fn deposit_handler(ctx: Context<Deposit>, amount_lamports: u64) -> Result<()> {
    let new_total = ctx
        .accounts
        .vault_config
        .total_deposited_lamports
        .checked_add(amount_lamports)
        .ok_or(ErrorCode::DepositOverflow)?;

    require!(
        new_total <= ctx.accounts.vault_config.limit_lamports,
        ErrorCode::VaultLimitExceeded
    );

    let vault_config_key = ctx.accounts.vault_config.key();
    let transfer_ix =
        system_instruction::transfer(&ctx.accounts.user.key(), &vault_config_key, amount_lamports);

    invoke(
        &transfer_ix,
        &[
            ctx.accounts.user.to_account_info(),
            ctx.accounts.vault_config.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
    )?;

    let vault_config = &mut ctx.accounts.vault_config;
    vault_config.total_deposited_lamports = new_total;

    msg!(
        "Deposited lamports into vault config: {}",
        vault_config.key()
    );
    Ok(())
}
