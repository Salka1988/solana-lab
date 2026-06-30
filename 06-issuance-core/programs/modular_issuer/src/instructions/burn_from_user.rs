use anchor_lang::prelude::*;
use anchor_spl::token_2022::{burn_checked, BurnChecked, Token2022};

use crate::{
    constants::{
        ISSUER_SEED, ISSUER_STATS_SEED, MINT_AUTHORITY_SEED, PROTOCOL_SEED, STABLECOIN_MINT_SEED,
        SUPPLY_STATS_SEED,
    },
    error::ErrorCode,
    state::{GlobalSupplyStats, IssuerConfig, IssuerStats, ProtocolConfig, StablecoinMintConfig},
};

#[derive(Accounts)]
pub struct BurnFromUser<'info> {
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
    /// CHECK: PDA authority only; signs as Token-2022 permanent delegate.
    pub mint_authority: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Token-2022 mint account validated by Token-2022 CPI and mint config.
    pub mint: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: Token-2022 account validated by Token-2022 CPI.
    pub user_token_account: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn burn_from_user_handler(ctx: Context<BurnFromUser>, amount: u64) -> Result<()> {
    require!(
        !ctx.accounts.protocol_config.paused,
        ErrorCode::ProtocolPaused
    );
    require!(!ctx.accounts.issuer_config.paused, ErrorCode::IssuerPaused);

    let new_issuer_outstanding = ctx
        .accounts
        .issuer_stats
        .current_outstanding
        .checked_sub(amount)
        .ok_or(ErrorCode::BurnAmountExceedsOutstanding)?;
    let new_current_supply = ctx
        .accounts
        .supply_stats
        .current_supply
        .checked_sub(amount)
        .ok_or(ErrorCode::BurnAmountExceedsOutstanding)?;

    let mint_authority_bump = [ctx.accounts.mint_config.mint_authority_bump];
    let mint_authority_seeds: &[&[u8]] = &[MINT_AUTHORITY_SEED, &mint_authority_bump];
    let signer_seeds = &[mint_authority_seeds];

    burn_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            BurnChecked {
                mint: ctx.accounts.mint.to_account_info(),
                from: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.mint_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
        ctx.accounts.mint_config.decimals,
    )?;

    let issuer_stats = &mut ctx.accounts.issuer_stats;
    issuer_stats.current_outstanding = new_issuer_outstanding;
    issuer_stats.total_burned = issuer_stats
        .total_burned
        .checked_add(amount)
        .ok_or(ErrorCode::BurnAmountExceedsOutstanding)?;

    let supply_stats = &mut ctx.accounts.supply_stats;
    supply_stats.current_supply = new_current_supply;
    supply_stats.total_burned = supply_stats
        .total_burned
        .checked_add(amount)
        .ok_or(ErrorCode::BurnAmountExceedsOutstanding)?;

    msg!(
        "Issuer {} burned {} tokens from {}",
        ctx.accounts.issuer_authority.key(),
        amount,
        ctx.accounts.user_token_account.key()
    );

    Ok(())
}
