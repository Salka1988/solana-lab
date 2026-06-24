pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("8XXuLTyBTomkWUySZCJV9dxKYZjynQvDDPxhERjz4i2c");

#[program]
pub mod reward_token {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        initialize::initialize_handler(ctx)
    }

    pub fn initialize_reward_mint(ctx: Context<InitializeRewardMint>, decimals: u8) -> Result<()> {
        initialize_reward_mint::initialize_reward_mint_handler(ctx, decimals)
    }

    pub fn ensure_user_ata(ctx: Context<EnsureUserAta>) -> Result<()> {
        ensure_user_ata::ensure_user_ata_handler(ctx)
    }

    pub fn mint_reward(ctx: Context<MintReward>, amount: u64) -> Result<()> {
        mint_reward::mint_reward_handler(ctx, amount)
    }

    pub fn burn_reward(ctx: Context<BurnReward>, amount: u64) -> Result<()> {
        burn_reward::burn_reward_handler(ctx, amount)
    }

    pub fn transfer_reward(ctx: Context<TransferReward>, amount: u64) -> Result<()> {
        transfer_reward::transfer_reward_handler(ctx, amount)
    }
}
