use anchor_lang::{prelude::Pubkey, AccountSerialize};

fn serialized_len<T: AccountSerialize>(account: &T) -> usize {
    let mut data = Vec::new();
    account.try_serialize(&mut data).unwrap();
    data.len()
}

#[test]
fn protocol_config_space_matches_serialized_len() {
    let account = modular_issuer::ProtocolConfig {
        admin: Pubkey::new_unique(),
        pending_admin: Pubkey::new_unique(),
        stablecoin_mint: Pubkey::new_unique(),
        global_supply_cap: 1_000_000,
        paused: false,
        bump: 254,
    };

    assert_eq!(
        serialized_len(&account),
        modular_issuer::ProtocolConfig::SPACE
    );
}

#[test]
fn stablecoin_mint_config_space_matches_serialized_len() {
    let account = modular_issuer::StablecoinMintConfig {
        protocol_config: Pubkey::new_unique(),
        mint: Pubkey::new_unique(),
        mint_authority: Pubkey::new_unique(),
        supply_stats: Pubkey::new_unique(),
        decimals: 6,
        mint_authority_bump: 253,
        bump: 252,
    };

    assert_eq!(
        serialized_len(&account),
        modular_issuer::StablecoinMintConfig::SPACE
    );
}

#[test]
fn global_supply_stats_space_matches_serialized_len() {
    let account = modular_issuer::GlobalSupplyStats {
        protocol_config: Pubkey::new_unique(),
        mint: Pubkey::new_unique(),
        current_supply: 500_000,
        total_minted: 750_000,
        total_burned: 250_000,
        bump: 251,
    };

    assert_eq!(
        serialized_len(&account),
        modular_issuer::GlobalSupplyStats::SPACE
    );
}

#[test]
fn issuer_config_space_matches_serialized_len() {
    let account = modular_issuer::IssuerConfig {
        protocol_config: Pubkey::new_unique(),
        authority: Pubkey::new_unique(),
        stats: Pubkey::new_unique(),
        mint_limit: 1_000_000,
        paused: false,
        bump: 250,
    };

    assert_eq!(
        serialized_len(&account),
        modular_issuer::IssuerConfig::SPACE
    );
}

#[test]
fn issuer_stats_space_matches_serialized_len() {
    let account = modular_issuer::IssuerStats {
        protocol_config: Pubkey::new_unique(),
        issuer_config: Pubkey::new_unique(),
        authority: Pubkey::new_unique(),
        current_outstanding: 100_000,
        total_minted: 250_000,
        total_burned: 150_000,
        bump: 249,
    };

    assert_eq!(serialized_len(&account), modular_issuer::IssuerStats::SPACE);
}
