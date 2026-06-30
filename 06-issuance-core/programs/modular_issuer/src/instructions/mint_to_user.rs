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

use crate::{
    constants::{
        ISSUER_SEED, ISSUER_STATS_SEED, MINT_AUTHORITY_SEED, PROTOCOL_SEED, STABLECOIN_MINT_SEED,
        SUPPLY_STATS_SEED,
    },
    error::ErrorCode,
    state::{GlobalSupplyStats, IssuerConfig, IssuerStats, ProtocolConfig, StablecoinMintConfig},
};

#[derive(Accounts)]
pub struct MintToUser<'info> {
    pub issuer_authority: Signer<'info>,

    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        seeds = [STABLECOIN_MINT_SEED],
        bump = mint_config.bump,
        constraint = mint_config.protocol_config == protocol_config.key(),
        constraint = mint_config.mint == mint.key(),
        constraint = mint_config.mint_authority == mint_authority.key(),
        constraint = mint_config.supply_stats == supply_stats.key()
    )]
    pub mint_config: Account<'info, StablecoinMintConfig>,

    #[account(
        mut,
        seeds = [SUPPLY_STATS_SEED],
        bump = supply_stats.bump,
        constraint = supply_stats.protocol_config == protocol_config.key(),
        constraint = supply_stats.mint == mint.key()
    )]
    pub supply_stats: Account<'info, GlobalSupplyStats>,

    #[account(
        mut,
        seeds = [ISSUER_SEED, issuer_authority.key().as_ref()],
        bump = issuer_config.bump,
        constraint = issuer_config.protocol_config == protocol_config.key(),
        constraint = issuer_config.authority == issuer_authority.key(),
        constraint = issuer_config.stats == issuer_stats.key()
    )]
    pub issuer_config: Account<'info, IssuerConfig>,

    #[account(
        mut,
        seeds = [ISSUER_STATS_SEED, issuer_authority.key().as_ref()],
        bump = issuer_stats.bump,
        constraint = issuer_stats.protocol_config == protocol_config.key(),
        constraint = issuer_stats.issuer_config == issuer_config.key(),
        constraint = issuer_stats.authority == issuer_authority.key()
    )]
    pub issuer_stats: Account<'info, IssuerStats>,

    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump = mint_config.mint_authority_bump
    )]
    /// CHECK: PDA authority only; no account data is read or written.
    pub mint_authority: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Token-2022 mint account validated by Token-2022 CPI and mint config.
    pub mint: UncheckedAccount<'info>,

    /// CHECK: Token account owner only; no data is read or written.
    pub user: UncheckedAccount<'info>,

    #[account(mut)]
    pub user_token_account: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn mint_to_user_handler(ctx: Context<MintToUser>, amount: u64) -> Result<()> {
    require!(
        !ctx.accounts.protocol_config.paused,
        ErrorCode::ProtocolPaused
    );
    require!(!ctx.accounts.issuer_config.paused, ErrorCode::IssuerPaused);

    let new_issuer_outstanding = ctx
        .accounts
        .issuer_stats
        .current_outstanding
        .checked_add(amount)
        .ok_or(ErrorCode::IssuerLimitExceeded)?;
    require!(
        new_issuer_outstanding <= ctx.accounts.issuer_config.mint_limit,
        ErrorCode::IssuerLimitExceeded
    );

    let new_current_supply = ctx
        .accounts
        .supply_stats
        .current_supply
        .checked_add(amount)
        .ok_or(ErrorCode::GlobalSupplyCapExceeded)?;
    require!(
        new_current_supply <= ctx.accounts.protocol_config.global_supply_cap,
        ErrorCode::GlobalSupplyCapExceeded
    );

    let token_account_space = ExtensionType::try_calculate_account_len::<PodAccount>(&[])?;
    let rent_lamports = Rent::get()?.minimum_balance(token_account_space);

    if token_account_space != TokenAccount::SIZE_OF {
        return Err(ProgramError::InvalidAccountData.into());
    }

    invoke(
        &system_instruction::create_account(
            ctx.accounts.issuer_authority.key,
            ctx.accounts.user_token_account.key,
            rent_lamports,
            token_account_space as u64,
            ctx.accounts.token_program.key,
        ),
        &[
            ctx.accounts.issuer_authority.to_account_info(),
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

    let issuer_stats = &mut ctx.accounts.issuer_stats;
    issuer_stats.current_outstanding = new_issuer_outstanding;
    issuer_stats.total_minted = issuer_stats
        .total_minted
        .checked_add(amount)
        .ok_or(ErrorCode::IssuerLimitExceeded)?;

    let supply_stats = &mut ctx.accounts.supply_stats;
    supply_stats.current_supply = new_current_supply;
    supply_stats.total_minted = supply_stats
        .total_minted
        .checked_add(amount)
        .ok_or(ErrorCode::GlobalSupplyCapExceeded)?;

    msg!(
        "Issuer {} minted {} tokens to {}",
        ctx.accounts.issuer_authority.key(),
        amount,
        ctx.accounts.user_token_account.key()
    );

    Ok(())
}
