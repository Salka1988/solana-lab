use crate::{
    constants::{ISSUER_SEED, ISSUER_STATS_SEED, PROTOCOL_SEED},
    error::ErrorCode,
    state::{IssuerConfig, IssuerStats, ProtocolConfig},
};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct RegisterIssuer<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        has_one = admin
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        init,
        payer = admin,
        space = IssuerConfig::SPACE,
        seeds = [ISSUER_SEED, issuer_authority.key().as_ref()],
        bump
    )]
    pub issuer_config: Account<'info, IssuerConfig>,

    #[account(
        init,
        payer = admin,
        space = IssuerStats::SPACE,
        seeds = [ISSUER_STATS_SEED, issuer_authority.key().as_ref()],
        bump
    )]
    pub issuer_stats: Account<'info, IssuerStats>,

    /// CHECK: Issuer authority is stored as an identity key; it does not need to sign registration.
    pub issuer_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn register_issuer_handler(ctx: Context<RegisterIssuer>, mint_limit: u64) -> Result<()> {
    require!(mint_limit > 0, ErrorCode::InvalidIssuerMintLimit);

    let protocol_config = ctx.accounts.protocol_config.key();
    let issuer_config_key = ctx.accounts.issuer_config.key();
    let issuer_stats_key = ctx.accounts.issuer_stats.key();
    let issuer_authority = ctx.accounts.issuer_authority.key();

    let issuer_config = &mut ctx.accounts.issuer_config;
    issuer_config.protocol_config = protocol_config;
    issuer_config.authority = issuer_authority;
    issuer_config.stats = issuer_stats_key;
    issuer_config.mint_limit = mint_limit;
    issuer_config.paused = false;
    issuer_config.bump = ctx.bumps.issuer_config;

    let issuer_stats = &mut ctx.accounts.issuer_stats;
    issuer_stats.protocol_config = protocol_config;
    issuer_stats.issuer_config = issuer_config_key;
    issuer_stats.authority = issuer_authority;
    issuer_stats.current_outstanding = 0;
    issuer_stats.total_minted = 0;
    issuer_stats.total_burned = 0;
    issuer_stats.bump = ctx.bumps.issuer_stats;

    msg!(
        "Registered issuer {} with mint limit {}",
        issuer_config.authority,
        mint_limit
    );

    Ok(())
}
