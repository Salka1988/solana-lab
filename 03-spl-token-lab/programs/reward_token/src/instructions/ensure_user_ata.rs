use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::{self, get_associated_token_address, AssociatedToken, Create as CreateAta},
    token::{Mint, Token},
};

use crate::{error::ErrorCode, RewardMintConfig, REWARD_MINT_CONFIG_SEED};

#[derive(Accounts)]
pub struct EnsureUserAta<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [REWARD_MINT_CONFIG_SEED],
        bump,
        constraint = reward_mint_config.reward_mint == reward_mint.key() @ ErrorCode::InvalidRewardMint
    )]
    pub reward_mint_config: Account<'info, RewardMintConfig>,

    pub reward_mint: Account<'info, Mint>,

    #[account(mut)]
    /// CHECK: ATA program creates or verifies this account.
    pub user_ata: UncheckedAccount<'info>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn ensure_user_ata_handler(ctx: Context<EnsureUserAta>) -> Result<()> {
    let expected_ata =
        get_associated_token_address(&ctx.accounts.user.key(), &ctx.accounts.reward_mint.key());

    require_keys_eq!(
        ctx.accounts.user_ata.key(),
        expected_ata,
        ErrorCode::InvalidAssociatedTokenAccount
    );

    if !ctx.accounts.user_ata.to_account_info().data_is_empty() {
        return Ok(());
    }

    associated_token::create_idempotent(CpiContext::new(
        ctx.accounts.associated_token_program.key(),
        CreateAta {
            payer: ctx.accounts.user.to_account_info(),
            associated_token: ctx.accounts.user_ata.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
            mint: ctx.accounts.reward_mint.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            token_program: ctx.accounts.token_program.to_account_info(),
        },
    ))?;

    msg!("Ensured user ATA: {}", ctx.accounts.user_ata.key());
    Ok(())
}
