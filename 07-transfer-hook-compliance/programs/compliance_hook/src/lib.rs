pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("Hook111111111111111111111111111111111111111");

#[program]
pub mod compliance_hook {
    use super::*;

    pub fn initialize_compliance_config(
        ctx: Context<InitializeComplianceConfig>,
        max_transfer_amount: u64,
        daily_transfer_limit: u64,
    ) -> Result<()> {
        admin::initialize_compliance_config_handler(ctx, max_transfer_amount, daily_transfer_limit)
    }

    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        admin::initialize_extra_account_meta_list_handler(ctx)
    }

    pub fn set_user_compliance(
        ctx: Context<SetUserCompliance>,
        allowlisted: bool,
        blocked: bool,
        issuer_active: bool,
    ) -> Result<()> {
        admin::set_user_compliance_handler(ctx, allowlisted, blocked, issuer_active)
    }

    pub fn set_transfer_limits(
        ctx: Context<SetTransferLimits>,
        max_transfer_amount: u64,
        daily_transfer_limit: u64,
    ) -> Result<()> {
        admin::set_transfer_limits_handler(ctx, max_transfer_amount, daily_transfer_limit)
    }

    pub fn set_protocol_paused(ctx: Context<SetProtocolPaused>, paused: bool) -> Result<()> {
        admin::set_protocol_paused_handler(ctx, paused)
    }

    pub fn fallback<'info>(
        program_id: &'info Pubkey,
        accounts: &'info [AccountInfo<'info>],
        data: &'info [u8],
    ) -> Result<()> {
        execute::execute_fallback(program_id, accounts, data)
    }
}
