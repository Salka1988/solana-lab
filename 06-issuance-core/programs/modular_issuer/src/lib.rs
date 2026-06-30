pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::{
    GlobalSupplyStats, IssuerConfig, IssuerStats, ProtocolConfig, StablecoinMintConfig,
};

declare_id!("6zKsNTfMRviuMCxkGS1JgbpPzPJC4ZZJFP1qLEKCdNq6");

#[program]
pub mod modular_issuer {
    use super::*;

    pub fn initialize_protocol(
        ctx: Context<InitializeProtocol>,
        global_supply_cap: u64,
    ) -> Result<()> {
        initialize::initialize_protocol_handler(ctx, global_supply_cap)
    }

    pub fn create_stablecoin_mint(
        ctx: Context<CreateStablecoinMint>,
        decimals: u8,
        name: String,
        symbol: String,
        uri: String,
    ) -> Result<()> {
        create_stablecoin_mint::create_stablecoin_mint_handler(ctx, decimals, name, symbol, uri)
    }

    pub fn register_issuer(ctx: Context<RegisterIssuer>, mint_limit: u64) -> Result<()> {
        register_issuer::register_issuer_handler(ctx, mint_limit)
    }

    pub fn set_issuer_paused(ctx: Context<SetIssuerPaused>, paused: bool) -> Result<()> {
        set_issuer_paused::set_issuer_paused_handler(ctx, paused)
    }

    pub fn rotate_issuer_authority(ctx: Context<RotateIssuerAuthority>) -> Result<()> {
        rotate_issuer_authority::rotate_issuer_authority_handler(ctx)
    }

    pub fn mint_to_user(ctx: Context<MintToUser>, amount: u64) -> Result<()> {
        mint_to_user::mint_to_user_handler(ctx, amount)
    }

    pub fn burn_from_user(ctx: Context<BurnFromUser>, amount: u64) -> Result<()> {
        burn_from_user::burn_from_user_handler(ctx, amount)
    }
}
