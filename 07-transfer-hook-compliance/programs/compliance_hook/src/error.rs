use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Compliance protocol is paused")]
    ProtocolPaused,
    #[msg("Transfer source is not allowlisted")]
    SourceNotAllowlisted,
    #[msg("Transfer destination is not allowlisted")]
    DestinationNotAllowlisted,
    #[msg("Transfer source is blocked")]
    SourceBlocked,
    #[msg("Transfer destination is blocked")]
    DestinationBlocked,
    #[msg("Source issuer is not active")]
    IssuerInactive,
    #[msg("Transfer amount exceeds per-transfer limit")]
    TransferLimitExceeded,
    #[msg("Transfer amount exceeds daily limit")]
    DailyLimitExceeded,
    #[msg("Transfer limits must be greater than zero")]
    InvalidTransferLimit,
    #[msg("Invalid transfer hook account list")]
    InvalidTransferHookAccounts,
}
