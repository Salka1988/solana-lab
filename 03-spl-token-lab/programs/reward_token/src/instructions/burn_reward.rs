use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount};

use crate::{error::ErrorCode, RewardMintConfig, REWARD_MINT_CONFIG_SEED};

#[derive(Accounts)]
pub struct BurnReward<'info> {
    pub user: Signer<'info>,

    #[account(
        seeds = [REWARD_MINT_CONFIG_SEED],
        bump,
        constraint = reward_mint_config.reward_mint == reward_mint.key() @ ErrorCode::InvalidRewardMint
    )]
    pub reward_mint_config: Account<'info, RewardMintConfig>,

    #[account(mut)]
    pub reward_mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = reward_mint,
        token::authority = user
    )]
    pub user_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn burn_reward_handler(ctx: Context<BurnReward>, amount: u64) -> Result<()> {
    token::burn(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Burn {
                mint: ctx.accounts.reward_mint.to_account_info(),
                from: ctx.accounts.user_ata.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount,
    )?;

    msg!("Burned reward tokens: {}", amount);
    Ok(())
}
