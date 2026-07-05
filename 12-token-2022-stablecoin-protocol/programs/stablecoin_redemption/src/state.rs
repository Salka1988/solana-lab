use anchor_lang::prelude::*;

#[account]
pub struct ProtocolConfig {
    pub admin: Pubkey,
    pub pending_admin: Pubkey,
    pub redemptions_paused: bool,
    pub next_request_id: u64,
    pub next_admin_action_id: u64,
    pub bump: u8,
}

impl ProtocolConfig {
    pub const LEN: usize = 32 + 32 + 1 + 8 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct RedemptionVault {
    pub protocol_config: Pubkey,
    pub outstanding_amount: u64,
    pub total_requested: u64,
    pub total_completed: u64,
    pub total_cancelled: u64,
    pub total_rejected: u64,
    pub bump: u8,
}

impl RedemptionVault {
    pub const LEN: usize = 32 + 8 + 8 + 8 + 8 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct RedemptionRequest {
    pub protocol_config: Pubkey,
    pub vault: Pubkey,
    pub request_id: u64,
    pub owner: Pubkey,
    pub amount: u64,
    pub status: RedemptionStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub bump: u8,
}

impl RedemptionRequest {
    pub const LEN: usize = 32 + 32 + 8 + 32 + 8 + RedemptionStatus::LEN + 8 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum RedemptionStatus {
    Pending,
    Cancelled,
    Completed,
    Rejected,
}

impl RedemptionStatus {
    pub const LEN: usize = 1;
}

#[account]
pub struct AdminActionLog {
    pub protocol_config: Pubkey,
    pub action_id: u64,
    pub admin: Pubkey,
    pub action: AdminAction,
    pub target: Pubkey,
    pub amount: u64,
    pub created_at: i64,
    pub bump: u8,
}

impl AdminActionLog {
    pub const LEN: usize = 32 + 8 + 32 + AdminAction::LEN + 32 + 8 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AdminAction {
    CompleteRedemption,
    RejectRedemption,
    SetRedemptionsPaused,
    BeginAdminTransfer,
    AcceptAdminTransfer,
}

impl AdminAction {
    pub const LEN: usize = 1;
}
