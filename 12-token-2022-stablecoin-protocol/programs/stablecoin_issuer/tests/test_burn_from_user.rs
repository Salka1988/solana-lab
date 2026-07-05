use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::{
        spl_token_2022::state::{Account as TokenAccount, Mint},
        ID as TOKEN_2022_PROGRAM_ID,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const GLOBAL_SUPPLY_CAP: u64 = 1_000_000_000;
const ISSUER_MINT_LIMIT: u64 = 100_000_000;
const MINT_AMOUNT: u64 = 25_000_000;
const BURN_AMOUNT: u64 = 7_000_000;
const TOKEN_DECIMALS: u8 = 6;
const TOKEN_NAME: &str = "Lab Stablecoin";
const TOKEN_SYMBOL: &str = "LABUSD";
const TOKEN_URI: &str = "https://example.com/lab-usd.json";

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_program(
        stablecoin_issuer::id(),
        include_bytes!("../../../target/deploy/stablecoin_issuer.so"),
    )
}

fn protocol_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[stablecoin_issuer::PROTOCOL_SEED],
        &stablecoin_issuer::id(),
    )
}

fn mint_config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[stablecoin_issuer::STABLECOIN_MINT_SEED],
        &stablecoin_issuer::id(),
    )
}

fn supply_stats_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[stablecoin_issuer::SUPPLY_STATS_SEED],
        &stablecoin_issuer::id(),
    )
}

fn mint_authority_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[stablecoin_issuer::MINT_AUTHORITY_SEED],
        &stablecoin_issuer::id(),
    )
}

fn issuer_config_pda(issuer_authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[stablecoin_issuer::ISSUER_SEED, issuer_authority.as_ref()],
        &stablecoin_issuer::id(),
    )
}

fn issuer_stats_pda(issuer_authority: Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            stablecoin_issuer::ISSUER_STATS_SEED,
            issuer_authority.as_ref(),
        ],
        &stablecoin_issuer::id(),
    )
}

fn initialize_protocol_ix(admin: Pubkey, protocol_config: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::InitializeProtocol {
            global_supply_cap: GLOBAL_SUPPLY_CAP,
        }
        .data(),
        stablecoin_issuer::accounts::InitializeProtocol {
            admin,
            protocol_config,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn create_stablecoin_mint_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    mint_config: Pubkey,
    supply_stats: Pubkey,
    mint_authority: Pubkey,
    mint: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::CreateStablecoinMint {
            decimals: TOKEN_DECIMALS,
            name: TOKEN_NAME.to_string(),
            symbol: TOKEN_SYMBOL.to_string(),
            uri: TOKEN_URI.to_string(),
        }
        .data(),
        stablecoin_issuer::accounts::CreateStablecoinMint {
            admin,
            protocol_config,
            mint_config,
            supply_stats,
            mint_authority,
            mint,
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn register_issuer_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    issuer_config: Pubkey,
    issuer_stats: Pubkey,
    issuer_authority: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::RegisterIssuer {
            mint_limit: ISSUER_MINT_LIMIT,
        }
        .data(),
        stablecoin_issuer::accounts::RegisterIssuer {
            admin,
            protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn set_issuer_paused_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    issuer_config: Pubkey,
    issuer_authority: Pubkey,
    paused: bool,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::SetIssuerPaused { paused }.data(),
        stablecoin_issuer::accounts::SetIssuerPaused {
            admin,
            protocol_config,
            issuer_config,
            issuer_authority,
        }
        .to_account_metas(None),
    )
}

fn mint_to_user_ix(
    fixture: &BurnFixture,
    user: Pubkey,
    user_token_account: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::MintToUser { amount }.data(),
        stablecoin_issuer::accounts::MintToUser {
            issuer_authority: fixture.issuer_authority.pubkey(),
            protocol_config: fixture.protocol_config,
            mint_config: fixture.mint_config,
            supply_stats: fixture.supply_stats,
            issuer_config: fixture.issuer_config,
            issuer_stats: fixture.issuer_stats,
            mint_authority: fixture.mint_authority,
            mint: fixture.mint.pubkey(),
            user,
            user_token_account,
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn burn_from_user_ix(
    issuer_authority: Pubkey,
    protocol_config: Pubkey,
    mint_config: Pubkey,
    supply_stats: Pubkey,
    issuer_config: Pubkey,
    issuer_stats: Pubkey,
    mint_authority: Pubkey,
    mint: Pubkey,
    user_token_account: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::BurnFromUser { amount }.data(),
        stablecoin_issuer::accounts::BurnFromUser {
            issuer_authority,
            protocol_config,
            mint_config,
            supply_stats,
            issuer_config,
            issuer_stats,
            mint_authority,
            mint,
            user_token_account,
            token_program: TOKEN_2022_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}

fn send_with_signers(
    svm: &mut LiteSVM,
    fee_payer: Pubkey,
    instruction: Instruction,
    signers: &[&Keypair],
) -> Result<(), litesvm::types::FailedTransactionMetadata> {
    test_support::send_instruction_with_signers_result(svm, fee_payer, instruction, signers)
}

struct BurnFixture {
    protocol_config: Pubkey,
    mint_config: Pubkey,
    supply_stats: Pubkey,
    mint_authority: Pubkey,
    mint: Keypair,
    issuer_authority: Keypair,
    issuer_config: Pubkey,
    issuer_stats: Pubkey,
    user: Keypair,
    user_token_account: Keypair,
}

fn initialize_fixture(svm: &mut LiteSVM, admin: &Keypair) -> BurnFixture {
    let mint = Keypair::new();
    let issuer_authority = Keypair::new();
    let user = Keypair::new();
    let user_token_account = Keypair::new();
    let (protocol_config, _) = protocol_config_pda();
    let (mint_config, _) = mint_config_pda();
    let (supply_stats, _) = supply_stats_pda();
    let (mint_authority, _) = mint_authority_pda();
    let (issuer_config, _) = issuer_config_pda(issuer_authority.pubkey());
    let (issuer_stats, _) = issuer_stats_pda(issuer_authority.pubkey());

    test_support::fund_user(svm, &issuer_authority);

    assert!(test_support::send_instruction(
        svm,
        admin,
        initialize_protocol_ix(admin.pubkey(), protocol_config),
    ));

    assert!(send_with_signers(
        svm,
        admin.pubkey(),
        create_stablecoin_mint_ix(
            admin.pubkey(),
            protocol_config,
            mint_config,
            supply_stats,
            mint_authority,
            mint.pubkey(),
        ),
        &[admin, &mint],
    )
    .is_ok());

    assert!(test_support::send_instruction(
        svm,
        admin,
        register_issuer_ix(
            admin.pubkey(),
            protocol_config,
            issuer_config,
            issuer_stats,
            issuer_authority.pubkey(),
        ),
    ));

    let fixture = BurnFixture {
        protocol_config,
        mint_config,
        supply_stats,
        mint_authority,
        mint,
        issuer_authority,
        issuer_config,
        issuer_stats,
        user,
        user_token_account,
    };

    assert!(send_with_signers(
        svm,
        fixture.issuer_authority.pubkey(),
        mint_to_user_ix(
            &fixture,
            fixture.user.pubkey(),
            fixture.user_token_account.pubkey(),
            MINT_AMOUNT,
        ),
        &[&fixture.issuer_authority, &fixture.user_token_account],
    )
    .is_ok());

    fixture
}

fn read_token_account(svm: &LiteSVM, token_account: &Pubkey) -> TokenAccount {
    test_support::token_2022_account(svm, token_account)
}

fn read_mint(svm: &LiteSVM, mint: &Pubkey) -> Mint {
    test_support::token_2022_mint(svm, mint)
}

#[test]
fn issuer_can_burn_from_user_and_updates_supply_stats() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    assert!(send_with_signers(
        &mut svm,
        fixture.issuer_authority.pubkey(),
        burn_from_user_ix(
            fixture.issuer_authority.pubkey(),
            fixture.protocol_config,
            fixture.mint_config,
            fixture.supply_stats,
            fixture.issuer_config,
            fixture.issuer_stats,
            fixture.mint_authority,
            fixture.mint.pubkey(),
            fixture.user_token_account.pubkey(),
            BURN_AMOUNT,
        ),
        &[&fixture.issuer_authority],
    )
    .is_ok());

    let token_account = read_token_account(&svm, &fixture.user_token_account.pubkey());
    let mint_state = read_mint(&svm, &fixture.mint.pubkey());
    let issuer_stats = test_support::deserialize_account::<stablecoin_issuer::IssuerStats>(
        &svm,
        &fixture.issuer_stats,
    );
    let supply_stats = test_support::deserialize_account::<stablecoin_issuer::GlobalSupplyStats>(
        &svm,
        &fixture.supply_stats,
    );

    assert_eq!(u64::from(token_account.amount), MINT_AMOUNT - BURN_AMOUNT);
    assert_eq!(u64::from(mint_state.supply), MINT_AMOUNT - BURN_AMOUNT);
    assert_eq!(issuer_stats.current_outstanding, MINT_AMOUNT - BURN_AMOUNT);
    assert_eq!(issuer_stats.total_minted, MINT_AMOUNT);
    assert_eq!(issuer_stats.total_burned, BURN_AMOUNT);
    assert_eq!(supply_stats.current_supply, MINT_AMOUNT - BURN_AMOUNT);
    assert_eq!(supply_stats.total_minted, MINT_AMOUNT);
    assert_eq!(supply_stats.total_burned, BURN_AMOUNT);
}

#[test]
fn burn_from_user_rejects_amount_above_tracked_outstanding() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    let result = send_with_signers(
        &mut svm,
        fixture.issuer_authority.pubkey(),
        burn_from_user_ix(
            fixture.issuer_authority.pubkey(),
            fixture.protocol_config,
            fixture.mint_config,
            fixture.supply_stats,
            fixture.issuer_config,
            fixture.issuer_stats,
            fixture.mint_authority,
            fixture.mint.pubkey(),
            fixture.user_token_account.pubkey(),
            MINT_AMOUNT + 1,
        ),
        &[&fixture.issuer_authority],
    );

    test_support::assert_result_fails_with(result, "BurnAmountExceedsOutstanding");
}

#[test]
fn burn_from_user_rejects_paused_issuer() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        set_issuer_paused_ix(
            admin.pubkey(),
            fixture.protocol_config,
            fixture.issuer_config,
            fixture.issuer_authority.pubkey(),
            true,
        ),
    ));

    let result = send_with_signers(
        &mut svm,
        fixture.issuer_authority.pubkey(),
        burn_from_user_ix(
            fixture.issuer_authority.pubkey(),
            fixture.protocol_config,
            fixture.mint_config,
            fixture.supply_stats,
            fixture.issuer_config,
            fixture.issuer_stats,
            fixture.mint_authority,
            fixture.mint.pubkey(),
            fixture.user_token_account.pubkey(),
            BURN_AMOUNT,
        ),
        &[&fixture.issuer_authority],
    );

    test_support::assert_result_fails_with(result, "IssuerPaused");
}

#[test]
fn burn_from_user_rejects_wrong_issuer_signer_for_config() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);
    let attacker = Keypair::new();

    test_support::fund_user(&mut svm, &attacker);

    let result = send_with_signers(
        &mut svm,
        attacker.pubkey(),
        burn_from_user_ix(
            attacker.pubkey(),
            fixture.protocol_config,
            fixture.mint_config,
            fixture.supply_stats,
            fixture.issuer_config,
            fixture.issuer_stats,
            fixture.mint_authority,
            fixture.mint.pubkey(),
            fixture.user_token_account.pubkey(),
            BURN_AMOUNT,
        ),
        &[&attacker],
    );

    test_support::assert_result_fails_with(result, "ConstraintSeeds");
}

#[test]
fn burn_from_user_rejects_wrong_mint_authority_pda() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);
    let wrong_mint_authority = Keypair::new().pubkey();

    let result = send_with_signers(
        &mut svm,
        fixture.issuer_authority.pubkey(),
        burn_from_user_ix(
            fixture.issuer_authority.pubkey(),
            fixture.protocol_config,
            fixture.mint_config,
            fixture.supply_stats,
            fixture.issuer_config,
            fixture.issuer_stats,
            wrong_mint_authority,
            fixture.mint.pubkey(),
            fixture.user_token_account.pubkey(),
            BURN_AMOUNT,
        ),
        &[&fixture.issuer_authority],
    );

    test_support::assert_result_fails_with(result, "A raw constraint was violated");
}

#[test]
fn burn_from_user_rejects_wrong_mint_account() {
    let (mut svm, admin) = setup();
    let fixture = initialize_fixture(&mut svm, &admin);
    let wrong_mint = Keypair::new().pubkey();

    let result = send_with_signers(
        &mut svm,
        fixture.issuer_authority.pubkey(),
        burn_from_user_ix(
            fixture.issuer_authority.pubkey(),
            fixture.protocol_config,
            fixture.mint_config,
            fixture.supply_stats,
            fixture.issuer_config,
            fixture.issuer_stats,
            fixture.mint_authority,
            wrong_mint,
            fixture.user_token_account.pubkey(),
            BURN_AMOUNT,
        ),
        &[&fixture.issuer_authority],
    );

    test_support::assert_result_fails_with(result, "A raw constraint was violated");
}
