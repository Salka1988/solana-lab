use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Redemption amount must be greater than zero")]
    InvalidRedemptionAmount,
    #[msg("Redemptions are paused")]
    RedemptionsPaused,
    #[msg("Request is not pending")]
    RequestNotPending,
    #[msg("Request owner is not authorized")]
    UnauthorizedRequestOwner,
    #[msg("Admin is not authorized")]
    UnauthorizedAdmin,
    #[msg("Pending admin is not authorized")]
    UnauthorizedPendingAdmin,
    #[msg("Pending admin is missing")]
    MissingPendingAdmin,
    #[msg("New admin must be different from current admin")]
    InvalidPendingAdmin,
    #[msg("Vault outstanding amount is too low")]
    VaultOutstandingTooLow,
}
