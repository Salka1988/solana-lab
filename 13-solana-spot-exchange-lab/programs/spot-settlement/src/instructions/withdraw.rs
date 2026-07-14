use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{transfer_checked, Token2022, TransferChecked},
    token_interface::{Mint, TokenAccount},
};

use crate::{
    constants::{MARKET_CONFIG_SEED, TRADER_MARKET_BALANCE_SEED, VAULT_AUTHORITY_SEED},
    error::ErrorCode,
    state::{CustodyAsset, MarketConfig, TraderMarketBalance},
};

#[derive(Accounts)]
pub struct Withdraw<'info> {
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
        mut,
        seeds = [
            TRADER_MARKET_BALANCE_SEED,
            market_config.key().as_ref(),
            trader.key().as_ref()
        ],
        bump = trader_balance.bump,
        has_one = market_config,
        has_one = trader
    )]
    pub trader_balance: Account<'info, TraderMarketBalance>,

    #[account(mut)]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub destination: InterfaceAccount<'info, TokenAccount>,

    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: PDA authority validated by seeds and used only as CPI signer.
    #[account(
        seeds = [VAULT_AUTHORITY_SEED, market_config.key().as_ref()],
        bump = market_config.vault_authority_bump
    )]
    pub vault_authority: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token2022>,
}

pub fn withdraw_handler(ctx: Context<Withdraw>, asset: CustodyAsset, amount: u64) -> Result<()> {
    require!(!ctx.accounts.market_config.paused, ErrorCode::MarketPaused);
    require!(amount > 0, ErrorCode::InvalidWithdrawAmount);

    let market = &ctx.accounts.market_config;
    let (expected_mint, expected_vault) = match asset {
        CustodyAsset::Base => (market.base_mint, market.base_vault),
        CustodyAsset::Quote => (market.quote_mint, market.quote_vault),
    };

    require_keys_eq!(
        ctx.accounts.mint.key(),
        expected_mint,
        ErrorCode::InvalidWithdrawMint
    );
    require_keys_eq!(
        ctx.accounts.destination.mint,
        expected_mint,
        ErrorCode::InvalidWithdrawDestination
    );
    require_keys_eq!(
        ctx.accounts.destination.owner,
        ctx.accounts.trader.key(),
        ErrorCode::InvalidWithdrawDestination
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
    require_keys_eq!(
        ctx.accounts.vault_authority.key(),
        market.vault_authority,
        ErrorCode::InvalidMarketVaultAuthority
    );

    match asset {
        CustodyAsset::Base => {
            ctx.accounts.trader_balance.available_base = ctx
                .accounts
                .trader_balance
                .available_base
                .checked_sub(amount)
                .ok_or(ErrorCode::InsufficientAvailableBalance)?;
        }
        CustodyAsset::Quote => {
            ctx.accounts.trader_balance.available_quote = ctx
                .accounts
                .trader_balance
                .available_quote
                .checked_sub(amount)
                .ok_or(ErrorCode::InsufficientAvailableBalance)?;
        }
    }

    let market_config_key = market.key();
    let vault_authority_bump = [market.vault_authority_bump];
    let signer_seeds = &[&[
        VAULT_AUTHORITY_SEED,
        market_config_key.as_ref(),
        vault_authority_bump.as_ref(),
    ][..]];

    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.vault.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.destination.to_account_info(),
                authority: ctx.accounts.vault_authority.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
        ctx.accounts.mint.decimals,
    )?;

    Ok(())
}
