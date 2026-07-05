use crate::{constants::PROTOCOL_SEED, error::ErrorCode, state::ProtocolConfig};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = ProtocolConfig::SPACE,
        seeds = [PROTOCOL_SEED],
        bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_protocol_handler(
    ctx: Context<InitializeProtocol>,
    global_supply_cap: u64,
) -> Result<()> {
    require!(global_supply_cap > 0, ErrorCode::InvalidGlobalSupplyCap);

    let protocol_config = &mut ctx.accounts.protocol_config;
    protocol_config.admin = ctx.accounts.admin.key();
    protocol_config.pending_admin = Pubkey::default();
    protocol_config.stablecoin_mint = Pubkey::default();
    protocol_config.global_supply_cap = global_supply_cap;
    protocol_config.paused = false;
    protocol_config.bump = ctx.bumps.protocol_config;

    msg!(
        "Initialized stablecoin issuer protocol with global supply cap {}",
        global_supply_cap
    );
    Ok(())
}
