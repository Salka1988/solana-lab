use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction},
};
use anchor_spl::{
    token_2022::{
        initialize_mint2,
        spl_token_2022::{extension::ExtensionType, pod::PodMint, state::PackedSizeOf},
        InitializeMint2, Token2022,
    },
    token_2022_extensions::metadata_pointer::{
        metadata_pointer_initialize, MetadataPointerInitialize,
    },
    token_2022_extensions::permanent_delegate::{
        permanent_delegate_initialize, PermanentDelegateInitialize,
    },
    token_2022_extensions::token_metadata::{token_metadata_initialize, TokenMetadataInitialize},
    token_2022_extensions::transfer_fee::{transfer_fee_initialize, TransferFeeInitialize},
};
use spl_token_metadata_interface::state::TokenMetadata;

use crate::{
    error::ErrorCode as FactoryErrorCode, Token2022MintConfig, MINT_AUTHORITY_SEED,
    TOKEN_2022_MINT_CONFIG_SEED,
};

const STANDALONE_TLV_STATE_HEADER_LEN: usize = 8;

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct CreateToken2022Mint<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + Token2022MintConfig::LEN,
        seeds = [TOKEN_2022_MINT_CONFIG_SEED, mint.key().as_ref()],
        bump
    )]
    pub mint_config: Account<'info, Token2022MintConfig>,

    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump
    )]
    /// CHECK: PDA authority only; no data read or written.
    pub mint_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub mint: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn create_token_2022_mint_handler(
    ctx: Context<CreateToken2022Mint>,
    decimals: u8,
    name: String,
    symbol: String,
    uri: String,
    transfer_fee_basis_points: u16,
    maximum_fee: u64,
) -> Result<()> {
    let token_metadata = TokenMetadata {
        mint: ctx.accounts.mint.key(),
        name: name.clone(),
        symbol: symbol.clone(),
        uri: uri.clone(),
        ..Default::default()
    };
    let extensions = [
        ExtensionType::MetadataPointer,
        ExtensionType::TransferFeeConfig,
        ExtensionType::PermanentDelegate,
    ];
    let fixed_mint_space = ExtensionType::try_calculate_account_len::<PodMint>(&extensions)?;
    let token_metadata_space = token_metadata
        .tlv_size_of()?
        .checked_sub(STANDALONE_TLV_STATE_HEADER_LEN)
        .ok_or(ProgramError::InvalidAccountData)?;
    let final_mint_space = fixed_mint_space
        .checked_add(token_metadata_space)
        .ok_or(ProgramError::InvalidAccountData)?;
    let rent_lamports = Rent::get()?.minimum_balance(final_mint_space);

    require!(
        fixed_mint_space > PodMint::SIZE_OF,
        FactoryErrorCode::InvalidMintSpace
    );

    invoke(
        &system_instruction::create_account(
            ctx.accounts.admin.key,
            ctx.accounts.mint.key,
            rent_lamports,
            fixed_mint_space as u64,
            ctx.accounts.token_program.key,
        ),
        &[
            ctx.accounts.admin.to_account_info(),
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
    )?;

    metadata_pointer_initialize(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            MetadataPointerInitialize {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        Some(ctx.accounts.mint_authority.key()),
        Some(ctx.accounts.mint.key()),
    )?;

    transfer_fee_initialize(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferFeeInitialize {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        Some(&ctx.accounts.mint_authority.key()),
        Some(&ctx.accounts.mint_authority.key()),
        transfer_fee_basis_points,
        maximum_fee,
    )?;

    permanent_delegate_initialize(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            PermanentDelegateInitialize {
                token_program_id: ctx.accounts.token_program.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        &ctx.accounts.mint_authority.key(),
    )?;

    initialize_mint2(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            InitializeMint2 {
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        decimals,
        &ctx.accounts.mint_authority.key(),
        Some(&ctx.accounts.mint_authority.key()),
    )?;

    let mint_authority_bump = [ctx.bumps.mint_authority];
    let mint_authority_seeds: &[&[u8]] = &[MINT_AUTHORITY_SEED, &mint_authority_bump];
    let signer_seeds = &[mint_authority_seeds];

    token_metadata_initialize(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TokenMetadataInitialize {
                program_id: ctx.accounts.token_program.to_account_info(),
                metadata: ctx.accounts.mint.to_account_info(),
                update_authority: ctx.accounts.mint_authority.to_account_info(),
                mint_authority: ctx.accounts.mint_authority.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
            },
            signer_seeds,
        ),
        name,
        symbol,
        uri,
    )?;

    let mint_config = &mut ctx.accounts.mint_config;
    mint_config.admin = ctx.accounts.admin.key();
    mint_config.mint = ctx.accounts.mint.key();
    mint_config.mint_authority_bump = ctx.bumps.mint_authority;
    mint_config.decimals = decimals;

    msg!(
        "Created Token-2022 mint {} with {} bytes and metadata",
        mint_config.mint,
        final_mint_space
    );

    Ok(())
}
