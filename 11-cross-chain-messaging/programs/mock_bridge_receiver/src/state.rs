use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct CrossChainMintMessage {
    pub source_chain_id: u16,
    pub destination_chain_id: u16,
    pub nonce: u64,
    pub mint: Pubkey,
    pub recipient: Pubkey,
    pub amount: u64,
}

impl CrossChainMintMessage {
    pub const LEN: usize = 2 + 2 + 8 + 32 + 32 + 8;
}

#[account]
pub struct BridgeConfig {
    pub admin: Pubkey,
    pub bridge_authority: Pubkey,
    pub registered_mint: Pubkey,
    pub per_message_limit: u64,
    pub bump: u8,
}

impl BridgeConfig {
    pub const LEN: usize = 32 + 32 + 32 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}

#[account]
pub struct ConsumedMessage {
    pub bridge_config: Pubkey,
    pub source_chain_id: u16,
    pub nonce: u64,
    pub mint: Pubkey,
    pub recipient: Pubkey,
    pub amount: u64,
    pub bump: u8,
}

impl ConsumedMessage {
    pub const LEN: usize = 32 + 2 + 8 + 32 + 32 + 8 + 1;
    pub const SPACE: usize = 8 + Self::LEN;
}
