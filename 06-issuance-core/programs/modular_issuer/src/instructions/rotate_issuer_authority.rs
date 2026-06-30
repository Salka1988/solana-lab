use crate::{
    constants::{ISSUER_SEED, ISSUER_STATS_SEED, PROTOCOL_SEED},
    state::{IssuerConfig, IssuerStats, ProtocolConfig},
};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct RotateIssuerAuthority<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        has_one = admin
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [ISSUER_SEED, current_authority.key().as_ref()],
        bump = current_issuer_config.bump,
        constraint = current_issuer_config.protocol_config == protocol_config.key(),
        constraint = current_issuer_config.authority == current_authority.key(),
        constraint = current_issuer_config.stats == current_issuer_stats.key()
    )]
    pub current_issuer_config: Account<'info, IssuerConfig>,

    #[account(
        mut,
        seeds = [ISSUER_STATS_SEED, current_authority.key().as_ref()],
        bump = current_issuer_stats.bump,
        constraint = current_issuer_stats.protocol_config == protocol_config.key(),
        constraint = current_issuer_stats.issuer_config == current_issuer_config.key(),
        constraint = current_issuer_stats.authority == current_authority.key()
    )]
    pub current_issuer_stats: Account<'info, IssuerStats>,

    #[account(
        init,
        payer = admin,
        space = IssuerConfig::SPACE,
        seeds = [ISSUER_SEED, new_authority.key().as_ref()],
        bump
    )]
    pub new_issuer_config: Account<'info, IssuerConfig>,

    #[account(
        init,
        payer = admin,
        space = IssuerStats::SPACE,
        seeds = [ISSUER_STATS_SEED, new_authority.key().as_ref()],
        bump
    )]
    pub new_issuer_stats: Account<'info, IssuerStats>,

    /// CHECK: Current issuer identity key used for PDA validation.
    pub current_authority: UncheckedAccount<'info>,

    /// CHECK: New issuer identity key used for new PDA derivation.
    pub new_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn rotate_issuer_authority_handler(ctx: Context<RotateIssuerAuthority>) -> Result<()> {
    let protocol_config = ctx.accounts.protocol_config.key();
    let new_issuer_config_key = ctx.accounts.new_issuer_config.key();
    let new_issuer_stats_key = ctx.accounts.new_issuer_stats.key();
    let new_authority = ctx.accounts.new_authority.key();
    let old_authority = ctx.accounts.current_authority.key();

    let mint_limit = ctx.accounts.current_issuer_config.mint_limit;
    let paused = ctx.accounts.current_issuer_config.paused;
    let current_outstanding = ctx.accounts.current_issuer_stats.current_outstanding;
    let total_minted = ctx.accounts.current_issuer_stats.total_minted;
    let total_burned = ctx.accounts.current_issuer_stats.total_burned;

    let new_issuer_config = &mut ctx.accounts.new_issuer_config;
    new_issuer_config.protocol_config = protocol_config;
    new_issuer_config.authority = new_authority;
    new_issuer_config.stats = new_issuer_stats_key;
    new_issuer_config.mint_limit = mint_limit;
    new_issuer_config.paused = paused;
    new_issuer_config.bump = ctx.bumps.new_issuer_config;

    let new_issuer_stats = &mut ctx.accounts.new_issuer_stats;
    new_issuer_stats.protocol_config = protocol_config;
    new_issuer_stats.issuer_config = new_issuer_config_key;
    new_issuer_stats.authority = new_authority;
    new_issuer_stats.current_outstanding = current_outstanding;
    new_issuer_stats.total_minted = total_minted;
    new_issuer_stats.total_burned = total_burned;
    new_issuer_stats.bump = ctx.bumps.new_issuer_stats;

    let current_issuer_config = &mut ctx.accounts.current_issuer_config;
    current_issuer_config.mint_limit = 0;
    current_issuer_config.paused = true;

    msg!(
        "Rotated issuer authority from {} to {}",
        old_authority,
        new_authority
    );

    Ok(())
}
