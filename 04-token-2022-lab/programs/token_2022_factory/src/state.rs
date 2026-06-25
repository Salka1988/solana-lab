use anchor_lang::prelude::*;

#[account]
pub struct Token2022MintConfig {
    pub admin: Pubkey,
    pub mint: Pubkey,
    pub mint_authority_bump: u8,
    pub decimals: u8,
}

impl Token2022MintConfig {
    pub const LEN: usize = 32 + 32 + 1 + 1;
}
