use anchor_lang::prelude::*;

use crate::{constants::*, error::ErrorCode, state::*};

#[derive(Accounts)]
pub struct AdminRedemptionAction<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        has_one = admin @ ErrorCode::UnauthorizedAdmin
    )]
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

    #[account(
        init,
        payer = admin,
        space = AdminActionLog::SPACE,
        seeds = [
            ADMIN_ACTION_LOG_SEED,
            protocol_config.key().as_ref(),
            &protocol_config.next_admin_action_id.to_le_bytes()
        ],
        bump
    )]
    pub admin_action_log: Account<'info, AdminActionLog>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetRedemptionsPaused<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        has_one = admin @ ErrorCode::UnauthorizedAdmin
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        init,
        payer = admin,
        space = AdminActionLog::SPACE,
        seeds = [
            ADMIN_ACTION_LOG_SEED,
            protocol_config.key().as_ref(),
            &protocol_config.next_admin_action_id.to_le_bytes()
        ],
        bump
    )]
    pub admin_action_log: Account<'info, AdminActionLog>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BeginAdminTransfer<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        has_one = admin @ ErrorCode::UnauthorizedAdmin
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        init,
        payer = admin,
        space = AdminActionLog::SPACE,
        seeds = [
            ADMIN_ACTION_LOG_SEED,
            protocol_config.key().as_ref(),
            &protocol_config.next_admin_action_id.to_le_bytes()
        ],
        bump
    )]
    pub admin_action_log: Account<'info, AdminActionLog>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AcceptAdminTransfer<'info> {
    #[account(mut)]
    pub pending_admin: Signer<'info>,

    #[account(mut, seeds = [PROTOCOL_SEED], bump = protocol_config.bump)]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        init,
        payer = pending_admin,
        space = AdminActionLog::SPACE,
        seeds = [
            ADMIN_ACTION_LOG_SEED,
            protocol_config.key().as_ref(),
            &protocol_config.next_admin_action_id.to_le_bytes()
        ],
        bump
    )]
    pub admin_action_log: Account<'info, AdminActionLog>,

    pub system_program: Program<'info, System>,
}

pub fn complete_redemption_handler(ctx: Context<AdminRedemptionAction>) -> Result<()> {
    require!(
        ctx.accounts.redemption_request.status == RedemptionStatus::Pending,
        ErrorCode::RequestNotPending
    );

    settle_request(
        ctx,
        RedemptionStatus::Completed,
        AdminAction::CompleteRedemption,
    )
}

pub fn reject_redemption_handler(ctx: Context<AdminRedemptionAction>) -> Result<()> {
    require!(
        ctx.accounts.redemption_request.status == RedemptionStatus::Pending,
        ErrorCode::RequestNotPending
    );

    settle_request(
        ctx,
        RedemptionStatus::Rejected,
        AdminAction::RejectRedemption,
    )
}

pub fn set_redemptions_paused_handler(
    ctx: Context<SetRedemptionsPaused>,
    paused: bool,
) -> Result<()> {
    ctx.accounts.protocol_config.redemptions_paused = paused;

    write_admin_log(
        &mut ctx.accounts.protocol_config,
        &mut ctx.accounts.admin_action_log,
        ctx.accounts.admin.key(),
        AdminAction::SetRedemptionsPaused,
        Pubkey::default(),
        paused as u64,
        ctx.bumps.admin_action_log,
    )
}

pub fn begin_admin_transfer_handler(
    ctx: Context<BeginAdminTransfer>,
    new_admin: Pubkey,
) -> Result<()> {
    require!(
        new_admin != Pubkey::default() && new_admin != ctx.accounts.protocol_config.admin,
        ErrorCode::InvalidPendingAdmin
    );

    ctx.accounts.protocol_config.pending_admin = new_admin;

    write_admin_log(
        &mut ctx.accounts.protocol_config,
        &mut ctx.accounts.admin_action_log,
        ctx.accounts.admin.key(),
        AdminAction::BeginAdminTransfer,
        new_admin,
        0,
        ctx.bumps.admin_action_log,
    )
}

pub fn accept_admin_transfer_handler(ctx: Context<AcceptAdminTransfer>) -> Result<()> {
    require!(
        ctx.accounts.protocol_config.pending_admin != Pubkey::default(),
        ErrorCode::MissingPendingAdmin
    );
    require!(
        ctx.accounts.protocol_config.pending_admin == ctx.accounts.pending_admin.key(),
        ErrorCode::UnauthorizedPendingAdmin
    );

    let previous_admin = ctx.accounts.protocol_config.admin;
    ctx.accounts.protocol_config.admin = ctx.accounts.pending_admin.key();
    ctx.accounts.protocol_config.pending_admin = Pubkey::default();

    write_admin_log(
        &mut ctx.accounts.protocol_config,
        &mut ctx.accounts.admin_action_log,
        ctx.accounts.pending_admin.key(),
        AdminAction::AcceptAdminTransfer,
        previous_admin,
        0,
        ctx.bumps.admin_action_log,
    )
}

fn settle_request(
    ctx: Context<AdminRedemptionAction>,
    status: RedemptionStatus,
    action: AdminAction,
) -> Result<()> {
    let amount = ctx.accounts.redemption_request.amount;

    let redemption_vault = &mut ctx.accounts.redemption_vault;
    redemption_vault.outstanding_amount = redemption_vault
        .outstanding_amount
        .checked_sub(amount)
        .ok_or(ErrorCode::VaultOutstandingTooLow)?;
    match status {
        RedemptionStatus::Completed => {
            redemption_vault.total_completed = redemption_vault
                .total_completed
                .checked_add(amount)
                .ok_or(ErrorCode::VaultOutstandingTooLow)?;
        }
        RedemptionStatus::Rejected => {
            redemption_vault.total_rejected = redemption_vault
                .total_rejected
                .checked_add(amount)
                .ok_or(ErrorCode::VaultOutstandingTooLow)?;
        }
        _ => unreachable!(),
    }

    let redemption_request = &mut ctx.accounts.redemption_request;
    redemption_request.status = status;
    redemption_request.updated_at = Clock::get()?.unix_timestamp;

    write_admin_log(
        &mut ctx.accounts.protocol_config,
        &mut ctx.accounts.admin_action_log,
        ctx.accounts.admin.key(),
        action,
        redemption_request.key(),
        amount,
        ctx.bumps.admin_action_log,
    )
}

fn write_admin_log(
    protocol_config: &mut Account<ProtocolConfig>,
    admin_action_log: &mut Account<AdminActionLog>,
    admin: Pubkey,
    action: AdminAction,
    target: Pubkey,
    amount: u64,
    bump: u8,
) -> Result<()> {
    let action_id = protocol_config.next_admin_action_id;
    admin_action_log.protocol_config = protocol_config.key();
    admin_action_log.action_id = action_id;
    admin_action_log.admin = admin;
    admin_action_log.action = action;
    admin_action_log.target = target;
    admin_action_log.amount = amount;
    admin_action_log.created_at = Clock::get()?.unix_timestamp;
    admin_action_log.bump = bump;

    protocol_config.next_admin_action_id = action_id
        .checked_add(1)
        .ok_or(ErrorCode::InvalidPendingAdmin)?;

    Ok(())
}
