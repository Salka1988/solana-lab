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

    pub fn deposit(ctx: Context<Deposit>, asset: CustodyAsset, amount: u64) -> Result<()> {
        deposit::deposit_handler(ctx, asset, amount)
    }

    pub fn withdraw(ctx: Context<Withdraw>, asset: CustodyAsset, amount: u64) -> Result<()> {
        withdraw::withdraw_handler(ctx, asset, amount)
    }

    pub fn settle_fill(
        ctx: Context<SettleFill>,
        settlement_id: u64,
        base_amount: u64,
        quote_amount: u64,
    ) -> Result<()> {
        settlement::settle_fill_handler(ctx, settlement_id, base_amount, quote_amount)
    }
}
