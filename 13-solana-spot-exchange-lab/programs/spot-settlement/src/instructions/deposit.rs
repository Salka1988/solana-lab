use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{transfer_checked, Token2022, TransferChecked},
    token_interface::{Mint, TokenAccount},
};

use crate::{
    constants::{MARKET_CONFIG_SEED, TRADER_MARKET_BALANCE_SEED},
    error::ErrorCode,
    state::{CustodyAsset, MarketConfig, TraderMarketBalance},
};

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub trader: Signer<'info>,

    #[account(
        seeds = [
            MARKET_CONFIG_SEED,
            market_config.base_mint.as_ref(),
            market_config.quote_mint.as_ref()
        ],
        bump = market_config.bump
    )]
    pub market_config: Account<'info, MarketConfig>,

    #[account(
        init_if_needed,
        payer = trader,
        space = TraderMarketBalance::SPACE,
        seeds = [
            TRADER_MARKET_BALANCE_SEED,
            market_config.key().as_ref(),
            trader.key().as_ref()
        ],
        bump
    )]
    pub trader_balance: Account<'info, TraderMarketBalance>,

    #[account(mut)]
    pub source: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub mint: InterfaceAccount<'info, Mint>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn deposit_handler(ctx: Context<Deposit>, asset: CustodyAsset, amount: u64) -> Result<()> {
    require!(!ctx.accounts.market_config.paused, ErrorCode::MarketPaused);
    require!(amount > 0, ErrorCode::InvalidDepositAmount);

    let market = &ctx.accounts.market_config;
    let (expected_mint, expected_vault) = match asset {
        CustodyAsset::Base => (market.base_mint, market.base_vault),
        CustodyAsset::Quote => (market.quote_mint, market.quote_vault),
    };

    require_keys_eq!(
        ctx.accounts.mint.key(),
        expected_mint,
        ErrorCode::InvalidDepositMint
    );
    require_keys_eq!(
        ctx.accounts.source.mint,
        expected_mint,
        ErrorCode::InvalidDepositSource
    );
    require_keys_eq!(
        ctx.accounts.source.owner,
        ctx.accounts.trader.key(),
        ErrorCode::InvalidDepositSource
    );
    require_keys_eq!(
        ctx.accounts.vault.key(),
        expected_vault,
        ErrorCode::InvalidMarketVault
    );
    require_keys_eq!(
        ctx.accounts.vault.mint,
        expected_mint,
        ErrorCode::InvalidMarketVault
    );
    require_keys_eq!(
        ctx.accounts.vault.owner,
        market.vault_authority,
        ErrorCode::InvalidMarketVaultAuthority
    );

    transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.source.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.trader.to_account_info(),
            },
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    let trader_balance = &mut ctx.accounts.trader_balance;
    if trader_balance.market_config == Pubkey::default() {
        trader_balance.market_config = market.key();
        trader_balance.trader = ctx.accounts.trader.key();
        trader_balance.bump = ctx.bumps.trader_balance;
    }

    match asset {
        CustodyAsset::Base => {
            trader_balance.available_base = trader_balance
                .available_base
                .checked_add(amount)
                .ok_or(ErrorCode::BalanceOverflow)?;
        }
        CustodyAsset::Quote => {
            trader_balance.available_quote = trader_balance
                .available_quote
                .checked_add(amount)
                .ok_or(ErrorCode::BalanceOverflow)?;
        }
    }

    Ok(())
}
