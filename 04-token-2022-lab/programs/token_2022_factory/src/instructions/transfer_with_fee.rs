use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::Token2022,
    token_2022_extensions::transfer_fee::{transfer_checked_with_fee, TransferCheckedWithFee},
};

use crate::{Token2022MintConfig, TOKEN_2022_MINT_CONFIG_SEED};

#[derive(Accounts)]
pub struct TransferWithFee<'info> {
    pub owner: Signer<'info>,

    #[account(
        seeds = [TOKEN_2022_MINT_CONFIG_SEED, mint.key().as_ref()],
        bump,
        has_one = mint,
    )]
    pub mint_config: Account<'info, Token2022MintConfig>,

    /// CHECK: Token-2022 mint account validated by Token-2022 CPI and config PDA.
    pub mint: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Token-2022 source account validated by Token-2022 CPI.
    pub source: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Token-2022 destination account validated by Token-2022 CPI.
    pub destination: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn transfer_with_fee_handler(
    ctx: Context<TransferWithFee>,
    amount: u64,
    fee: u64,
) -> Result<()> {
    transfer_checked_with_fee(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferCheckedWithFee {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                source: ctx.accounts.source.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                destination: ctx.accounts.destination.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.mint_config.decimals,
        fee,
    )?;

    msg!("Transferred {} tokens with {} withheld fee", amount, fee);

    Ok(())
}
