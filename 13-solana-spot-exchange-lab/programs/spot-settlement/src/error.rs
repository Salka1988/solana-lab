use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Market base and quote mints must differ")]
    SameMarketMints,
    #[msg("Market vaults must differ")]
    SameMarketVaults,
    #[msg("Only protocol admin can perform this action")]
    UnauthorizedAdmin,
    #[msg("Market is paused")]
    MarketPaused,
    #[msg("Deposit amount must be greater than zero")]
    InvalidDepositAmount,
    #[msg("Invalid deposit mint")]
    InvalidDepositMint,
    #[msg("Invalid deposit source account")]
    InvalidDepositSource,
    #[msg("Invalid market vault")]
    InvalidMarketVault,
    #[msg("Invalid market vault authority")]
    InvalidMarketVaultAuthority,
    #[msg("Deposit balance overflow")]
    BalanceOverflow,
}
