use anchor_lang::{
    prelude::*,
    solana_program::{account_info::AccountInfo, program_error::ProgramError},
    AccountDeserialize, AccountSerialize,
};
use spl_transfer_hook_interface::instruction::TransferHookInstruction;

use crate::{
    constants::EXTRA_ACCOUNT_METAS_SEED,
    error::ErrorCode,
    state::{ComplianceConfig, UserCompliance},
};

const SECONDS_PER_DAY: i64 = 86_400;

pub fn execute_fallback<'info>(
    program_id: &'info Pubkey,
    accounts: &'info [AccountInfo<'info>],
    data: &'info [u8],
) -> Result<()> {
    let instruction =
        TransferHookInstruction::unpack(data).map_err(|_| ProgramError::InvalidInstructionData)?;

    match instruction {
        TransferHookInstruction::Execute { amount } => {
            execute_transfer_hook(program_id, accounts, data, amount)
        }
        _ => Err(ProgramError::InvalidInstructionData.into()),
    }
}

fn execute_transfer_hook<'info>(
    program_id: &'info Pubkey,
    accounts: &'info [AccountInfo<'info>],
    instruction_data: &'info [u8],
    amount: u64,
) -> Result<()> {
    require!(accounts.len() >= 8, ErrorCode::InvalidTransferHookAccounts);

    let source = &accounts[0];
    let mint = &accounts[1];
    let destination = &accounts[2];
    let validation = &accounts[4];
    let config_info = &accounts[5];
    let source_compliance_info = &accounts[6];
    let destination_compliance_info = &accounts[7];

    let expected_validation =
        Pubkey::find_program_address(&[EXTRA_ACCOUNT_METAS_SEED, mint.key.as_ref()], program_id).0;
    require!(
        *validation.key == expected_validation,
        ErrorCode::InvalidTransferHookAccounts
    );

    require!(
        TransferHookInstruction::unpack(instruction_data).is_ok(),
        ErrorCode::InvalidTransferHookAccounts
    );

    let expected_config = Pubkey::find_program_address(
        &[crate::COMPLIANCE_CONFIG_SEED, mint.key.as_ref()],
        program_id,
    )
    .0;
    require!(
        *config_info.key == expected_config,
        ErrorCode::InvalidTransferHookAccounts
    );

    let expected_source_profile = Pubkey::find_program_address(
        &[
            crate::USER_COMPLIANCE_SEED,
            mint.key.as_ref(),
            source.key.as_ref(),
        ],
        program_id,
    )
    .0;
    require!(
        *source_compliance_info.key == expected_source_profile,
        ErrorCode::InvalidTransferHookAccounts
    );

    let expected_destination_profile = Pubkey::find_program_address(
        &[
            crate::USER_COMPLIANCE_SEED,
            mint.key.as_ref(),
            destination.key.as_ref(),
        ],
        program_id,
    )
    .0;
    require!(
        *destination_compliance_info.key == expected_destination_profile,
        ErrorCode::InvalidTransferHookAccounts
    );

    let config = deserialize_account::<ComplianceConfig>(config_info)?;
    require!(!config.paused, ErrorCode::ProtocolPaused);
    require!(
        config.mint == *mint.key,
        ErrorCode::InvalidTransferHookAccounts
    );
    require!(
        amount <= config.max_transfer_amount,
        ErrorCode::TransferLimitExceeded
    );

    let mut source_compliance = deserialize_account::<UserCompliance>(source_compliance_info)?;
    let destination_compliance =
        deserialize_account::<UserCompliance>(destination_compliance_info)?;

    validate_profile(&source_compliance, config_info.key, &config, source.key)?;
    validate_profile(
        &destination_compliance,
        config_info.key,
        &config,
        destination.key,
    )?;

    require!(
        source_compliance.allowlisted,
        ErrorCode::SourceNotAllowlisted
    );
    require!(
        destination_compliance.allowlisted,
        ErrorCode::DestinationNotAllowlisted
    );
    require!(!source_compliance.blocked, ErrorCode::SourceBlocked);
    require!(
        !destination_compliance.blocked,
        ErrorCode::DestinationBlocked
    );
    require!(source_compliance.issuer_active, ErrorCode::IssuerInactive);

    let current_day = Clock::get()?.unix_timestamp / SECONDS_PER_DAY;
    if source_compliance.current_day != current_day {
        source_compliance.current_day = current_day;
        source_compliance.transferred_today = 0;
    }

    let new_daily_amount = source_compliance
        .transferred_today
        .checked_add(amount)
        .ok_or(ErrorCode::DailyLimitExceeded)?;
    require!(
        new_daily_amount <= config.daily_transfer_limit,
        ErrorCode::DailyLimitExceeded
    );
    source_compliance.transferred_today = new_daily_amount;

    serialize_account(source_compliance_info, &source_compliance)?;

    Ok(())
}

fn validate_profile(
    profile: &UserCompliance,
    config_key: &Pubkey,
    config: &ComplianceConfig,
    user: &Pubkey,
) -> Result<()> {
    require!(
        profile.config == *config_key,
        ErrorCode::InvalidTransferHookAccounts
    );
    require!(
        profile.mint == config.mint,
        ErrorCode::InvalidTransferHookAccounts
    );
    require!(
        profile.user == *user,
        ErrorCode::InvalidTransferHookAccounts
    );

    Ok(())
}

fn deserialize_account<T: AccountDeserialize>(account_info: &AccountInfo) -> Result<T> {
    require!(
        account_info.owner == &crate::id(),
        ErrorCode::InvalidTransferHookAccounts
    );

    T::try_deserialize(&mut account_info.try_borrow_data()?.as_ref())
}

fn serialize_account<T: AccountSerialize>(account_info: &AccountInfo, account: &T) -> Result<()> {
    require!(
        account_info.is_writable,
        ErrorCode::InvalidTransferHookAccounts
    );

    account.try_serialize(&mut account_info.try_borrow_mut_data()?.as_mut())
}
