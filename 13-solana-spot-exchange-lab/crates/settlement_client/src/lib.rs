#![forbid(unsafe_code)]

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
    InstructionData, ToAccountMetas,
};
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_ed25519_program::new_ed25519_instruction_with_signature;

pub const TRUSTED_SETTLEMENT_COMPUTE_UNIT_LIMIT: u32 = 25_000;
pub const SIGNED_SETTLEMENT_COMPUTE_UNIT_LIMIT: u32 = 100_000;
pub const CANCEL_SIGNED_ORDER_COMPUTE_UNIT_LIMIT: u32 = 40_000;
pub const DEFAULT_PRIORITY_FEE_MICRO_LAMPORTS: u64 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComputeBudgetPreset {
    pub unit_limit: u32,
    pub micro_lamports: u64,
}

impl ComputeBudgetPreset {
    pub const fn trusted_settlement() -> Self {
        Self {
            unit_limit: TRUSTED_SETTLEMENT_COMPUTE_UNIT_LIMIT,
            micro_lamports: DEFAULT_PRIORITY_FEE_MICRO_LAMPORTS,
        }
    }

    pub const fn signed_settlement() -> Self {
        Self {
            unit_limit: SIGNED_SETTLEMENT_COMPUTE_UNIT_LIMIT,
            micro_lamports: DEFAULT_PRIORITY_FEE_MICRO_LAMPORTS,
        }
    }

    pub const fn cancel_signed_order() -> Self {
        Self {
            unit_limit: CANCEL_SIGNED_ORDER_COMPUTE_UNIT_LIMIT,
            micro_lamports: DEFAULT_PRIORITY_FEE_MICRO_LAMPORTS,
        }
    }

    pub const fn with_priority_fee(mut self, micro_lamports: u64) -> Self {
        self.micro_lamports = micro_lamports;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedSettlementAccounts {
    pub settlement_authority: Pubkey,
    pub market_config: Pubkey,
    pub buyer: Pubkey,
    pub seller: Pubkey,
    pub buyer_balance: Pubkey,
    pub seller_balance: Pubkey,
    pub payer: Pubkey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelSignedOrderAccounts {
    pub trader: Pubkey,
    pub market_config: Pubkey,
    pub payer: Pubkey,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SignedSettlementInstructionBuilder {
    prefix_instructions: Vec<Instruction>,
}

impl SignedSettlementInstructionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_prefix_instruction(mut self, instruction: Instruction) -> Self {
        self.prefix_instructions.push(instruction);
        self
    }

    pub fn with_compute_unit_limit(self, units: u32) -> Self {
        self.with_prefix_instruction(ComputeBudgetInstruction::set_compute_unit_limit(units))
    }

    pub fn with_compute_unit_price(self, micro_lamports: u64) -> Self {
        self.with_prefix_instruction(ComputeBudgetInstruction::set_compute_unit_price(
            micro_lamports,
        ))
    }

    pub fn with_compute_budget(self, units: u32, micro_lamports: u64) -> Self {
        self.with_compute_unit_limit(units)
            .with_compute_unit_price(micro_lamports)
    }

    pub fn with_compute_budget_preset(self, preset: ComputeBudgetPreset) -> Self {
        self.with_compute_budget(preset.unit_limit, preset.micro_lamports)
    }

    pub fn with_signed_settlement_compute_budget(self) -> Self {
        self.with_compute_budget_preset(ComputeBudgetPreset::signed_settlement())
    }

    pub fn build(
        self,
        accounts: SignedSettlementAccounts,
        args: spot_settlement::SignedFillArgs,
    ) -> Vec<Instruction> {
        let mut instructions = self.prefix_instructions;
        instructions.push(buyer_signature_instruction(accounts.buyer, args));
        instructions.push(seller_signature_instruction(accounts.seller, args));
        instructions.push(settle_signed_fill_instruction(accounts, args));
        instructions
    }
}

pub fn signed_settlement_instructions(
    accounts: SignedSettlementAccounts,
    args: spot_settlement::SignedFillArgs,
) -> Vec<Instruction> {
    SignedSettlementInstructionBuilder::new().build(accounts, args)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CancelSignedOrderInstructionBuilder {
    prefix_instructions: Vec<Instruction>,
}

impl CancelSignedOrderInstructionBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_prefix_instruction(mut self, instruction: Instruction) -> Self {
        self.prefix_instructions.push(instruction);
        self
    }

    pub fn with_compute_unit_limit(self, units: u32) -> Self {
        self.with_prefix_instruction(ComputeBudgetInstruction::set_compute_unit_limit(units))
    }

    pub fn with_compute_unit_price(self, micro_lamports: u64) -> Self {
        self.with_prefix_instruction(ComputeBudgetInstruction::set_compute_unit_price(
            micro_lamports,
        ))
    }

    pub fn with_compute_budget(self, units: u32, micro_lamports: u64) -> Self {
        self.with_compute_unit_limit(units)
            .with_compute_unit_price(micro_lamports)
    }

    pub fn with_compute_budget_preset(self, preset: ComputeBudgetPreset) -> Self {
        self.with_compute_budget(preset.unit_limit, preset.micro_lamports)
    }

    pub fn with_cancel_signed_order_compute_budget(self) -> Self {
        self.with_compute_budget_preset(ComputeBudgetPreset::cancel_signed_order())
    }

    pub fn build(
        self,
        accounts: CancelSignedOrderAccounts,
        order_hash: [u8; 32],
        order: spot_settlement::SignedOrderPayload,
    ) -> Vec<Instruction> {
        let mut instructions = self.prefix_instructions;
        instructions.push(cancel_signed_order_instruction(accounts, order_hash, order));
        instructions
    }
}

pub fn cancel_signed_order_instructions(
    accounts: CancelSignedOrderAccounts,
    order_hash: [u8; 32],
    order: spot_settlement::SignedOrderPayload,
) -> Vec<Instruction> {
    CancelSignedOrderInstructionBuilder::new().build(accounts, order_hash, order)
}

fn buyer_signature_instruction(
    buyer: Pubkey,
    args: spot_settlement::SignedFillArgs,
) -> Instruction {
    new_ed25519_instruction_with_signature(
        &args.buyer_order.signing_preimage(),
        &args.buyer_signature,
        buyer.as_array(),
    )
}

fn seller_signature_instruction(
    seller: Pubkey,
    args: spot_settlement::SignedFillArgs,
) -> Instruction {
    new_ed25519_instruction_with_signature(
        &args.seller_order.signing_preimage(),
        &args.seller_signature,
        seller.as_array(),
    )
}

fn settle_signed_fill_instruction(
    accounts: SignedSettlementAccounts,
    args: spot_settlement::SignedFillArgs,
) -> Instruction {
    Instruction::new_with_bytes(
        spot_settlement::id(),
        &spot_settlement::instruction::SettleSignedFill {
            settlement_id: args.settlement_id,
            buyer_order_hash: args.buyer_order_hash,
            seller_order_hash: args.seller_order_hash,
            args,
        }
        .data(),
        spot_settlement::accounts::SettleSignedFill {
            settlement_authority: accounts.settlement_authority,
            market_config: accounts.market_config,
            buyer: accounts.buyer,
            seller: accounts.seller,
            buyer_balance: accounts.buyer_balance,
            seller_balance: accounts.seller_balance,
            buyer_order_fill_state: order_fill_state_pda(
                accounts.market_config,
                args.buyer_order_hash,
            ),
            seller_order_fill_state: order_fill_state_pda(
                accounts.market_config,
                args.seller_order_hash,
            ),
            settlement_receipt: settlement_receipt_pda(accounts.market_config, args.settlement_id),
            payer: accounts.payer,
            instructions_sysvar: solana_sdk_ids::sysvar::instructions::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn cancel_signed_order_instruction(
    accounts: CancelSignedOrderAccounts,
    order_hash: [u8; 32],
    order: spot_settlement::SignedOrderPayload,
) -> Instruction {
    Instruction::new_with_bytes(
        spot_settlement::id(),
        &spot_settlement::instruction::CancelSignedOrder { order_hash, order }.data(),
        spot_settlement::accounts::CancelSignedOrder {
            trader: accounts.trader,
            market_config: accounts.market_config,
            order_fill_state: order_fill_state_pda(accounts.market_config, order_hash),
            payer: accounts.payer,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn settlement_receipt_pda(market_config: Pubkey, settlement_id: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            spot_settlement::SETTLEMENT_RECEIPT_SEED,
            market_config.as_ref(),
            &settlement_id.to_le_bytes(),
        ],
        &spot_settlement::id(),
    )
    .0
}

fn order_fill_state_pda(market_config: Pubkey, order_hash: [u8; 32]) -> Pubkey {
    Pubkey::find_program_address(
        &[
            spot_settlement::ORDER_FILL_STATE_SEED,
            market_config.as_ref(),
            order_hash.as_ref(),
        ],
        &spot_settlement::id(),
    )
    .0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pubkey(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn signed_order(
        market_config: Pubkey,
        trader: Pubkey,
        side: spot_settlement::SignedOrderSide,
    ) -> spot_settlement::SignedOrderPayload {
        spot_settlement::SignedOrderPayload {
            order_id: 1,
            market_config,
            trader,
            side,
            price: 20,
            quantity: 10,
            nonce: 1,
            expiry_slot: u64::MAX,
        }
    }

    fn signed_fill_args() -> spot_settlement::SignedFillArgs {
        let market_config = pubkey(1);
        let buyer_order = signed_order(
            market_config,
            pubkey(2),
            spot_settlement::SignedOrderSide::Bid,
        );
        let seller_order = signed_order(
            market_config,
            pubkey(3),
            spot_settlement::SignedOrderSide::Ask,
        );

        spot_settlement::SignedFillArgs {
            settlement_id: 1,
            fill_price: 20,
            fill_quantity: 10,
            buyer_order_hash: [4; 32],
            seller_order_hash: [5; 32],
            buyer_order,
            buyer_signature: [6; 64],
            seller_order,
            seller_signature: [7; 64],
        }
    }

    fn accounts() -> SignedSettlementAccounts {
        SignedSettlementAccounts {
            settlement_authority: pubkey(8),
            market_config: pubkey(1),
            buyer: pubkey(2),
            seller: pubkey(3),
            buyer_balance: pubkey(9),
            seller_balance: pubkey(10),
            payer: pubkey(8),
        }
    }

    fn cancel_accounts() -> CancelSignedOrderAccounts {
        CancelSignedOrderAccounts {
            trader: pubkey(2),
            market_config: pubkey(1),
            payer: pubkey(2),
        }
    }

    #[test]
    fn builder_places_signed_settlement_tail_after_prefix_instructions() {
        let prefix = system_program::ID;

        let instructions = SignedSettlementInstructionBuilder::new()
            .with_prefix_instruction(Instruction {
                program_id: prefix,
                accounts: Vec::new(),
                data: Vec::new(),
            })
            .build(accounts(), signed_fill_args());

        assert_eq!(instructions.len(), 4);
        assert_eq!(instructions[0].program_id, prefix);
        assert_eq!(
            instructions[1].program_id,
            solana_sdk_ids::ed25519_program::ID
        );
        assert_eq!(
            instructions[2].program_id,
            solana_sdk_ids::ed25519_program::ID
        );
        assert_eq!(instructions[3].program_id, spot_settlement::id());
    }

    #[test]
    fn builder_places_compute_budget_before_signed_settlement_tail() {
        let instructions = SignedSettlementInstructionBuilder::new()
            .with_compute_budget_preset(
                ComputeBudgetPreset::signed_settlement().with_priority_fee(2_000),
            )
            .build(accounts(), signed_fill_args());

        assert_eq!(instructions.len(), 5);
        assert_eq!(
            instructions[0].program_id,
            solana_compute_budget_interface::id()
        );
        assert_eq!(
            instructions[1].program_id,
            solana_compute_budget_interface::id()
        );
        assert_eq!(instructions[0].data, vec![2, 160, 134, 1, 0]);
        assert_eq!(instructions[1].data, vec![3, 208, 7, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            instructions[2].program_id,
            solana_sdk_ids::ed25519_program::ID
        );
        assert_eq!(
            instructions[3].program_id,
            solana_sdk_ids::ed25519_program::ID
        );
        assert_eq!(instructions[4].program_id, spot_settlement::id());
    }

    #[test]
    fn signed_settlement_preset_has_documented_margin() {
        let preset = ComputeBudgetPreset::signed_settlement();

        assert_eq!(preset.unit_limit, 100_000);
        assert_eq!(preset.micro_lamports, 0);
        assert!(preset.unit_limit > 79_984);
    }

    #[test]
    fn cancel_builder_places_compute_budget_before_cancel_instruction() {
        let order = signed_order(pubkey(1), pubkey(2), spot_settlement::SignedOrderSide::Bid);
        let instructions = CancelSignedOrderInstructionBuilder::new()
            .with_compute_budget_preset(
                ComputeBudgetPreset::cancel_signed_order().with_priority_fee(2_000),
            )
            .build(cancel_accounts(), [9; 32], order);

        assert_eq!(instructions.len(), 3);
        assert_eq!(
            instructions[0].program_id,
            solana_compute_budget_interface::id()
        );
        assert_eq!(
            instructions[1].program_id,
            solana_compute_budget_interface::id()
        );
        assert_eq!(instructions[0].data, vec![2, 64, 156, 0, 0]);
        assert_eq!(instructions[1].data, vec![3, 208, 7, 0, 0, 0, 0, 0, 0]);
        assert_eq!(instructions[2].program_id, spot_settlement::id());
    }

    #[test]
    fn cancel_signed_order_preset_has_documented_margin() {
        let preset = ComputeBudgetPreset::cancel_signed_order();

        assert_eq!(preset.unit_limit, 40_000);
        assert_eq!(preset.micro_lamports, 0);
        assert!(preset.unit_limit > 31_120);
    }
}
