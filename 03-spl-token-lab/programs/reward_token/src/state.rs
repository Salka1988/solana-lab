use anchor_lang::prelude::*;

#[account]
pub struct RewardMintConfig {
    pub admin: Pubkey,
    pub reward_mint: Pubkey,
    pub mint_authority_bump: u8,
    pub decimals: u8,
}

impl RewardMintConfig {
    pub const LEN: usize = 32 + 32 + 1 + 1;
}
