use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

use crate::{RewardMintConfig, MINT_AUTHORITY_SEED, REWARD_MINT_CONFIG_SEED};

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct InitializeRewardMint<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + RewardMintConfig::LEN,
        seeds = [REWARD_MINT_CONFIG_SEED],
        bump
    )]
    pub reward_mint_config: Account<'info, RewardMintConfig>,

    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump
    )]
    /// CHECK: PDA authority only; no data read or written.
    pub mint_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = admin,
        mint::decimals = decimals,
        mint::authority = mint_authority,
        mint::freeze_authority = mint_authority
    )]
    pub reward_mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn initialize_reward_mint_handler(
    ctx: Context<InitializeRewardMint>,
    decimals: u8,
) -> Result<()> {
    let reward_mint_config = &mut ctx.accounts.reward_mint_config;

    reward_mint_config.admin = ctx.accounts.admin.key();
    reward_mint_config.reward_mint = ctx.accounts.reward_mint.key();
    reward_mint_config.mint_authority_bump = ctx.bumps.mint_authority;
    reward_mint_config.decimals = decimals;

    msg!(
        "Initialized reward mint: {}",
        reward_mint_config.reward_mint
    );
    Ok(())
}
