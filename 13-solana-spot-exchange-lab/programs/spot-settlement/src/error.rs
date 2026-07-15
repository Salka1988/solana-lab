use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Market base and quote mints must differ")]
    SameMarketMints,
    #[msg("Market vaults must differ")]
    SameMarketVaults,
    #[msg("Only protocol admin can perform this action")]
    UnauthorizedAdmin,
    #[msg("Only settlement authority can perform this action")]
    UnauthorizedSettlementAuthority,
    #[msg("Market is paused")]
    MarketPaused,
    #[msg("Settlement amount must be greater than zero")]
    InvalidSettlementAmount,
    #[msg("Deposit amount must be greater than zero")]
    InvalidDepositAmount,
    #[msg("Withdraw amount must be greater than zero")]
    InvalidWithdrawAmount,
    #[msg("Invalid deposit mint")]
    InvalidDepositMint,
    #[msg("Invalid deposit source account")]
    InvalidDepositSource,
    #[msg("Invalid withdraw mint")]
    InvalidWithdrawMint,
    #[msg("Invalid withdraw destination account")]
    InvalidWithdrawDestination,
    #[msg("Invalid market vault")]
    InvalidMarketVault,
    #[msg("Invalid market vault authority")]
    InvalidMarketVaultAuthority,
    #[msg("Insufficient available balance")]
    InsufficientAvailableBalance,
    #[msg("Settlement balance overflow")]
    SettlementBalanceOverflow,
    #[msg("Deposit balance overflow")]
    BalanceOverflow,
}
