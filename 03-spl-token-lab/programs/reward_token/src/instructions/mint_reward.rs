use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

use crate::{error::ErrorCode, RewardMintConfig, MINT_AUTHORITY_SEED, REWARD_MINT_CONFIG_SEED};

#[derive(Accounts)]
pub struct MintReward<'info> {
    pub admin: Signer<'info>,

    #[account(
        seeds = [REWARD_MINT_CONFIG_SEED],
        bump,
        has_one = admin,
        constraint = reward_mint_config.reward_mint == reward_mint.key() @ ErrorCode::InvalidRewardMint
    )]
    pub reward_mint_config: Account<'info, RewardMintConfig>,

    #[account(mut)]
    pub reward_mint: Account<'info, Mint>,

    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump = reward_mint_config.mint_authority_bump
    )]
    /// CHECK: PDA authority only; no data read or written.
    pub mint_authority: UncheckedAccount<'info>,

    #[account(
        mut,
        token::mint = reward_mint
    )]
    pub recipient_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn mint_reward_handler(ctx: Context<MintReward>, amount: u64) -> Result<()> {
    let signer_seeds: &[&[&[u8]]] = &[&[
        MINT_AUTHORITY_SEED,
        &[ctx.accounts.reward_mint_config.mint_authority_bump],
    ]];

    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            MintTo {
                mint: ctx.accounts.reward_mint.to_account_info(),
                to: ctx.accounts.recipient_ata.to_account_info(),
                authority: ctx.accounts.mint_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )?;

    msg!("Minted reward tokens: {}", amount);
    Ok(())
}
