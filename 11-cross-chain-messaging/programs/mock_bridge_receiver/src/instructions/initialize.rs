use anchor_lang::prelude::*;

use crate::{constants::*, error::ErrorCode, state::*};

#[derive(Accounts)]
pub struct InitializeBridgeConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = BridgeConfig::SPACE,
        seeds = [BRIDGE_CONFIG_SEED, registered_mint.key().as_ref()],
        bump
    )]
    pub bridge_config: Account<'info, BridgeConfig>,

    /// CHECK: trusted bridge signer identity
    pub bridge_authority: UncheckedAccount<'info>,

    /// CHECK: mint identity for this mock bridge config
    pub registered_mint: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_bridge_config_handler(
    ctx: Context<InitializeBridgeConfig>,
    per_message_limit: u64,
) -> Result<()> {
    require!(per_message_limit > 0, ErrorCode::BridgeLimitExceeded);

    let bridge_config = &mut ctx.accounts.bridge_config;
    bridge_config.admin = ctx.accounts.admin.key();
    bridge_config.bridge_authority = ctx.accounts.bridge_authority.key();
    bridge_config.registered_mint = ctx.accounts.registered_mint.key();
    bridge_config.per_message_limit = per_message_limit;
    bridge_config.bump = ctx.bumps.bridge_config;

    Ok(())
}
