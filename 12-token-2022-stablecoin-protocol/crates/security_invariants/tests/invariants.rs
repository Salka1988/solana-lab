use proptest::prelude::*;
use security_invariants::{
    AdminTransferModel, IssuanceModel, ProtocolError, RedemptionModel, RedemptionStatus,
    TransferHookAccounts,
};

#[derive(Clone, Copy, Debug)]
enum IssuanceAction {
    Mint(u64),
    Burn(u64),
}

prop_compose! {
    fn issuance_action()(is_mint in any::<bool>(), amount in 0_u64..1_000_000) -> IssuanceAction {
        if is_mint {
            IssuanceAction::Mint(amount)
        } else {
            IssuanceAction::Burn(amount)
        }
    }
}

proptest! {
    #[test]
    fn issuance_caps_and_supply_equation_hold(
        global_cap in 1_u64..5_000_000,
        issuer_limit in 1_u64..5_000_000,
        actions in prop::collection::vec(issuance_action(), 1..128),
    ) {
        let mut model = IssuanceModel::new(global_cap, issuer_limit);

        for action in actions {
            match action {
                IssuanceAction::Mint(amount) => {
                    let _ = model.mint(amount);
                }
                IssuanceAction::Burn(amount) => {
                    let _ = model.burn(amount);
                }
            }

            model.assert_invariants();
        }
    }

    #[test]
    fn redemption_request_has_one_terminal_state(
        amount in 1_u64..1_000_000,
        actions in prop::collection::vec(0_u8..3, 1..32),
    ) {
        let mut model = RedemptionModel::new(amount);

        for action in actions {
            let result = match action {
                0 => model.cancel(),
                1 => model.complete(),
                _ => model.reject(),
            };

            if model.status != RedemptionStatus::Pending {
                assert!(result.is_ok() || result == Err(ProtocolError::RequestNotPending));
            }

            model.assert_invariants();
        }
    }

    #[test]
    fn admin_transfer_only_pending_admin_can_accept(
        current_admin in 1_u64..1_000_000,
        pending_admin in 1_u64..1_000_000,
        attacker in 1_u64..1_000_000,
    ) {
        prop_assume!(current_admin != pending_admin);
        prop_assume!(attacker != pending_admin);

        let mut model = AdminTransferModel::new(current_admin);

        assert_eq!(
            model.begin(attacker, pending_admin),
            Err(ProtocolError::Unauthorized)
        );
        assert!(model.begin(current_admin, pending_admin).is_ok());
        assert_eq!(model.accept(attacker), Err(ProtocolError::Unauthorized));
        assert_eq!(model.admin, current_admin);
        assert!(model.accept(pending_admin).is_ok());
        assert_eq!(model.admin, pending_admin);
        assert_eq!(model.pending_admin, None);
    }

    #[test]
    fn transfer_hook_rejects_compliance_account_substitution(
        source_token_account in 1_u64..1_000_000,
        destination_token_account in 1_u64..1_000_000,
        fake_source_user in 1_u64..1_000_000,
        fake_destination_user in 1_u64..1_000_000,
    ) {
        let valid = TransferHookAccounts {
            source_token_account,
            destination_token_account,
            source_compliance_user: source_token_account,
            destination_compliance_user: destination_token_account,
        };
        assert!(valid.validate().is_ok());

        if fake_source_user != source_token_account {
            let invalid_source = TransferHookAccounts {
                source_compliance_user: fake_source_user,
                ..valid
            };
            assert_eq!(invalid_source.validate(), Err(ProtocolError::Unauthorized));
        }

        if fake_destination_user != destination_token_account {
            let invalid_destination = TransferHookAccounts {
                destination_compliance_user: fake_destination_user,
                ..valid
            };
            assert_eq!(invalid_destination.validate(), Err(ProtocolError::Unauthorized));
        }
    }
}

#[test]
fn issuance_overflow_attempt_is_rejected_without_state_change() {
    let mut model = IssuanceModel::new(u64::MAX, u64::MAX);

    assert!(model.mint(u64::MAX).is_ok());
    let before = model;
    assert_eq!(model.mint(1), Err(ProtocolError::Overflow));
    assert_eq!(model, before);
}

#[test]
fn redemption_completion_replay_is_rejected() {
    let mut model = RedemptionModel::new(500);

    assert!(model.complete().is_ok());
    assert_eq!(model.complete(), Err(ProtocolError::RequestNotPending));
    assert_eq!(model.cancel(), Err(ProtocolError::RequestNotPending));
    model.assert_invariants();
}
