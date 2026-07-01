use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke_signed, system_instruction},
};
use spl_tlv_account_resolution::{
    account::ExtraAccountMeta, seeds::Seed, state::ExtraAccountMetaList,
};
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

use crate::{
    constants::{
        COMPLIANCE_CONFIG_SEED, EXTRA_ACCOUNT_METAS_SEED, EXTRA_ACCOUNT_META_LIST_SPACE,
        USER_COMPLIANCE_SEED,
    },
    error::ErrorCode,
    state::{ComplianceConfig, UserCompliance},
};

#[derive(Accounts)]
pub struct InitializeComplianceConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = ComplianceConfig::SPACE,
        seeds = [COMPLIANCE_CONFIG_SEED, mint.key().as_ref()],
        bump
    )]
    pub config: Account<'info, ComplianceConfig>,

    /// CHECK: Mint identity only.
    pub mint: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [COMPLIANCE_CONFIG_SEED, mint.key().as_ref()],
        bump = config.bump,
        has_one = admin,
        has_one = mint
    )]
    pub config: Account<'info, ComplianceConfig>,

    #[account(
        mut,
        seeds = [EXTRA_ACCOUNT_METAS_SEED, mint.key().as_ref()],
        bump
    )]
    /// CHECK: Raw TLV validation PDA owned by this program.
    pub extra_account_meta_list: UncheckedAccount<'info>,

    /// CHECK: Mint identity only.
    pub mint: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetUserCompliance<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [COMPLIANCE_CONFIG_SEED, mint.key().as_ref()],
        bump = config.bump,
        has_one = admin,
        has_one = mint
    )]
    pub config: Account<'info, ComplianceConfig>,

    #[account(
        init_if_needed,
        payer = admin,
        space = UserCompliance::SPACE,
        seeds = [USER_COMPLIANCE_SEED, mint.key().as_ref(), user.key().as_ref()],
        bump
    )]
    pub user_compliance: Account<'info, UserCompliance>,

    /// CHECK: User or token account identity for compliance status.
    pub user: UncheckedAccount<'info>,

    /// CHECK: Mint identity only.
    pub mint: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SetTransferLimits<'info> {
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [COMPLIANCE_CONFIG_SEED, config.mint.as_ref()],
        bump = config.bump,
        has_one = admin
    )]
    pub config: Account<'info, ComplianceConfig>,
}

#[derive(Accounts)]
pub struct SetProtocolPaused<'info> {
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [COMPLIANCE_CONFIG_SEED, config.mint.as_ref()],
        bump = config.bump,
        has_one = admin
    )]
    pub config: Account<'info, ComplianceConfig>,
}

pub fn initialize_compliance_config_handler(
    ctx: Context<InitializeComplianceConfig>,
    max_transfer_amount: u64,
    daily_transfer_limit: u64,
) -> Result<()> {
    require!(
        max_transfer_amount > 0 && daily_transfer_limit > 0,
        ErrorCode::InvalidTransferLimit
    );

    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.mint = ctx.accounts.mint.key();
    config.max_transfer_amount = max_transfer_amount;
    config.daily_transfer_limit = daily_transfer_limit;
    config.paused = false;
    config.bump = ctx.bumps.config;

    Ok(())
}

pub fn initialize_extra_account_meta_list_handler(
    ctx: Context<InitializeExtraAccountMetaList>,
) -> Result<()> {
    let rent_lamports = Rent::get()?.minimum_balance(EXTRA_ACCOUNT_META_LIST_SPACE);
    let bump = [ctx.bumps.extra_account_meta_list];
    let mint_key = ctx.accounts.mint.key();
    let seeds: &[&[u8]] = &[EXTRA_ACCOUNT_METAS_SEED, mint_key.as_ref(), &bump];

    invoke_signed(
        &system_instruction::create_account(
            ctx.accounts.admin.key,
            ctx.accounts.extra_account_meta_list.key,
            rent_lamports,
            EXTRA_ACCOUNT_META_LIST_SPACE as u64,
            ctx.program_id,
        ),
        &[
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.extra_account_meta_list.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
        &[seeds],
    )?;

    let extra_account_metas = vec![
        ExtraAccountMeta::new_with_seeds(
            &[
                Seed::Literal {
                    bytes: COMPLIANCE_CONFIG_SEED.to_vec(),
                },
                Seed::AccountKey { index: 1 },
            ],
            false,
            false,
        )
        .map_err(|_| ErrorCode::InvalidTransferHookAccounts)?,
        ExtraAccountMeta::new_with_seeds(
            &[
                Seed::Literal {
                    bytes: USER_COMPLIANCE_SEED.to_vec(),
                },
                Seed::AccountKey { index: 1 },
                Seed::AccountKey { index: 0 },
            ],
            false,
            true,
        )
        .map_err(|_| ErrorCode::InvalidTransferHookAccounts)?,
        ExtraAccountMeta::new_with_seeds(
            &[
                Seed::Literal {
                    bytes: USER_COMPLIANCE_SEED.to_vec(),
                },
                Seed::AccountKey { index: 1 },
                Seed::AccountKey { index: 2 },
            ],
            false,
            false,
        )
        .map_err(|_| ErrorCode::InvalidTransferHookAccounts)?,
    ];

    ExtraAccountMetaList::init::<ExecuteInstruction>(
        &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
        &extra_account_metas,
    )
    .map_err(|_| ErrorCode::InvalidTransferHookAccounts)?;

    Ok(())
}

pub fn set_user_compliance_handler(
    ctx: Context<SetUserCompliance>,
    allowlisted: bool,
    blocked: bool,
    issuer_active: bool,
) -> Result<()> {
    let user_compliance = &mut ctx.accounts.user_compliance;
    user_compliance.config = ctx.accounts.config.key();
    user_compliance.mint = ctx.accounts.mint.key();
    user_compliance.user = ctx.accounts.user.key();
    user_compliance.allowlisted = allowlisted;
    user_compliance.blocked = blocked;
    user_compliance.issuer_active = issuer_active;
    user_compliance.bump = ctx.bumps.user_compliance;

    Ok(())
}

pub fn set_transfer_limits_handler(
    ctx: Context<SetTransferLimits>,
    max_transfer_amount: u64,
    daily_transfer_limit: u64,
) -> Result<()> {
    require!(
        max_transfer_amount > 0 && daily_transfer_limit > 0,
        ErrorCode::InvalidTransferLimit
    );

    let config = &mut ctx.accounts.config;
    config.max_transfer_amount = max_transfer_amount;
    config.daily_transfer_limit = daily_transfer_limit;

    Ok(())
}

pub fn set_protocol_paused_handler(ctx: Context<SetProtocolPaused>, paused: bool) -> Result<()> {
    ctx.accounts.config.paused = paused;

    Ok(())
}
