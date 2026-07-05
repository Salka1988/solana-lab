use anchor_lang::prelude::*;

use crate::{constants::*, state::*};

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

    #[account(
        init,
        payer = admin,
        space = RedemptionVault::SPACE,
        seeds = [REDEMPTION_VAULT_SEED, protocol_config.key().as_ref()],
        bump
    )]
    pub redemption_vault: Account<'info, RedemptionVault>,

    pub system_program: Program<'info, System>,
}

pub fn initialize_protocol_handler(ctx: Context<InitializeProtocol>) -> Result<()> {
    let protocol_config = &mut ctx.accounts.protocol_config;
    protocol_config.admin = ctx.accounts.admin.key();
    protocol_config.pending_admin = Pubkey::default();
    protocol_config.redemptions_paused = false;
    protocol_config.next_request_id = 0;
    protocol_config.next_admin_action_id = 0;
    protocol_config.bump = ctx.bumps.protocol_config;

    let redemption_vault = &mut ctx.accounts.redemption_vault;
    redemption_vault.protocol_config = protocol_config.key();
    redemption_vault.outstanding_amount = 0;
    redemption_vault.total_requested = 0;
    redemption_vault.total_completed = 0;
    redemption_vault.total_cancelled = 0;
    redemption_vault.total_rejected = 0;
    redemption_vault.bump = ctx.bumps.redemption_vault;

    Ok(())
}
