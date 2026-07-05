use solana_m0_test_support as test_support;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::ID as TOKEN_2022_PROGRAM_ID,
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const TOKEN_DECIMALS: u8 = 6;
const TOKEN_NAME: &str = "Lab Stablecoin";
const TOKEN_SYMBOL: &str = "LABUSD";
const TOKEN_URI: &str = "https://example.com/lab-usd.json";

struct ProtocolFixture {
    protocol_config: Pubkey,
    mint_config: Pubkey,
    supply_stats: Pubkey,
    mint_authority: Pubkey,
    mint: Keypair,
}

struct IssuerFixture {
    authority: Keypair,
    config: Pubkey,
    stats: Pubkey,
}

fn setup() -> (LiteSVM, Keypair) {
    test_support::new_svm_with_program(
        stablecoin_issuer::id(),
        include_bytes!("../../../target/deploy/stablecoin_issuer.so"),
    )
}

fn protocol_config_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::PROTOCOL_SEED],
        &stablecoin_issuer::id(),
    )
    .0
}

fn mint_config_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::STABLECOIN_MINT_SEED],
        &stablecoin_issuer::id(),
    )
    .0
}

fn supply_stats_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::SUPPLY_STATS_SEED],
        &stablecoin_issuer::id(),
    )
    .0
}

fn mint_authority_pda() -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::MINT_AUTHORITY_SEED],
        &stablecoin_issuer::id(),
    )
    .0
}

fn issuer_config_pda(authority: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::ISSUER_SEED, authority.as_ref()],
        &stablecoin_issuer::id(),
    )
    .0
}

fn issuer_stats_pda(authority: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[stablecoin_issuer::ISSUER_STATS_SEED, authority.as_ref()],
        &stablecoin_issuer::id(),
    )
    .0
}

fn initialize_protocol_ix(admin: Pubkey, protocol_config: Pubkey, cap: u64) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::InitializeProtocol {
            global_supply_cap: cap,
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

fn create_stablecoin_mint_ix(admin: Pubkey, fixture: &ProtocolFixture) -> Instruction {
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
            protocol_config: fixture.protocol_config,
            mint_config: fixture.mint_config,
            supply_stats: fixture.supply_stats,
            mint_authority: fixture.mint_authority,
            mint: fixture.mint.pubkey(),
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn register_issuer_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    issuer: &IssuerFixture,
    mint_limit: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::RegisterIssuer { mint_limit }.data(),
        stablecoin_issuer::accounts::RegisterIssuer {
            admin,
            protocol_config,
            issuer_config: issuer.config,
            issuer_stats: issuer.stats,
            issuer_authority: issuer.authority.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn rotate_issuer_authority_ix(
    admin: Pubkey,
    protocol_config: Pubkey,
    current: &IssuerFixture,
    new: &IssuerFixture,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::RotateIssuerAuthority {}.data(),
        stablecoin_issuer::accounts::RotateIssuerAuthority {
            admin,
            protocol_config,
            current_issuer_config: current.config,
            current_issuer_stats: current.stats,
            new_issuer_config: new.config,
            new_issuer_stats: new.stats,
            current_authority: current.authority.pubkey(),
            new_authority: new.authority.pubkey(),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn mint_to_user_ix(
    protocol: &ProtocolFixture,
    issuer: &IssuerFixture,
    user: Pubkey,
    user_token_account: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::MintToUser { amount }.data(),
        stablecoin_issuer::accounts::MintToUser {
            issuer_authority: issuer.authority.pubkey(),
            protocol_config: protocol.protocol_config,
            mint_config: protocol.mint_config,
            supply_stats: protocol.supply_stats,
            issuer_config: issuer.config,
            issuer_stats: issuer.stats,
            mint_authority: protocol.mint_authority,
            mint: protocol.mint.pubkey(),
            user,
            user_token_account,
            token_program: TOKEN_2022_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn burn_from_user_ix(
    protocol: &ProtocolFixture,
    issuer: &IssuerFixture,
    user_token_account: Pubkey,
    amount: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        stablecoin_issuer::id(),
        &stablecoin_issuer::instruction::BurnFromUser { amount }.data(),
        stablecoin_issuer::accounts::BurnFromUser {
            issuer_authority: issuer.authority.pubkey(),
            protocol_config: protocol.protocol_config,
            mint_config: protocol.mint_config,
            supply_stats: protocol.supply_stats,
            issuer_config: issuer.config,
            issuer_stats: issuer.stats,
            mint_authority: protocol.mint_authority,
            mint: protocol.mint.pubkey(),
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

fn new_issuer() -> IssuerFixture {
    let authority = Keypair::new();
    IssuerFixture {
        config: issuer_config_pda(authority.pubkey()),
        stats: issuer_stats_pda(authority.pubkey()),
        authority,
    }
}

fn initialize_protocol_fixture(svm: &mut LiteSVM, admin: &Keypair, cap: u64) -> ProtocolFixture {
    let fixture = ProtocolFixture {
        protocol_config: protocol_config_pda(),
        mint_config: mint_config_pda(),
        supply_stats: supply_stats_pda(),
        mint_authority: mint_authority_pda(),
        mint: Keypair::new(),
    };

    assert!(test_support::send_instruction(
        svm,
        admin,
        initialize_protocol_ix(admin.pubkey(), fixture.protocol_config, cap),
    ));
    assert!(send_with_signers(
        svm,
        admin.pubkey(),
        create_stablecoin_mint_ix(admin.pubkey(), &fixture),
        &[admin, &fixture.mint],
    )
    .is_ok());

    fixture
}

fn register_issuer(
    svm: &mut LiteSVM,
    admin: &Keypair,
    protocol_config: Pubkey,
    issuer: &IssuerFixture,
    mint_limit: u64,
) {
    test_support::fund_user(svm, &issuer.authority);
    assert!(test_support::send_instruction(
        svm,
        admin,
        register_issuer_ix(admin.pubkey(), protocol_config, issuer, mint_limit),
    ));
}

#[test]
fn multiple_issuers_share_one_global_supply_cap() {
    let (mut svm, admin) = setup();
    let protocol = initialize_protocol_fixture(&mut svm, &admin, 100);
    let issuer_a = new_issuer();
    let issuer_b = new_issuer();

    register_issuer(&mut svm, &admin, protocol.protocol_config, &issuer_a, 100);
    register_issuer(&mut svm, &admin, protocol.protocol_config, &issuer_b, 100);

    let user_a = Keypair::new();
    let token_a = Keypair::new();
    assert!(send_with_signers(
        &mut svm,
        issuer_a.authority.pubkey(),
        mint_to_user_ix(&protocol, &issuer_a, user_a.pubkey(), token_a.pubkey(), 70),
        &[&issuer_a.authority, &token_a],
    )
    .is_ok());

    let rejected_user = Keypair::new();
    let rejected_token = Keypair::new();
    let result = send_with_signers(
        &mut svm,
        issuer_b.authority.pubkey(),
        mint_to_user_ix(
            &protocol,
            &issuer_b,
            rejected_user.pubkey(),
            rejected_token.pubkey(),
            31,
        ),
        &[&issuer_b.authority, &rejected_token],
    );
    test_support::assert_result_fails_with(result, "GlobalSupplyCapExceeded");

    let user_b = Keypair::new();
    let token_b = Keypair::new();
    assert!(send_with_signers(
        &mut svm,
        issuer_b.authority.pubkey(),
        mint_to_user_ix(&protocol, &issuer_b, user_b.pubkey(), token_b.pubkey(), 30),
        &[&issuer_b.authority, &token_b],
    )
    .is_ok());

    let supply_stats = test_support::deserialize_account::<stablecoin_issuer::GlobalSupplyStats>(
        &svm,
        &protocol.supply_stats,
    );
    let issuer_a_stats =
        test_support::deserialize_account::<stablecoin_issuer::IssuerStats>(&svm, &issuer_a.stats);
    let issuer_b_stats =
        test_support::deserialize_account::<stablecoin_issuer::IssuerStats>(&svm, &issuer_b.stats);

    assert_eq!(supply_stats.current_supply, 100);
    assert_eq!(supply_stats.total_minted, 100);
    assert_eq!(issuer_a_stats.current_outstanding, 70);
    assert_eq!(issuer_b_stats.current_outstanding, 30);
}

#[test]
fn rotated_issuer_can_burn_prior_outstanding_and_old_authority_is_retired() {
    let (mut svm, admin) = setup();
    let protocol = initialize_protocol_fixture(&mut svm, &admin, 1_000);
    let old_issuer = new_issuer();
    let new_issuer = new_issuer();
    let user = Keypair::new();
    let user_token_account = Keypair::new();

    register_issuer(
        &mut svm,
        &admin,
        protocol.protocol_config,
        &old_issuer,
        1_000,
    );

    assert!(send_with_signers(
        &mut svm,
        old_issuer.authority.pubkey(),
        mint_to_user_ix(
            &protocol,
            &old_issuer,
            user.pubkey(),
            user_token_account.pubkey(),
            400,
        ),
        &[&old_issuer.authority, &user_token_account],
    )
    .is_ok());

    assert!(test_support::send_instruction(
        &mut svm,
        &admin,
        rotate_issuer_authority_ix(
            admin.pubkey(),
            protocol.protocol_config,
            &old_issuer,
            &new_issuer,
        ),
    ));

    let rejected_token = Keypair::new();
    let result = send_with_signers(
        &mut svm,
        old_issuer.authority.pubkey(),
        mint_to_user_ix(
            &protocol,
            &old_issuer,
            user.pubkey(),
            rejected_token.pubkey(),
            1,
        ),
        &[&old_issuer.authority, &rejected_token],
    );
    test_support::assert_result_fails_with(result, "IssuerPaused");

    test_support::fund_user(&mut svm, &new_issuer.authority);
    assert!(send_with_signers(
        &mut svm,
        new_issuer.authority.pubkey(),
        burn_from_user_ix(&protocol, &new_issuer, user_token_account.pubkey(), 150),
        &[&new_issuer.authority],
    )
    .is_ok());

    let old_config = test_support::deserialize_account::<stablecoin_issuer::IssuerConfig>(
        &svm,
        &old_issuer.config,
    );
    let new_stats = test_support::deserialize_account::<stablecoin_issuer::IssuerStats>(
        &svm,
        &new_issuer.stats,
    );
    let supply_stats = test_support::deserialize_account::<stablecoin_issuer::GlobalSupplyStats>(
        &svm,
        &protocol.supply_stats,
    );

    assert!(old_config.paused);
    assert_eq!(old_config.mint_limit, 0);
    assert_eq!(new_stats.current_outstanding, 250);
    assert_eq!(new_stats.total_minted, 400);
    assert_eq!(new_stats.total_burned, 150);
    assert_eq!(supply_stats.current_supply, 250);
    assert_eq!(supply_stats.total_minted, 400);
    assert_eq!(supply_stats.total_burned, 150);
}
