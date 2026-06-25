use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction},
};
use anchor_spl::token_2022::{
    initialize_account3, mint_to_checked,
    spl_token_2022::{
        extension::ExtensionType,
        pod::PodAccount,
        state::{Account as TokenAccount, PackedSizeOf},
    },
    InitializeAccount3, MintToChecked, Token2022,
};

use crate::{Token2022MintConfig, MINT_AUTHORITY_SEED, TOKEN_2022_MINT_CONFIG_SEED};

#[derive(Accounts)]
pub struct MintToUser<'info> {
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
    /// CHECK: PDA authority only; no data read or written.
    pub mint_authority: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Token-2022 mint account validated by Token-2022 CPI and config PDA.
    pub mint: UncheckedAccount<'info>,

    /// CHECK: Token account owner only; no data read or written.
    pub user: UncheckedAccount<'info>,

    #[account(mut)]
    pub user_token_account: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn mint_to_user_handler(ctx: Context<MintToUser>, amount: u64) -> Result<()> {
    let required_account_extensions =
        ExtensionType::get_required_init_account_extensions(&[ExtensionType::TransferFeeConfig]);
    let token_account_space =
        ExtensionType::try_calculate_account_len::<PodAccount>(&required_account_extensions)?;
    let rent_lamports = Rent::get()?.minimum_balance(token_account_space);

    if token_account_space <= TokenAccount::SIZE_OF {
        return Err(ProgramError::InvalidAccountData.into());
    }

    invoke(
        &system_instruction::create_account(
            ctx.accounts.admin.key,
            ctx.accounts.user_token_account.key,
            rent_lamports,
            token_account_space as u64,
            ctx.accounts.token_program.key,
        ),
        &[
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.user_token_account.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
    )?;

    initialize_account3(CpiContext::new(
        ctx.accounts.token_program.key(),
        InitializeAccount3 {
            account: ctx.accounts.user_token_account.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            authority: ctx.accounts.user.to_account_info(),
        },
    ))?;

    let mint_authority_bump = [ctx.accounts.mint_config.mint_authority_bump];
    let mint_authority_seeds: &[&[u8]] = &[MINT_AUTHORITY_SEED, &mint_authority_bump];
    let signer_seeds = &[mint_authority_seeds];

    mint_to_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            MintToChecked {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.mint_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
        ctx.accounts.mint_config.decimals,
    )?;

    msg!(
        "Minted {} tokens to Token-2022 account {}",
        amount,
        ctx.accounts.user_token_account.key()
    );

    Ok(())
}
