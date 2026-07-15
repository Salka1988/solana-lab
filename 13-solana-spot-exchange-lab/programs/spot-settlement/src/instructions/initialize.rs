use anchor_lang::prelude::*;

use crate::{
    constants::{MARKET_CONFIG_SEED, PROTOCOL_CONFIG_SEED, VAULT_AUTHORITY_SEED},
    error::ErrorCode,
    state::{MarketConfig, ProtocolConfig},
};

#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = ProtocolConfig::SPACE,
        seeds = [PROTOCOL_CONFIG_SEED],
        bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_protocol_handler(ctx: Context<InitializeProtocol>) -> Result<()> {
    let protocol_config = &mut ctx.accounts.protocol_config;
    protocol_config.admin = ctx.accounts.admin.key();
    protocol_config.bump = ctx.bumps.protocol_config;
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeMarket<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [PROTOCOL_CONFIG_SEED],
        bump = protocol_config.bump,
        has_one = admin @ ErrorCode::UnauthorizedAdmin
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        init,
        payer = admin,
        space = MarketConfig::SPACE,
        seeds = [
            MARKET_CONFIG_SEED,
            base_mint.key().as_ref(),
            quote_mint.key().as_ref()
        ],
        bump
    )]
    pub market_config: Account<'info, MarketConfig>,

    /// CHECK: mint identity only in scaffold step
    pub base_mint: UncheckedAccount<'info>,

    /// CHECK: mint identity only in scaffold step
    pub quote_mint: UncheckedAccount<'info>,

    /// CHECK: token vault identity validated in later deposit/settlement steps
    pub base_vault: UncheckedAccount<'info>,

    /// CHECK: token vault identity validated in later deposit/settlement steps
    pub quote_vault: UncheckedAccount<'info>,

    /// CHECK: PDA authority identity derived and stored
    #[account(
        seeds = [VAULT_AUTHORITY_SEED, market_config.key().as_ref()],
        bump
    )]
    pub vault_authority: UncheckedAccount<'info>,

    /// CHECK: trusted off-chain settlement signer identity
    pub settlement_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_market_handler(ctx: Context<InitializeMarket>) -> Result<()> {
    require_keys_neq!(
        ctx.accounts.base_mint.key(),
        ctx.accounts.quote_mint.key(),
        ErrorCode::SameMarketMints
    );
    require_keys_neq!(
        ctx.accounts.base_vault.key(),
        ctx.accounts.quote_vault.key(),
        ErrorCode::SameMarketVaults
    );

    let market_config = &mut ctx.accounts.market_config;
    market_config.protocol_config = ctx.accounts.protocol_config.key();
    market_config.admin = ctx.accounts.admin.key();
    market_config.settlement_authority = ctx.accounts.settlement_authority.key();
    market_config.base_mint = ctx.accounts.base_mint.key();
    market_config.quote_mint = ctx.accounts.quote_mint.key();
    market_config.base_vault = ctx.accounts.base_vault.key();
    market_config.quote_vault = ctx.accounts.quote_vault.key();
    market_config.vault_authority = ctx.accounts.vault_authority.key();
    market_config.vault_authority_bump = ctx.bumps.vault_authority;
    market_config.paused = false;
    market_config.bump = ctx.bumps.market_config;

    Ok(())
}
