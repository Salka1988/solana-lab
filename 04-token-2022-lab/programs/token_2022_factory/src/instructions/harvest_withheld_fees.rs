use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::Token2022,
    token_2022_extensions::transfer_fee::{
        harvest_withheld_tokens_to_mint, HarvestWithheldTokensToMint,
    },
};

use crate::{Token2022MintConfig, TOKEN_2022_MINT_CONFIG_SEED};

#[derive(Accounts)]
pub struct HarvestWithheldFees<'info> {
    #[account(
        seeds = [TOKEN_2022_MINT_CONFIG_SEED, mint.key().as_ref()],
        bump,
        has_one = mint,
    )]
    pub mint_config: Account<'info, Token2022MintConfig>,

    #[account(mut)]
    /// CHECK: Token-2022 mint account validated by Token-2022 CPI and config PDA.
    pub mint: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn harvest_withheld_fees_handler<'info>(
    ctx: Context<'info, HarvestWithheldFees<'info>>,
) -> Result<()> {
    let sources = ctx.remaining_accounts.to_vec();

    harvest_withheld_tokens_to_mint(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            HarvestWithheldTokensToMint {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        sources,
    )?;

    msg!(
        "Harvested withheld fees from {} token accounts to mint {}",
        ctx.remaining_accounts.len(),
        ctx.accounts.mint.key()
    );

    Ok(())
}
