pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("9BccVRkKCvpjedrsMRoM5vZc9PtvCzfnc2kQgJzPRtun");

#[program]
pub mod mock_bridge_receiver {
    use super::*;

    pub fn initialize_bridge_config(
        ctx: Context<InitializeBridgeConfig>,
        per_message_limit: u64,
    ) -> Result<()> {
        initialize::initialize_bridge_config_handler(ctx, per_message_limit)
    }

    pub fn consume_cross_chain_mint_message(
        ctx: Context<ConsumeCrossChainMintMessage>,
        message: CrossChainMintMessage,
    ) -> Result<()> {
        consume::consume_cross_chain_mint_message_handler(ctx, message)
    }
}
