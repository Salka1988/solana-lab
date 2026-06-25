use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::Token2022,
    token_2022_extensions::transfer_fee::{transfer_fee_set, TransferFeeSetTransferFee},
};

use crate::{Token2022MintConfig, MINT_AUTHORITY_SEED, TOKEN_2022_MINT_CONFIG_SEED};

#[derive(Accounts)]
pub struct SetTransferFeeConfig<'info> {
    pub admin: Signer<'info>,

    #[account(
        seeds = [TOKEN_2022_MINT_CONFIG_SEED, mint.key().as_ref()],
        bump,
        has_one = admin,
        has_one = mint,
    )]
    pub mint_config: Account<'info, Token2022MintConfig>,

    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump = mint_config.mint_authority_bump
    )]
    /// CHECK: PDA transfer fee config authority only; no data read or written.
    pub mint_authority: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Token-2022 mint account validated by Token-2022 CPI and config PDA.
    pub mint: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn set_transfer_fee_config_handler(
    ctx: Context<SetTransferFeeConfig>,
    transfer_fee_basis_points: u16,
    maximum_fee: u64,
) -> Result<()> {
    let mint_authority_bump = [ctx.accounts.mint_config.mint_authority_bump];
    let mint_authority_seeds: &[&[u8]] = &[MINT_AUTHORITY_SEED, &mint_authority_bump];
    let signer_seeds = &[mint_authority_seeds];

    transfer_fee_set(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferFeeSetTransferFee {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                authority: ctx.accounts.mint_authority.to_account_info(),
            },
            signer_seeds,
        ),
        transfer_fee_basis_points,
        maximum_fee,
    )?;

    msg!(
        "Scheduled transfer fee update: {} basis points, max {}",
        transfer_fee_basis_points,
        maximum_fee
    );

    Ok(())
}
