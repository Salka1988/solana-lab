pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("Cran29KwUp3xJZCbuGyeQAgL6pj5cA9m2MowtLQn9z2R");

#[program]
pub mod spot_settlement {
    use super::*;

    pub fn initialize_protocol(ctx: Context<InitializeProtocol>) -> Result<()> {
        initialize::initialize_protocol_handler(ctx)
    }

    pub fn initialize_market(ctx: Context<InitializeMarket>) -> Result<()> {
        initialize::initialize_market_handler(ctx)
    }

    pub fn deposit(ctx: Context<Deposit>, asset: DepositAsset, amount: u64) -> Result<()> {
        deposit::deposit_handler(ctx, asset, amount)
    }
}
