use anchor_lang::prelude::*;

use crate::{constants::*, error::ErrorCode, state::*};

#[derive(Accounts)]
pub struct RequestRedemption<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(mut, seeds = [PROTOCOL_SEED], bump = protocol_config.bump)]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [REDEMPTION_VAULT_SEED, protocol_config.key().as_ref()],
        bump = redemption_vault.bump
    )]
    pub redemption_vault: Account<'info, RedemptionVault>,

    #[account(
        init,
        payer = owner,
        space = RedemptionRequest::SPACE,
        seeds = [
            REDEMPTION_REQUEST_SEED,
            protocol_config.key().as_ref(),
            &protocol_config.next_request_id.to_le_bytes()
        ],
        bump
    )]
    pub redemption_request: Account<'info, RedemptionRequest>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CancelRedemption<'info> {
    pub owner: Signer<'info>,

    #[account(seeds = [PROTOCOL_SEED], bump = protocol_config.bump)]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [REDEMPTION_VAULT_SEED, protocol_config.key().as_ref()],
        bump = redemption_vault.bump
    )]
    pub redemption_vault: Account<'info, RedemptionVault>,

    #[account(
        mut,
        seeds = [
            REDEMPTION_REQUEST_SEED,
            protocol_config.key().as_ref(),
            &redemption_request.request_id.to_le_bytes()
        ],
        bump = redemption_request.bump,
        has_one = protocol_config,
        constraint = redemption_request.vault == redemption_vault.key()
    )]
    pub redemption_request: Account<'info, RedemptionRequest>,
}

pub fn request_redemption_handler(ctx: Context<RequestRedemption>, amount: u64) -> Result<()> {
    require!(amount > 0, ErrorCode::InvalidRedemptionAmount);
    require!(
        !ctx.accounts.protocol_config.redemptions_paused,
        ErrorCode::RedemptionsPaused
    );

    let clock = Clock::get()?;
    let request_id = ctx.accounts.protocol_config.next_request_id;

    let redemption_request = &mut ctx.accounts.redemption_request;
    redemption_request.protocol_config = ctx.accounts.protocol_config.key();
    redemption_request.vault = ctx.accounts.redemption_vault.key();
    redemption_request.request_id = request_id;
    redemption_request.owner = ctx.accounts.owner.key();
    redemption_request.amount = amount;
    redemption_request.status = RedemptionStatus::Pending;
    redemption_request.created_at = clock.unix_timestamp;
    redemption_request.updated_at = clock.unix_timestamp;
    redemption_request.bump = ctx.bumps.redemption_request;

    let redemption_vault = &mut ctx.accounts.redemption_vault;
    redemption_vault.outstanding_amount = redemption_vault
        .outstanding_amount
        .checked_add(amount)
        .ok_or(ErrorCode::VaultOutstandingTooLow)?;
    redemption_vault.total_requested = redemption_vault
        .total_requested
        .checked_add(amount)
        .ok_or(ErrorCode::VaultOutstandingTooLow)?;

    ctx.accounts.protocol_config.next_request_id = request_id
        .checked_add(1)
        .ok_or(ErrorCode::InvalidRedemptionAmount)?;

    Ok(())
}

pub fn cancel_redemption_handler(ctx: Context<CancelRedemption>) -> Result<()> {
    require!(
        ctx.accounts.redemption_request.status == RedemptionStatus::Pending,
        ErrorCode::RequestNotPending
    );
    require!(
        ctx.accounts.redemption_request.owner == ctx.accounts.owner.key(),
        ErrorCode::UnauthorizedRequestOwner
    );

    let amount = ctx.accounts.redemption_request.amount;
    let redemption_vault = &mut ctx.accounts.redemption_vault;
    redemption_vault.outstanding_amount = redemption_vault
        .outstanding_amount
        .checked_sub(amount)
        .ok_or(ErrorCode::VaultOutstandingTooLow)?;
    redemption_vault.total_cancelled = redemption_vault
        .total_cancelled
        .checked_add(amount)
        .ok_or(ErrorCode::VaultOutstandingTooLow)?;

    let redemption_request = &mut ctx.accounts.redemption_request;
    redemption_request.status = RedemptionStatus::Cancelled;
    redemption_request.updated_at = Clock::get()?.unix_timestamp;

    Ok(())
}
