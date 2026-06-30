use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, program_error::ProgramError, system_instruction},
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
};
use spl_token_metadata_interface::state::TokenMetadata;

use crate::{
    constants::{MINT_AUTHORITY_SEED, PROTOCOL_SEED, STABLECOIN_MINT_SEED, SUPPLY_STATS_SEED},
    error::ErrorCode,
    state::{GlobalSupplyStats, ProtocolConfig, StablecoinMintConfig},
};

const STANDALONE_TLV_STATE_HEADER_LEN: usize = 8;

#[derive(Accounts)]
#[instruction(decimals: u8)]
pub struct CreateStablecoinMint<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        has_one = admin
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    #[account(
        init,
        payer = admin,
        space = StablecoinMintConfig::SPACE,
        seeds = [STABLECOIN_MINT_SEED],
        bump
    )]
    pub mint_config: Account<'info, StablecoinMintConfig>,

    #[account(
        init,
        payer = admin,
        space = GlobalSupplyStats::SPACE,
        seeds = [SUPPLY_STATS_SEED],
        bump
    )]
    pub supply_stats: Account<'info, GlobalSupplyStats>,

    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump
    )]
    /// CHECK: PDA authority only; no account data is read or written.
    pub mint_authority: UncheckedAccount<'info>,

    #[account(mut)]
    pub mint: Signer<'info>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn create_stablecoin_mint_handler(
    ctx: Context<CreateStablecoinMint>,
    decimals: u8,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    require!(
        ctx.accounts.protocol_config.stablecoin_mint == Pubkey::default(),
        ErrorCode::StablecoinMintAlreadyCreated
    );

    let token_metadata = TokenMetadata {
        mint: ctx.accounts.mint.key(),
        name: name.clone(),
        symbol: symbol.clone(),
        uri: uri.clone(),
        ..Default::default()
    };
    let extensions = [
        ExtensionType::MetadataPointer,
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

    if fixed_mint_space <= PodMint::SIZE_OF {
        return Err(ProgramError::InvalidAccountData.into());
    }

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

    let protocol_config = &mut ctx.accounts.protocol_config;
    protocol_config.stablecoin_mint = ctx.accounts.mint.key();

    let mint_config = &mut ctx.accounts.mint_config;
    mint_config.protocol_config = protocol_config.key();
    mint_config.mint = ctx.accounts.mint.key();
    mint_config.mint_authority = ctx.accounts.mint_authority.key();
    mint_config.supply_stats = ctx.accounts.supply_stats.key();
    mint_config.decimals = decimals;
    mint_config.mint_authority_bump = ctx.bumps.mint_authority;
    mint_config.bump = ctx.bumps.mint_config;

    let supply_stats = &mut ctx.accounts.supply_stats;
    supply_stats.protocol_config = protocol_config.key();
    supply_stats.mint = ctx.accounts.mint.key();
    supply_stats.current_supply = 0;
    supply_stats.total_minted = 0;
    supply_stats.total_burned = 0;
    supply_stats.bump = ctx.bumps.supply_stats;

    msg!(
        "Created stablecoin mint {} with {} bytes",
        mint_config.mint,
        final_mint_space
    );

    Ok(())
}
