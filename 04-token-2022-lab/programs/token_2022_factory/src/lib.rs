pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("59CUZwGMc4xHD4FigFqLyFMsfwaXGPVQuxNNkE9ikzLP");

#[program]
pub mod token_2022_factory {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        initialize::initialize_handler(ctx)
    }

    pub fn create_token_2022_mint(
        ctx: Context<CreateToken2022Mint>,
        decimals: u8,
        name: String,
        symbol: String,
        uri: String,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
    ) -> Result<()> {
        create_token_2022_mint::create_token_2022_mint_handler(
            ctx,
            decimals,
            name,
            symbol,
            uri,
            transfer_fee_basis_points,
            maximum_fee,
        )
    }

    pub fn mint_to_user(ctx: Context<MintToUser>, amount: u64) -> Result<()> {
        mint_to_user::mint_to_user_handler(ctx, amount)
    }

    pub fn transfer_with_fee(ctx: Context<TransferWithFee>, amount: u64, fee: u64) -> Result<()> {
        transfer_with_fee::transfer_with_fee_handler(ctx, amount, fee)
    }

    pub fn burn_from_user(ctx: Context<BurnFromUser>, amount: u64) -> Result<()> {
        burn_from_user::burn_from_user_handler(ctx, amount)
    }

    pub fn harvest_withheld_fees<'info>(
        ctx: Context<'info, HarvestWithheldFees<'info>>,
    ) -> Result<()> {
        harvest_withheld_fees::harvest_withheld_fees_handler(ctx)
    }

    pub fn withdraw_withheld_fees(ctx: Context<WithdrawWithheldFees>) -> Result<()> {
        withdraw_withheld_fees::withdraw_withheld_fees_handler(ctx)
    }

    pub fn set_transfer_fee_config(
        ctx: Context<SetTransferFeeConfig>,
        transfer_fee_basis_points: u16,
        maximum_fee: u64,
    ) -> Result<()> {
        set_transfer_fee_config::set_transfer_fee_config_handler(
            ctx,
            transfer_fee_basis_points,
            maximum_fee,
        )
    }
}
