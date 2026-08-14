#![forbid(unsafe_code)]

use anchor_lang::{prelude::Pubkey, solana_program::instruction::Instruction};
use settlement_client::{
    cancel_signed_order_flow_transaction_instructions,
    signed_settlement_flow_transaction_instructions, CancelSignedOrderFlowAccounts,
    ComputeBudgetPreset, SignedSettlementFlowAccounts,
};
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionPlan {
    pub instructions: Vec<Instruction>,
    pub required_signers: Vec<Pubkey>,
    pub fee_payer: Pubkey,
}

pub type TransactionSignature = [u8; 64];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubmittedTransaction {
    pub signature: TransactionSignature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmissionError {
    BlockhashUnavailable(String),
    BuildTransaction(String),
    MissingRequiredSigner(Pubkey),
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

pub trait BlockhashProvider {
    fn latest_blockhash(&mut self) -> Result<Hash, SubmissionError>;
}

pub trait TransactionSigner {
    fn sign(
        &self,
        plan: &InstructionPlan,
        recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, SubmissionError>;
}

pub trait TransactionSender {
    fn send(
        &mut self,
        transaction: VersionedTransaction,
    ) -> Result<SubmittedTransaction, SubmissionError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RpcSubmitter<B, S, T> {
    blockhash_provider: B,
    transaction_signer: S,
    transaction_sender: T,
}

impl<B, S, T> RpcSubmitter<B, S, T> {
    pub const fn new(blockhash_provider: B, transaction_signer: S, transaction_sender: T) -> Self {
        Self {
            blockhash_provider,
            transaction_signer,
            transaction_sender,
        }
    }
}

impl<B, S, T> SolanaSubmitter for RpcSubmitter<B, S, T>
where
    B: BlockhashProvider,
    S: TransactionSigner,
    T: TransactionSender,
{
    fn submit(&mut self, plan: InstructionPlan) -> Result<SubmittedTransaction, SubmissionError> {
        let blockhash = self.blockhash_provider.latest_blockhash()?;
        let transaction = self.transaction_signer.sign(&plan, blockhash)?;
        self.transaction_sender.send(transaction)
    }
}

#[derive(Debug)]
pub struct InMemoryTransactionSigner {
    signers: Vec<Keypair>,
}

impl InMemoryTransactionSigner {
    pub const fn new(signers: Vec<Keypair>) -> Self {
        Self { signers }
    }
}

impl TransactionSigner for InMemoryTransactionSigner {
    fn sign(
        &self,
        plan: &InstructionPlan,
        recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, SubmissionError> {
        let message = Message::new_with_blockhash(
            &plan.instructions,
            Some(&plan.fee_payer),
            &recent_blockhash,
        );
        let signers = required_keypairs(&self.signers, &plan.required_signers)?;

        VersionedTransaction::try_new(VersionedMessage::Legacy(message), &signers)
            .map_err(|error| SubmissionError::BuildTransaction(error.to_string()))
    }
}

fn required_keypairs<'a>(
    available: &'a [Keypair],
    required: &[Pubkey],
) -> Result<Vec<&'a Keypair>, SubmissionError> {
    let mut keypairs = Vec::with_capacity(required.len());

    for required_signer in required {
        let keypair = available
            .iter()
            .find(|keypair| keypair.pubkey() == *required_signer)
            .ok_or(SubmissionError::MissingRequiredSigner(*required_signer))?;
        keypairs.push(keypair);
    }

    Ok(keypairs)
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
            fee_payer: request.payer,
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
            fee_payer: request.payer,
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

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct FixedBlockhashProvider(Result<Hash, SubmissionError>);

    impl BlockhashProvider for FixedBlockhashProvider {
        fn latest_blockhash(&mut self) -> Result<Hash, SubmissionError> {
            self.0.clone()
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct RecordingTransactionSender {
        result: Result<SubmittedTransaction, SubmissionError>,
        sent_count: usize,
    }

    impl RecordingTransactionSender {
        const fn new(result: Result<SubmittedTransaction, SubmissionError>) -> Self {
            Self {
                result,
                sent_count: 0,
            }
        }
    }

    impl TransactionSender for RecordingTransactionSender {
        fn send(
            &mut self,
            transaction: VersionedTransaction,
        ) -> Result<SubmittedTransaction, SubmissionError> {
            assert!(!transaction.signatures.is_empty());
            self.sent_count += 1;
            self.result.clone()
        }
    }

    fn pubkey(byte: u8) -> Pubkey {
        Pubkey::new_from_array([byte; 32])
    }

    fn keypair(byte: u8) -> Keypair {
        Keypair::new_from_array([byte; 32])
    }

    fn blockhash(byte: u8) -> Hash {
        Hash::new_from_array([byte; 32])
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

    #[test]
    fn rpc_submitter_builds_signs_and_sends_signed_settlement_plan() {
        let settlement_authority = keypair(8);
        let payer = keypair(9);
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let plan = SettlementInstructionPlanner::default().plan_signed_settlement(
            SignedSettlementRequest {
                settlement_authority: settlement_authority.pubkey(),
                base_mint,
                quote_mint,
                buyer: pubkey(2),
                seller: pubkey(3),
                payer: payer.pubkey(),
                args: signed_fill_args(market_config),
            },
        );
        let mut submitter = RpcSubmitter::new(
            FixedBlockhashProvider(Ok(blockhash(1))),
            InMemoryTransactionSigner::new(vec![settlement_authority, payer]),
            RecordingTransactionSender::new(Ok(SubmittedTransaction {
                signature: [44; 64],
            })),
        );

        let submitted = submitter.submit(plan).unwrap();

        assert_eq!(submitted.signature, [44; 64]);
        assert_eq!(submitter.transaction_sender.sent_count, 1);
    }

    #[test]
    fn rpc_submitter_builds_signs_and_sends_cancel_plan() {
        let trader = keypair(2);
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let plan = SettlementInstructionPlanner::default().plan_cancel_signed_order(
            CancelSignedOrderRequest {
                trader: trader.pubkey(),
                base_mint,
                quote_mint,
                payer: trader.pubkey(),
                order_hash: [9; 32],
                order: signed_order(
                    market_config,
                    trader.pubkey(),
                    spot_settlement::SignedOrderSide::Bid,
                ),
            },
        );
        let mut submitter = RpcSubmitter::new(
            FixedBlockhashProvider(Ok(blockhash(1))),
            InMemoryTransactionSigner::new(vec![trader]),
            RecordingTransactionSender::new(Ok(SubmittedTransaction {
                signature: [45; 64],
            })),
        );

        let submitted = submitter.submit(plan).unwrap();

        assert_eq!(submitted.signature, [45; 64]);
        assert_eq!(submitter.transaction_sender.sent_count, 1);
    }

    #[test]
    fn rpc_submitter_rejects_missing_required_signer() {
        let settlement_authority = keypair(8);
        let payer = keypair(9);
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let plan = SettlementInstructionPlanner::default().plan_signed_settlement(
            SignedSettlementRequest {
                settlement_authority: settlement_authority.pubkey(),
                base_mint,
                quote_mint,
                buyer: pubkey(2),
                seller: pubkey(3),
                payer: payer.pubkey(),
                args: signed_fill_args(market_config),
            },
        );
        let mut submitter = RpcSubmitter::new(
            FixedBlockhashProvider(Ok(blockhash(1))),
            InMemoryTransactionSigner::new(vec![settlement_authority]),
            RecordingTransactionSender::new(Ok(SubmittedTransaction {
                signature: [46; 64],
            })),
        );

        let error = submitter.submit(plan).unwrap_err();

        assert_eq!(
            error,
            SubmissionError::MissingRequiredSigner(payer.pubkey())
        );
        assert_eq!(submitter.transaction_sender.sent_count, 0);
    }

    #[test]
    fn rpc_submitter_propagates_blockhash_failure() {
        let trader = keypair(2);
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let plan = SettlementInstructionPlanner::default().plan_cancel_signed_order(
            CancelSignedOrderRequest {
                trader: trader.pubkey(),
                base_mint,
                quote_mint,
                payer: trader.pubkey(),
                order_hash: [9; 32],
                order: signed_order(
                    market_config,
                    trader.pubkey(),
                    spot_settlement::SignedOrderSide::Bid,
                ),
            },
        );
        let mut submitter = RpcSubmitter::new(
            FixedBlockhashProvider(Err(SubmissionError::BlockhashUnavailable(
                "rpc unavailable".to_string(),
            ))),
            InMemoryTransactionSigner::new(vec![trader]),
            RecordingTransactionSender::new(Ok(SubmittedTransaction {
                signature: [47; 64],
            })),
        );

        let error = submitter.submit(plan).unwrap_err();

        assert_eq!(
            error,
            SubmissionError::BlockhashUnavailable("rpc unavailable".to_string())
        );
        assert_eq!(submitter.transaction_sender.sent_count, 0);
    }

    #[test]
    fn rpc_submitter_propagates_sender_failure() {
        let trader = keypair(2);
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let plan = SettlementInstructionPlanner::default().plan_cancel_signed_order(
            CancelSignedOrderRequest {
                trader: trader.pubkey(),
                base_mint,
                quote_mint,
                payer: trader.pubkey(),
                order_hash: [9; 32],
                order: signed_order(
                    market_config,
                    trader.pubkey(),
                    spot_settlement::SignedOrderSide::Bid,
                ),
            },
        );
        let mut submitter = RpcSubmitter::new(
            FixedBlockhashProvider(Ok(blockhash(1))),
            InMemoryTransactionSigner::new(vec![trader]),
            RecordingTransactionSender::new(Err(SubmissionError::Rejected(
                "transaction rejected".to_string(),
            ))),
        );

        let error = submitter.submit(plan).unwrap_err();

        assert_eq!(
            error,
            SubmissionError::Rejected("transaction rejected".to_string())
        );
        assert_eq!(submitter.transaction_sender.sent_count, 1);
    }
}
