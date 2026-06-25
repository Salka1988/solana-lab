use anchor_lang::prelude::*;
use anchor_spl::token_2022::{burn_checked, BurnChecked, Token2022};

use crate::{Token2022MintConfig, TOKEN_2022_MINT_CONFIG_SEED};

#[derive(Accounts)]
pub struct BurnFromUser<'info> {
    pub owner: Signer<'info>,

    #[account(
        seeds = [TOKEN_2022_MINT_CONFIG_SEED, mint.key().as_ref()],
        bump,
        has_one = mint,
    )]
    pub mint_config: Account<'info, Token2022MintConfig>,

    #[account(mut)]
    /// CHECK: Token-2022 mint account validated by Token-2022 CPI and config PDA.
    pub mint: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Token-2022 account validated by Token-2022 CPI.
    pub user_token_account: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn burn_from_user_handler(ctx: Context<BurnFromUser>, amount: u64) -> Result<()> {
    burn_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            BurnChecked {
                mint: ctx.accounts.mint.to_account_info(),
                from: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.owner.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.mint_config.decimals,
    )?;

    msg!(
        "Burned {} tokens from Token-2022 account {}",
        amount,
        ctx.accounts.user_token_account.key()
    );

    Ok(())
}
