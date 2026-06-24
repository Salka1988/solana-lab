use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::{error::ErrorCode, RewardMintConfig, REWARD_MINT_CONFIG_SEED};

#[derive(Accounts)]
pub struct TransferReward<'info> {
    pub sender: Signer<'info>,

    #[account(
        seeds = [REWARD_MINT_CONFIG_SEED],
        bump,
        constraint = reward_mint_config.reward_mint == reward_mint.key() @ ErrorCode::InvalidRewardMint
    )]
    pub reward_mint_config: Account<'info, RewardMintConfig>,

    pub reward_mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = reward_mint,
        token::authority = sender
    )]
    pub sender_ata: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = reward_mint
    )]
    pub recipient_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn transfer_reward_handler(ctx: Context<TransferReward>, amount: u64) -> Result<()> {
    token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            Transfer {
                from: ctx.accounts.sender_ata.to_account_info(),
                to: ctx.accounts.recipient_ata.to_account_info(),
                authority: ctx.accounts.sender.to_account_info(),
            },
        ),
        amount,
    )?;

    msg!("Transferred reward tokens: {}", amount);
    Ok(())
}
