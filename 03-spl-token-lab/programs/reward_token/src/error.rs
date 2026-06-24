use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Reward token error")]
    RewardTokenError,

    #[msg("Invalid associated token account")]
    InvalidAssociatedTokenAccount,

    #[msg("Invalid reward mint")]
    InvalidRewardMint,
}
