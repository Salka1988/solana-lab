use anchor_spl::token_2022::spl_token_2022::{extension::ExtensionType, pod::PodMint};

#[test]
fn mint_extensions_increase_account_size() {
    let base_mint_len = ExtensionType::try_calculate_account_len::<PodMint>(&[]).unwrap();
    let metadata_pointer_len =
        ExtensionType::try_calculate_account_len::<PodMint>(&[ExtensionType::MetadataPointer])
            .unwrap();
    let stablecoin_mint_len = ExtensionType::try_calculate_account_len::<PodMint>(&[
        ExtensionType::MetadataPointer,
        ExtensionType::TransferFeeConfig,
        ExtensionType::PermanentDelegate,
    ])
    .unwrap();

    println!("base mint len: {base_mint_len}");
    println!("metadata pointer mint len: {metadata_pointer_len}");
    println!("stablecoin mint len: {stablecoin_mint_len}");

    assert!(metadata_pointer_len > base_mint_len);
    assert!(stablecoin_mint_len > metadata_pointer_len);
}

#[test]
fn transfer_fee_mint_requires_token_account_fee_extension() {
    let mint_extensions = [ExtensionType::TransferFeeConfig];
    let required_account_extensions =
        ExtensionType::get_required_init_account_extensions(&mint_extensions);

    println!(
        "account extensions required by transfer fee mint: {:?}",
        required_account_extensions
    );

    assert_eq!(
        required_account_extensions,
        vec![ExtensionType::TransferFeeAmount]
    );
}
