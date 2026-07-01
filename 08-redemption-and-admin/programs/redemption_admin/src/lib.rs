pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("9Nw7daj1a4bqTL5R9qFCCGUnDWEPfk7zhFbu9V26WuCr");

#[program]
pub mod redemption_admin {
    use super::*;

    pub fn initialize_protocol(ctx: Context<InitializeProtocol>) -> Result<()> {
        initialize::initialize_protocol_handler(ctx)
    }

    pub fn request_redemption(ctx: Context<RequestRedemption>, amount: u64) -> Result<()> {
        redemption::request_redemption_handler(ctx, amount)
    }

    pub fn cancel_redemption(ctx: Context<CancelRedemption>) -> Result<()> {
        redemption::cancel_redemption_handler(ctx)
    }

    pub fn complete_redemption(ctx: Context<AdminRedemptionAction>) -> Result<()> {
        admin::complete_redemption_handler(ctx)
    }

    pub fn reject_redemption(ctx: Context<AdminRedemptionAction>) -> Result<()> {
        admin::reject_redemption_handler(ctx)
    }

    pub fn set_redemptions_paused(ctx: Context<SetRedemptionsPaused>, paused: bool) -> Result<()> {
        admin::set_redemptions_paused_handler(ctx, paused)
    }

    pub fn begin_admin_transfer(ctx: Context<BeginAdminTransfer>, new_admin: Pubkey) -> Result<()> {
        admin::begin_admin_transfer_handler(ctx, new_admin)
    }

    pub fn accept_admin_transfer(ctx: Context<AcceptAdminTransfer>) -> Result<()> {
        admin::accept_admin_transfer_handler(ctx)
    }
}
