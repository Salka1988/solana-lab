use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Deposit amount overflows vault total")]
    DepositOverflow,

    #[msg("Deposit would exceed vault limit")]
    VaultLimitExceeded,

    #[msg("Withdraw amount exceeds deposited balance")]
    InsufficientVaultBalance,

    #[msg("Vault must be empty before closing")]
    VaultNotEmpty,
}
