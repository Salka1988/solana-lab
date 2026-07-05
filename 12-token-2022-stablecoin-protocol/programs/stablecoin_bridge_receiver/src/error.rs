use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Message destination chain is invalid")]
    InvalidDestinationChain,
    #[msg("Message mint is not registered")]
    UnregisteredMint,
    #[msg("Bridge amount limit exceeded")]
    BridgeLimitExceeded,
    #[msg("Message amount must be greater than zero")]
    InvalidMessageAmount,
    #[msg("Message recipient cannot be default pubkey")]
    InvalidRecipient,
    #[msg("Bridge authority is not authorized")]
    UnauthorizedBridgeAuthority,
}
