use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Protocol is paused")]
    ProtocolPaused,
    #[msg("Issuer is paused")]
    IssuerPaused,
    #[msg("Issuer is not authorized")]
    UnauthorizedIssuer,
    #[msg("Mint amount exceeds issuer limit")]
    IssuerLimitExceeded,
    #[msg("Mint amount exceeds global supply cap")]
    GlobalSupplyCapExceeded,
    #[msg("Global supply cap must be greater than zero")]
    InvalidGlobalSupplyCap,
    #[msg("Stablecoin mint already created")]
    StablecoinMintAlreadyCreated,
    #[msg("Issuer mint limit must be greater than zero")]
    InvalidIssuerMintLimit,
    #[msg("Burn amount exceeds tracked outstanding supply")]
    BurnAmountExceedsOutstanding,
}
