#![forbid(unsafe_code)]

use anchor_lang::{prelude::Pubkey, solana_program::instruction::Instruction};
use settlement_client::{
    cancel_signed_order_flow_transaction_instructions,
    signed_settlement_flow_transaction_instructions, CancelSignedOrderFlowAccounts,
    ComputeBudgetPreset, SignedSettlementFlowAccounts,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionPlan {
    pub instructions: Vec<Instruction>,
    pub required_signers: Vec<Pubkey>,
}

pub type TransactionSignature = [u8; 64];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmittedTransaction {
    pub signature: TransactionSignature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmissionError {
    Rejected(String),
}

pub trait SolanaSubmitter {
    fn submit(&mut self, plan: InstructionPlan) -> Result<SubmittedTransaction, SubmissionError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingSubmitter {
    signature: TransactionSignature,
    pub submitted_plans: Vec<InstructionPlan>,
}

impl RecordingSubmitter {
    pub const fn new(signature: TransactionSignature) -> Self {
        Self {
            signature,
            submitted_plans: Vec::new(),
        }
    }
}

impl SolanaSubmitter for RecordingSubmitter {
    fn submit(&mut self, plan: InstructionPlan) -> Result<SubmittedTransaction, SubmissionError> {
        self.submitted_plans.push(plan);
        Ok(SubmittedTransaction {
            signature: self.signature,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementInstructionPlanner {
    signed_settlement_compute_budget: ComputeBudgetPreset,
    cancel_compute_budget: ComputeBudgetPreset,
}

impl Default for SettlementInstructionPlanner {
    fn default() -> Self {
        Self {
            signed_settlement_compute_budget: ComputeBudgetPreset::signed_settlement(),
            cancel_compute_budget: ComputeBudgetPreset::cancel_signed_order(),
        }
    }
}

impl SettlementInstructionPlanner {
    pub const fn new(
        signed_settlement_compute_budget: ComputeBudgetPreset,
        cancel_compute_budget: ComputeBudgetPreset,
    ) -> Self {
        Self {
            signed_settlement_compute_budget,
            cancel_compute_budget,
        }
    }

    pub fn plan_signed_settlement(&self, request: SignedSettlementRequest) -> InstructionPlan {
        let instructions = signed_settlement_flow_transaction_instructions(
            SignedSettlementFlowAccounts {
                settlement_authority: request.settlement_authority,
                base_mint: request.base_mint,
                quote_mint: request.quote_mint,
                buyer: request.buyer,
                seller: request.seller,
                payer: request.payer,
            },
            request.args,
            self.signed_settlement_compute_budget,
        );

        InstructionPlan {
            instructions,
            required_signers: unique_signers([request.settlement_authority, request.payer]),
        }
    }

    pub fn plan_cancel_signed_order(&self, request: CancelSignedOrderRequest) -> InstructionPlan {
        let instructions = cancel_signed_order_flow_transaction_instructions(
            CancelSignedOrderFlowAccounts {
                trader: request.trader,
                base_mint: request.base_mint,
                quote_mint: request.quote_mint,
                payer: request.payer,
            },
            request.order_hash,
            request.order,
            self.cancel_compute_budget,
        );

        InstructionPlan {
            instructions,
            required_signers: unique_signers([request.trader, request.payer]),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedSettlementRequest {
    pub settlement_authority: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub buyer: Pubkey,
    pub seller: Pubkey,
    pub payer: Pubkey,
    pub args: spot_settlement::SignedFillArgs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelSignedOrderRequest {
    pub trader: Pubkey,
    pub base_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub payer: Pubkey,
    pub order_hash: [u8; 32],
    pub order: spot_settlement::SignedOrderPayload,
}

fn unique_signers<const N: usize>(signers: [Pubkey; N]) -> Vec<Pubkey> {
    let mut unique = Vec::with_capacity(N);

    for signer in signers {
        if !unique.contains(&signer) {
            unique.push(signer);
        }
    }

    unique
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

    fn signed_fill_args(market_config: Pubkey) -> spot_settlement::SignedFillArgs {
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

    #[test]
    fn signed_settlement_plan_contains_ordered_instructions_and_signers() {
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let planner = SettlementInstructionPlanner::new(
            ComputeBudgetPreset::signed_settlement().with_priority_fee(2_000),
            ComputeBudgetPreset::cancel_signed_order(),
        );

        let plan = planner.plan_signed_settlement(SignedSettlementRequest {
            settlement_authority: pubkey(8),
            base_mint,
            quote_mint,
            buyer: pubkey(2),
            seller: pubkey(3),
            payer: pubkey(9),
            args: signed_fill_args(market_config),
        });

        assert_eq!(plan.required_signers, vec![pubkey(8), pubkey(9)]);
        assert_eq!(plan.instructions.len(), 5);
        assert_eq!(
            plan.instructions[0].program_id,
            solana_compute_budget_interface::id()
        );
        assert_eq!(
            plan.instructions[1].program_id,
            solana_compute_budget_interface::id()
        );
        assert_eq!(
            plan.instructions[2].program_id,
            solana_sdk_ids::ed25519_program::ID
        );
        assert_eq!(
            plan.instructions[3].program_id,
            solana_sdk_ids::ed25519_program::ID
        );
        assert_eq!(plan.instructions[4].program_id, spot_settlement::id());
    }

    #[test]
    fn cancel_plan_deduplicates_signers_when_trader_pays() {
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let planner = SettlementInstructionPlanner::default();

        let plan = planner.plan_cancel_signed_order(CancelSignedOrderRequest {
            trader: pubkey(2),
            base_mint,
            quote_mint,
            payer: pubkey(2),
            order_hash: [9; 32],
            order: signed_order(
                market_config,
                pubkey(2),
                spot_settlement::SignedOrderSide::Bid,
            ),
        });

        assert_eq!(plan.required_signers, vec![pubkey(2)]);
        assert_eq!(plan.instructions.len(), 3);
        assert_eq!(
            plan.instructions[0].program_id,
            solana_compute_budget_interface::id()
        );
        assert_eq!(
            plan.instructions[1].program_id,
            solana_compute_budget_interface::id()
        );
        assert_eq!(plan.instructions[2].program_id, spot_settlement::id());
    }

    #[test]
    fn submitter_accepts_signed_settlement_plan() {
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let plan = SettlementInstructionPlanner::default().plan_signed_settlement(
            SignedSettlementRequest {
                settlement_authority: pubkey(8),
                base_mint,
                quote_mint,
                buyer: pubkey(2),
                seller: pubkey(3),
                payer: pubkey(9),
                args: signed_fill_args(market_config),
            },
        );
        let mut submitter = RecordingSubmitter::new([42; 64]);

        let submitted = submitter.submit(plan.clone()).unwrap();

        assert_eq!(submitted.signature, [42; 64]);
        assert_eq!(submitter.submitted_plans, vec![plan]);
    }

    #[test]
    fn submitter_accepts_cancel_plan() {
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let plan = SettlementInstructionPlanner::default().plan_cancel_signed_order(
            CancelSignedOrderRequest {
                trader: pubkey(2),
                base_mint,
                quote_mint,
                payer: pubkey(2),
                order_hash: [9; 32],
                order: signed_order(
                    market_config,
                    pubkey(2),
                    spot_settlement::SignedOrderSide::Bid,
                ),
            },
        );
        let mut submitter = RecordingSubmitter::new([43; 64]);

        let submitted = submitter.submit(plan.clone()).unwrap();

        assert_eq!(submitted.signature, [43; 64]);
        assert_eq!(submitter.submitted_plans, vec![plan]);
    }
}
