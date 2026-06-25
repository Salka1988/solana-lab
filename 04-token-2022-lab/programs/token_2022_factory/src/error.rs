use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Token-2022 factory error")]
    Token2022FactoryError,

    #[msg("Mint account space does not match required Token-2022 extension space")]
    InvalidMintSpace,
}
