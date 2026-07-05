use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::{
        spl_token_2022::{
            extension::permanent_delegate::PermanentDelegate,
            extension::transfer_fee::TransferFeeConfig,
            extension::{
                metadata_pointer::MetadataPointer, BaseStateWithExtensions, ExtensionType,
                StateWithExtensions,
            },
            state::Mint,
        },
        ID as TOKEN_2022_PROGRAM_ID,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    spl_token_metadata_interface::state::TokenMetadata,
    token_2022_factory::Token2022MintConfig,
};

const TOKEN_DECIMALS: u8 = 6;
const TOKEN_NAME: &str = "Lab Stablecoin";
const TOKEN_SYMBOL: &str = "LABUSD";
const TOKEN_URI: &str = "https://example.com/lab-usd.json";
const TRANSFER_FEE_BASIS_POINTS: u16 = 25;
const MAXIMUM_FEE: u64 = 1_000_000;
const STANDALONE_TLV_STATE_HEADER_LEN: usize = 8;

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_program(
        token_2022_factory::id(),
        include_bytes!("../../../target/deploy/token_2022_factory.so"),
    )
}

fn mint_config_pda(mint: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            token_2022_factory::TOKEN_2022_MINT_CONFIG_SEED,
            mint.as_ref(),
        ],
        &token_2022_factory::id(),
    )
}

fn mint_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[token_2022_factory::MINT_AUTHORITY_SEED],
        &token_2022_factory::id(),
    )
}

fn create_token_2022_mint_ix(
    admin: Pubkey,
    mint_config: Pubkey,
    mint_authority: Pubkey,
    mint: Pubkey,
    decimals: u8,
    name: &str,
    symbol: &str,
    uri: &str,
    transfer_fee_basis_points: u16,
    maximum_fee: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        token_2022_factory::id(),
        &token_2022_factory::instruction::CreateToken2022Mint {
            decimals,
            name: name.to_string(),
            symbol: symbol.to_string(),
            uri: uri.to_string(),
            transfer_fee_basis_points,
            maximum_fee,
        }
        .data(),
        token_2022_factory::accounts::CreateToken2022Mint {
            admin,
            mint_config,
            mint_authority,
            mint,
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn send_create_token_2022_mint(
    svm: &mut LiteSVM,
    admin: &Keypair,
    mint: &Keypair,
    instruction: Instruction,
) -> bool {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[instruction], Some(&admin.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[admin, mint]).unwrap();

    match svm.send_transaction(tx) {
        Ok(meta) => {
            println!("{}", meta.pretty_logs());
            true
        }
        Err(err) => {
            println!("{err:#?}");
            false
        }
    }
}

fn read_mint_config(svm: &LiteSVM, mint_config: &Pubkey) -> Token2022MintConfig {
    test_support::deserialize_account(svm, mint_config)
}

#[test]
fn create_token_2022_mint_initializes_metadata_transfer_fee_and_permanent_delegate() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let (mint_config, _) = mint_config_pda(mint.pubkey());
    let (mint_authority, _) = mint_authority_pda();

    assert!(send_create_token_2022_mint(
        &mut svm,
        &admin,
        &mint,
        create_token_2022_mint_ix(
            admin.pubkey(),
            mint_config,
            mint_authority,
            mint.pubkey(),
            TOKEN_DECIMALS,
            TOKEN_NAME,
            TOKEN_SYMBOL,
            TOKEN_URI,
            TRANSFER_FEE_BASIS_POINTS,
            MAXIMUM_FEE,
        ),
    ));

    let mint_account = svm.get_account(&mint.pubkey()).expect("mint exists");
    let mint_state = StateWithExtensions::<Mint>::unpack(&mint_account.data).unwrap();
    let metadata_pointer = mint_state.get_extension::<MetadataPointer>().unwrap();
    let transfer_fee_config = mint_state.get_extension::<TransferFeeConfig>().unwrap();
    let permanent_delegate = mint_state.get_extension::<PermanentDelegate>().unwrap();
    let token_metadata = mint_state
        .get_variable_len_extension::<TokenMetadata>()
        .unwrap();
    let expected_metadata = TokenMetadata {
        mint: mint.pubkey(),
        name: TOKEN_NAME.to_string(),
        symbol: TOKEN_SYMBOL.to_string(),
        uri: TOKEN_URI.to_string(),
        ..Default::default()
    };
    let expected_mint_len = ExtensionType::try_calculate_account_len::<Mint>(&[
        ExtensionType::MetadataPointer,
        ExtensionType::TransferFeeConfig,
        ExtensionType::PermanentDelegate,
    ])
    .unwrap()
        + expected_metadata.tlv_size_of().unwrap()
        - STANDALONE_TLV_STATE_HEADER_LEN;
    let config = read_mint_config(&svm, &mint_config);

    assert_eq!(mint_account.owner, TOKEN_2022_PROGRAM_ID);
    assert_eq!(mint_account.data.len(), expected_mint_len);
    assert_eq!(mint_state.base.decimals, TOKEN_DECIMALS);
    assert_eq!(mint_state.base.supply, 0);
    assert_eq!(mint_state.base.mint_authority, Some(mint_authority).into());
    assert_eq!(
        mint_state.base.freeze_authority,
        Some(mint_authority).into()
    );
    assert_eq!(metadata_pointer.metadata_address.0, mint.pubkey());
    assert_eq!(metadata_pointer.authority.0, mint_authority);
    assert_eq!(
        transfer_fee_config.transfer_fee_config_authority.0,
        mint_authority
    );
    assert_eq!(
        transfer_fee_config.withdraw_withheld_authority.0,
        mint_authority
    );
    assert_eq!(
        u16::from(
            transfer_fee_config
                .newer_transfer_fee
                .transfer_fee_basis_points
        ),
        TRANSFER_FEE_BASIS_POINTS
    );
    assert_eq!(
        u64::from(transfer_fee_config.newer_transfer_fee.maximum_fee),
        MAXIMUM_FEE
    );
    assert_eq!(permanent_delegate.delegate.0, mint_authority);
    assert_eq!(token_metadata.mint, mint.pubkey());
    assert_eq!(token_metadata.name, TOKEN_NAME);
    assert_eq!(token_metadata.symbol, TOKEN_SYMBOL);
    assert_eq!(token_metadata.uri, TOKEN_URI);
    assert_eq!(config.admin, admin.pubkey());
    assert_eq!(config.mint, mint.pubkey());
    assert_eq!(config.decimals, TOKEN_DECIMALS);
}

#[test]
fn wrong_mint_authority_pda_is_rejected() {
    let (mut svm, admin) = setup();
    let mint = Keypair::new();
    let (mint_config, _) = mint_config_pda(mint.pubkey());
    let wrong_mint_authority = Keypair::new().pubkey();

    assert!(!send_create_token_2022_mint(
        &mut svm,
        &admin,
        &mint,
        create_token_2022_mint_ix(
            admin.pubkey(),
            mint_config,
            wrong_mint_authority,
            mint.pubkey(),
            TOKEN_DECIMALS,
            TOKEN_NAME,
            TOKEN_SYMBOL,
            TOKEN_URI,
            TRANSFER_FEE_BASIS_POINTS,
            MAXIMUM_FEE,
        ),
    ));
}
