#![forbid(unsafe_code)]

pub mod application_bridge;

use anchor_lang::{prelude::Pubkey, solana_program::instruction::Instruction};
use async_trait::async_trait;
use settlement_client::{
    cancel_signed_order_flow_transaction_instructions,
    signed_settlement_flow_transaction_instructions, CancelSignedOrderFlowAccounts,
    ComputeBudgetPreset, SignedSettlementFlowAccounts,
};
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
#[cfg(feature = "rpc")]
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
#[cfg(feature = "rpc")]
use solana_signature::Signature;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;

pub use application_bridge::{
    ApplicationSettlementBridge, ApplicationSettlementBridgeError, SettlementSignedOrder,
    SettlementSignedOrderSource,
};

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
    BlockhashExpired(String),
    BlockhashUnavailable(String),
    BuildTransaction(String),
    MissingRequiredSigner(Pubkey),
    Rejected(String),
    Uncertain(String),
}

impl SubmissionError {
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::BlockhashExpired(_) | Self::BlockhashUnavailable(_) | Self::Uncertain(_)
        )
    }
}

#[async_trait]
pub trait SolanaSubmitter {
    async fn submit(
        &mut self,
        plan: InstructionPlan,
    ) -> Result<SubmittedTransaction, SubmissionError>;
}

const DEFAULT_MAX_SUBMIT_ATTEMPTS: usize = 3;

fn retryable_submission_error(error: &SubmissionError) -> bool {
    error.is_retryable()
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

#[async_trait]
impl SolanaSubmitter for RecordingSubmitter {
    async fn submit(
        &mut self,
        plan: InstructionPlan,
    ) -> Result<SubmittedTransaction, SubmissionError> {
        self.submitted_plans.push(plan);
        Ok(SubmittedTransaction {
            signature: self.signature,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationStatus {
    Confirmed,
    Failed(String),
    Uncertain(String),
}

#[async_trait]
pub trait TransactionConfirmer {
    async fn confirm(
        &mut self,
        signature: TransactionSignature,
    ) -> Result<ConfirmationStatus, SubmissionError>;
}

#[async_trait]
pub trait ConfirmationPoller {
    async fn poll(
        &mut self,
        signature: TransactionSignature,
    ) -> Result<ConfirmationStatus, SubmissionError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollingPolicy {
    pub max_polls: usize,
}

impl Default for PollingPolicy {
    fn default() -> Self {
        Self { max_polls: 20 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollingConfirmer<P> {
    poller: P,
    policy: PollingPolicy,
}

impl<P> PollingConfirmer<P> {
    pub const fn new(poller: P, policy: PollingPolicy) -> Self {
        Self { poller, policy }
    }
}

#[async_trait]
impl<P> TransactionConfirmer for PollingConfirmer<P>
where
    P: ConfirmationPoller + Send,
{
    async fn confirm(
        &mut self,
        signature: TransactionSignature,
    ) -> Result<ConfirmationStatus, SubmissionError> {
        for _ in 0..self.policy.max_polls {
            match self.poller.poll(signature).await? {
                ConfirmationStatus::Uncertain(_) => {}
                status => return Ok(status),
            }
        }

        Ok(ConfirmationStatus::Uncertain(
            "confirmation polling exhausted".to_string(),
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmingSubmitter<S, C> {
    submitter: S,
    confirmer: C,
}

impl<S, C> ConfirmingSubmitter<S, C> {
    pub const fn new(submitter: S, confirmer: C) -> Self {
        Self {
            submitter,
            confirmer,
        }
    }
}

#[async_trait]
impl<S, C> SolanaSubmitter for ConfirmingSubmitter<S, C>
where
    S: SolanaSubmitter + Send,
    C: TransactionConfirmer + Send,
{
    async fn submit(
        &mut self,
        plan: InstructionPlan,
    ) -> Result<SubmittedTransaction, SubmissionError> {
        let submitted = self.submitter.submit(plan).await?;

        match self.confirmer.confirm(submitted.signature).await? {
            ConfirmationStatus::Confirmed => Ok(submitted),
            ConfirmationStatus::Failed(error) => Err(SubmissionError::Rejected(error)),
            ConfirmationStatus::Uncertain(error) => Err(SubmissionError::Uncertain(error)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: usize,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: DEFAULT_MAX_SUBMIT_ATTEMPTS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryingSubmitter<S> {
    submitter: S,
    policy: RetryPolicy,
}

impl<S> RetryingSubmitter<S> {
    pub const fn new(submitter: S, policy: RetryPolicy) -> Self {
        Self { submitter, policy }
    }
}

#[async_trait]
impl<S> SolanaSubmitter for RetryingSubmitter<S>
where
    S: SolanaSubmitter + Send,
{
    async fn submit(
        &mut self,
        plan: InstructionPlan,
    ) -> Result<SubmittedTransaction, SubmissionError> {
        let max_attempts = self.policy.max_attempts.max(1);
        let mut last_error = None;

        for _ in 0..max_attempts {
            match self.submitter.submit(plan.clone()).await {
                Ok(submitted) => return Ok(submitted),
                Err(error) if retryable_submission_error(&error) => {
                    last_error = Some(error);
                }
                Err(error) => return Err(error),
            }
        }

        Err(last_error
            .unwrap_or_else(|| SubmissionError::Uncertain("retry attempts exhausted".to_string())))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadLetter {
    pub plan: InstructionPlan,
    pub error: SubmissionError,
}

#[async_trait]
pub trait DeadLetterSink {
    async fn record(&mut self, dead_letter: DeadLetter) -> Result<(), SubmissionError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingDeadLetterSink {
    pub dead_letters: Vec<DeadLetter>,
}

impl RecordingDeadLetterSink {
    pub const fn new() -> Self {
        Self {
            dead_letters: Vec::new(),
        }
    }
}

#[async_trait]
impl DeadLetterSink for RecordingDeadLetterSink {
    async fn record(&mut self, dead_letter: DeadLetter) -> Result<(), SubmissionError> {
        self.dead_letters.push(dead_letter);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadLetteringSubmitter<S, D> {
    submitter: S,
    dead_letters: D,
}

impl<S, D> DeadLetteringSubmitter<S, D> {
    pub const fn new(submitter: S, dead_letters: D) -> Self {
        Self {
            submitter,
            dead_letters,
        }
    }
}

#[async_trait]
impl<S, D> SolanaSubmitter for DeadLetteringSubmitter<S, D>
where
    S: SolanaSubmitter + Send,
    D: DeadLetterSink + Send,
{
    async fn submit(
        &mut self,
        plan: InstructionPlan,
    ) -> Result<SubmittedTransaction, SubmissionError> {
        match self.submitter.submit(plan.clone()).await {
            Ok(submitted) => Ok(submitted),
            Err(error) => {
                self.dead_letters
                    .record(DeadLetter {
                        plan,
                        error: error.clone(),
                    })
                    .await?;
                Err(error)
            }
        }
    }
}

#[async_trait]
pub trait BlockhashProvider {
    async fn latest_blockhash(&mut self) -> Result<Hash, SubmissionError>;
}

pub trait TransactionSigner {
    fn sign(
        &self,
        plan: &InstructionPlan,
        recent_blockhash: Hash,
    ) -> Result<VersionedTransaction, SubmissionError>;
}

#[async_trait]
pub trait TransactionSender {
    async fn send(
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

#[async_trait]
impl<B, S, T> SolanaSubmitter for RpcSubmitter<B, S, T>
where
    B: BlockhashProvider + Send,
    S: TransactionSigner + Send + Sync,
    T: TransactionSender + Send,
{
    async fn submit(
        &mut self,
        plan: InstructionPlan,
    ) -> Result<SubmittedTransaction, SubmissionError> {
        let blockhash = self.blockhash_provider.latest_blockhash().await?;
        let transaction = self.transaction_signer.sign(&plan, blockhash)?;
        self.transaction_sender.send(transaction).await
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

#[cfg(feature = "rpc")]
pub struct RpcBlockhashProvider {
    client: RpcClient,
}

#[cfg(feature = "rpc")]
impl RpcBlockhashProvider {
    pub fn new(url: String) -> Self {
        Self {
            client: RpcClient::new(url),
        }
    }

    pub const fn from_client(client: RpcClient) -> Self {
        Self { client }
    }
}

#[cfg(feature = "rpc")]
#[async_trait]
impl BlockhashProvider for RpcBlockhashProvider {
    async fn latest_blockhash(&mut self) -> Result<Hash, SubmissionError> {
        self.client
            .get_latest_blockhash()
            .await
            .map_err(|error| SubmissionError::BlockhashUnavailable(error.to_string()))
    }
}

#[cfg(feature = "rpc")]
pub struct RpcTransactionSender {
    client: RpcClient,
}

#[cfg(feature = "rpc")]
impl RpcTransactionSender {
    pub fn new(url: String) -> Self {
        Self {
            client: RpcClient::new(url),
        }
    }

    pub const fn from_client(client: RpcClient) -> Self {
        Self { client }
    }
}

#[cfg(feature = "rpc")]
pub struct RpcConfirmationPoller {
    client: RpcClient,
}

#[cfg(feature = "rpc")]
impl RpcConfirmationPoller {
    pub fn new(url: String) -> Self {
        Self {
            client: RpcClient::new(url),
        }
    }

    pub const fn from_client(client: RpcClient) -> Self {
        Self { client }
    }
}

#[cfg(feature = "rpc")]
#[async_trait]
impl ConfirmationPoller for RpcConfirmationPoller {
    async fn poll(
        &mut self,
        signature: TransactionSignature,
    ) -> Result<ConfirmationStatus, SubmissionError> {
        let signature = Signature::from(signature);
        match self
            .client
            .get_signature_status(&signature)
            .await
            .map_err(|error| SubmissionError::Uncertain(error.to_string()))?
        {
            Some(Ok(())) => Ok(ConfirmationStatus::Confirmed),
            Some(Err(error)) => Ok(ConfirmationStatus::Failed(error.to_string())),
            None => Ok(ConfirmationStatus::Uncertain(
                "signature status not found".to_string(),
            )),
        }
    }
}

#[cfg(feature = "rpc")]
#[async_trait]
impl TransactionSender for RpcTransactionSender {
    async fn send(
        &mut self,
        transaction: VersionedTransaction,
    ) -> Result<SubmittedTransaction, SubmissionError> {
        let signature = self
            .client
            .send_transaction(&transaction)
            .await
            .map_err(|error| SubmissionError::Rejected(error.to_string()))?;

        Ok(SubmittedTransaction {
            signature: *signature.as_array(),
        })
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementWorkerReport {
    pub submitted: Vec<SubmittedTransaction>,
    pub failed: Vec<SettlementSubmissionFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementSubmissionFailure {
    pub request: SignedSettlementRequest,
    pub error: SubmissionError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementRequestWorker<S> {
    planner: SettlementInstructionPlanner,
    submitter: S,
}

impl<S> SettlementRequestWorker<S> {
    pub fn new(submitter: S) -> Self {
        Self {
            planner: SettlementInstructionPlanner::default(),
            submitter,
        }
    }

    pub const fn with_planner(planner: SettlementInstructionPlanner, submitter: S) -> Self {
        Self { planner, submitter }
    }

    pub async fn submit_requests(
        &mut self,
        requests: impl IntoIterator<Item = SignedSettlementRequest>,
    ) -> SettlementWorkerReport
    where
        S: SolanaSubmitter + Send,
    {
        let mut submitted = Vec::new();
        let mut failed = Vec::new();

        for request in requests {
            let plan = self.planner.plan_signed_settlement(request);
            match self.submitter.submit(plan).await {
                Ok(transaction) => submitted.push(transaction),
                Err(error) => failed.push(SettlementSubmissionFailure { request, error }),
            }
        }

        SettlementWorkerReport { submitted, failed }
    }

    pub const fn submitter(&self) -> &S {
        &self.submitter
    }
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

    #[async_trait]
    impl BlockhashProvider for FixedBlockhashProvider {
        async fn latest_blockhash(&mut self) -> Result<Hash, SubmissionError> {
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

    #[async_trait]
    impl TransactionSender for RecordingTransactionSender {
        async fn send(
            &mut self,
            transaction: VersionedTransaction,
        ) -> Result<SubmittedTransaction, SubmissionError> {
            assert!(!transaction.signatures.is_empty());
            self.sent_count += 1;
            self.result.clone()
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct FixedConfirmer {
        result: Result<ConfirmationStatus, SubmissionError>,
        confirmed_signatures: Vec<TransactionSignature>,
    }

    impl FixedConfirmer {
        const fn new(result: Result<ConfirmationStatus, SubmissionError>) -> Self {
            Self {
                result,
                confirmed_signatures: Vec::new(),
            }
        }
    }

    #[async_trait]
    impl TransactionConfirmer for FixedConfirmer {
        async fn confirm(
            &mut self,
            signature: TransactionSignature,
        ) -> Result<ConfirmationStatus, SubmissionError> {
            self.confirmed_signatures.push(signature);
            self.result.clone()
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct FailingSubmitter(SubmissionError);

    #[async_trait]
    impl SolanaSubmitter for FailingSubmitter {
        async fn submit(
            &mut self,
            _plan: InstructionPlan,
        ) -> Result<SubmittedTransaction, SubmissionError> {
            Err(self.0.clone())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SequenceSubmitter {
        results: Vec<Result<SubmittedTransaction, SubmissionError>>,
        submitted_plans: Vec<InstructionPlan>,
    }

    impl SequenceSubmitter {
        const fn new(results: Vec<Result<SubmittedTransaction, SubmissionError>>) -> Self {
            Self {
                results,
                submitted_plans: Vec::new(),
            }
        }
    }

    #[async_trait]
    impl SolanaSubmitter for SequenceSubmitter {
        async fn submit(
            &mut self,
            plan: InstructionPlan,
        ) -> Result<SubmittedTransaction, SubmissionError> {
            self.submitted_plans.push(plan);
            if self.results.is_empty() {
                return Err(SubmissionError::Uncertain("no sequence result".to_string()));
            }
            self.results.remove(0)
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SequencePoller {
        results: Vec<Result<ConfirmationStatus, SubmissionError>>,
        polled_signatures: Vec<TransactionSignature>,
    }

    impl SequencePoller {
        const fn new(results: Vec<Result<ConfirmationStatus, SubmissionError>>) -> Self {
            Self {
                results,
                polled_signatures: Vec::new(),
            }
        }
    }

    #[async_trait]
    impl ConfirmationPoller for SequencePoller {
        async fn poll(
            &mut self,
            signature: TransactionSignature,
        ) -> Result<ConfirmationStatus, SubmissionError> {
            self.polled_signatures.push(signature);
            if self.results.is_empty() {
                return Err(SubmissionError::Uncertain("no poll result".to_string()));
            }
            self.results.remove(0)
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

    #[cfg(feature = "rpc")]
    fn rpc_url() -> String {
        std::env::var("SOLANA_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8899".to_string())
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

    fn cancel_plan() -> InstructionPlan {
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;

        SettlementInstructionPlanner::default().plan_cancel_signed_order(CancelSignedOrderRequest {
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
        })
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

    #[tokio::test]
    async fn submitter_accepts_signed_settlement_plan() {
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

        let submitted = submitter.submit(plan.clone()).await.unwrap();

        assert_eq!(submitted.signature, [42; 64]);
        assert_eq!(submitter.submitted_plans, vec![plan]);
    }

    #[tokio::test]
    async fn settlement_worker_submits_signed_settlement_requests() {
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let mut worker = SettlementRequestWorker::new(RecordingSubmitter::new([42; 64]));

        let report = worker
            .submit_requests([
                SignedSettlementRequest {
                    settlement_authority: pubkey(8),
                    base_mint,
                    quote_mint,
                    buyer: pubkey(2),
                    seller: pubkey(3),
                    payer: pubkey(9),
                    args: signed_fill_args(market_config),
                },
                SignedSettlementRequest {
                    settlement_authority: pubkey(8),
                    base_mint,
                    quote_mint,
                    buyer: pubkey(2),
                    seller: pubkey(3),
                    payer: pubkey(9),
                    args: signed_fill_args(market_config),
                },
            ])
            .await;

        assert_eq!(
            report,
            SettlementWorkerReport {
                submitted: vec![
                    SubmittedTransaction {
                        signature: [42; 64]
                    },
                    SubmittedTransaction {
                        signature: [42; 64]
                    }
                ],
                failed: Vec::new(),
            }
        );
        assert_eq!(worker.submitter().submitted_plans.len(), 2);
    }

    #[tokio::test]
    async fn settlement_worker_reports_failures_and_continues() {
        let base_mint = pubkey(11);
        let quote_mint = pubkey(12);
        let market_config = settlement_client::market_config_pda(base_mint, quote_mint).0;
        let request = SignedSettlementRequest {
            settlement_authority: pubkey(8),
            base_mint,
            quote_mint,
            buyer: pubkey(2),
            seller: pubkey(3),
            payer: pubkey(9),
            args: signed_fill_args(market_config),
        };
        let mut worker = SettlementRequestWorker::new(SequenceSubmitter::new(vec![
            Err(SubmissionError::Rejected("first failed".to_string())),
            Ok(SubmittedTransaction {
                signature: [42; 64],
            }),
        ]));

        let report = worker.submit_requests([request, request]).await;

        assert_eq!(
            report,
            SettlementWorkerReport {
                submitted: vec![SubmittedTransaction {
                    signature: [42; 64]
                }],
                failed: vec![SettlementSubmissionFailure {
                    request,
                    error: SubmissionError::Rejected("first failed".to_string()),
                }],
            }
        );
        assert_eq!(worker.submitter().submitted_plans.len(), 2);
    }

    #[tokio::test]
    async fn submitter_accepts_cancel_plan() {
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

        let submitted = submitter.submit(plan.clone()).await.unwrap();

        assert_eq!(submitted.signature, [43; 64]);
        assert_eq!(submitter.submitted_plans, vec![plan]);
    }

    #[tokio::test]
    async fn confirming_submitter_returns_submitted_transaction_when_confirmed() {
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
        let mut submitter = ConfirmingSubmitter::new(
            RecordingSubmitter::new([50; 64]),
            FixedConfirmer::new(Ok(ConfirmationStatus::Confirmed)),
        );

        let submitted = submitter.submit(plan).await.unwrap();

        assert_eq!(submitted.signature, [50; 64]);
        assert_eq!(submitter.confirmer.confirmed_signatures, vec![[50; 64]]);
    }

    #[tokio::test]
    async fn confirming_submitter_maps_failed_confirmation_to_rejected() {
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
        let mut submitter = ConfirmingSubmitter::new(
            RecordingSubmitter::new([51; 64]),
            FixedConfirmer::new(Ok(ConfirmationStatus::Failed(
                "execution failed".to_string(),
            ))),
        );

        let error = submitter.submit(plan).await.unwrap_err();

        assert_eq!(
            error,
            SubmissionError::Rejected("execution failed".to_string())
        );
        assert_eq!(submitter.confirmer.confirmed_signatures, vec![[51; 64]]);
    }

    #[tokio::test]
    async fn confirming_submitter_surfaces_uncertain_confirmation() {
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
        let mut submitter = ConfirmingSubmitter::new(
            RecordingSubmitter::new([52; 64]),
            FixedConfirmer::new(Ok(ConfirmationStatus::Uncertain(
                "status not found".to_string(),
            ))),
        );

        let error = submitter.submit(plan).await.unwrap_err();

        assert_eq!(
            error,
            SubmissionError::Uncertain("status not found".to_string())
        );
        assert_eq!(submitter.confirmer.confirmed_signatures, vec![[52; 64]]);
    }

    #[tokio::test]
    async fn confirming_submitter_skips_confirmation_when_submit_fails() {
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
        let mut submitter = ConfirmingSubmitter::new(
            FailingSubmitter(SubmissionError::Rejected("rpc rejected".to_string())),
            FixedConfirmer::new(Ok(ConfirmationStatus::Confirmed)),
        );

        let error = submitter.submit(plan).await.unwrap_err();

        assert_eq!(error, SubmissionError::Rejected("rpc rejected".to_string()));
        assert!(submitter.confirmer.confirmed_signatures.is_empty());
    }

    #[tokio::test]
    async fn polling_confirmer_polls_until_confirmed() {
        let mut confirmer = PollingConfirmer::new(
            SequencePoller::new(vec![
                Ok(ConfirmationStatus::Uncertain("not found".to_string())),
                Ok(ConfirmationStatus::Confirmed),
            ]),
            PollingPolicy { max_polls: 3 },
        );

        let status = confirmer.confirm([60; 64]).await.unwrap();

        assert_eq!(status, ConfirmationStatus::Confirmed);
        assert_eq!(confirmer.poller.polled_signatures, vec![[60; 64], [60; 64]]);
    }

    #[tokio::test]
    async fn polling_confirmer_stops_on_failed_status() {
        let mut confirmer = PollingConfirmer::new(
            SequencePoller::new(vec![Ok(ConfirmationStatus::Failed(
                "execution failed".to_string(),
            ))]),
            PollingPolicy { max_polls: 3 },
        );

        let status = confirmer.confirm([61; 64]).await.unwrap();

        assert_eq!(
            status,
            ConfirmationStatus::Failed("execution failed".to_string())
        );
        assert_eq!(confirmer.poller.polled_signatures, vec![[61; 64]]);
    }

    #[tokio::test]
    async fn polling_confirmer_reports_uncertain_after_exhaustion() {
        let mut confirmer = PollingConfirmer::new(
            SequencePoller::new(vec![
                Ok(ConfirmationStatus::Uncertain("not found".to_string())),
                Ok(ConfirmationStatus::Uncertain("not found".to_string())),
            ]),
            PollingPolicy { max_polls: 2 },
        );

        let status = confirmer.confirm([62; 64]).await.unwrap();

        assert_eq!(
            status,
            ConfirmationStatus::Uncertain("confirmation polling exhausted".to_string())
        );
        assert_eq!(confirmer.poller.polled_signatures, vec![[62; 64], [62; 64]]);
    }

    #[tokio::test]
    async fn retrying_submitter_retries_blockhash_expiry_with_fresh_submission() {
        let plan = cancel_plan();
        let mut submitter = RetryingSubmitter::new(
            SequenceSubmitter::new(vec![
                Err(SubmissionError::BlockhashExpired("expired".to_string())),
                Ok(SubmittedTransaction {
                    signature: [63; 64],
                }),
            ]),
            RetryPolicy { max_attempts: 2 },
        );

        let submitted = submitter.submit(plan.clone()).await.unwrap();

        assert_eq!(submitted.signature, [63; 64]);
        assert_eq!(
            submitter.submitter.submitted_plans,
            vec![plan.clone(), plan]
        );
    }

    #[tokio::test]
    async fn retrying_submitter_does_not_retry_rejected_submission() {
        let plan = cancel_plan();
        let mut submitter = RetryingSubmitter::new(
            SequenceSubmitter::new(vec![Err(SubmissionError::Rejected(
                "signature failure".to_string(),
            ))]),
            RetryPolicy { max_attempts: 3 },
        );

        let error = submitter.submit(plan.clone()).await.unwrap_err();

        assert_eq!(
            error,
            SubmissionError::Rejected("signature failure".to_string())
        );
        assert_eq!(submitter.submitter.submitted_plans, vec![plan]);
    }

    #[tokio::test]
    async fn retrying_submitter_returns_last_retryable_error_after_exhaustion() {
        let plan = cancel_plan();
        let mut submitter = RetryingSubmitter::new(
            SequenceSubmitter::new(vec![
                Err(SubmissionError::BlockhashUnavailable(
                    "rpc unavailable".to_string(),
                )),
                Err(SubmissionError::Uncertain("timeout".to_string())),
            ]),
            RetryPolicy { max_attempts: 2 },
        );

        let error = submitter.submit(plan.clone()).await.unwrap_err();

        assert_eq!(error, SubmissionError::Uncertain("timeout".to_string()));
        assert_eq!(
            submitter.submitter.submitted_plans,
            vec![plan.clone(), plan]
        );
    }

    #[tokio::test]
    async fn dead_lettering_submitter_records_failed_submission() {
        let plan = cancel_plan();
        let mut submitter = DeadLetteringSubmitter::new(
            FailingSubmitter(SubmissionError::Uncertain("timeout".to_string())),
            RecordingDeadLetterSink::new(),
        );

        let error = submitter.submit(plan.clone()).await.unwrap_err();

        assert_eq!(error, SubmissionError::Uncertain("timeout".to_string()));
        assert_eq!(
            submitter.dead_letters.dead_letters,
            vec![DeadLetter {
                plan,
                error: SubmissionError::Uncertain("timeout".to_string()),
            }]
        );
    }

    #[tokio::test]
    async fn rpc_submitter_builds_signs_and_sends_signed_settlement_plan() {
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

        let submitted = submitter.submit(plan).await.unwrap();

        assert_eq!(submitted.signature, [44; 64]);
        assert_eq!(submitter.transaction_sender.sent_count, 1);
    }

    #[tokio::test]
    async fn rpc_submitter_builds_signs_and_sends_cancel_plan() {
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

        let submitted = submitter.submit(plan).await.unwrap();

        assert_eq!(submitted.signature, [45; 64]);
        assert_eq!(submitter.transaction_sender.sent_count, 1);
    }

    #[tokio::test]
    async fn rpc_submitter_rejects_missing_required_signer() {
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

        let error = submitter.submit(plan).await.unwrap_err();

        assert_eq!(
            error,
            SubmissionError::MissingRequiredSigner(payer.pubkey())
        );
        assert_eq!(submitter.transaction_sender.sent_count, 0);
    }

    #[tokio::test]
    async fn rpc_submitter_propagates_blockhash_failure() {
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

        let error = submitter.submit(plan).await.unwrap_err();

        assert_eq!(
            error,
            SubmissionError::BlockhashUnavailable("rpc unavailable".to_string())
        );
        assert_eq!(submitter.transaction_sender.sent_count, 0);
    }

    #[tokio::test]
    async fn rpc_submitter_propagates_sender_failure() {
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

        let error = submitter.submit(plan).await.unwrap_err();

        assert_eq!(
            error,
            SubmissionError::Rejected("transaction rejected".to_string())
        );
        assert_eq!(submitter.transaction_sender.sent_count, 1);
    }

    #[cfg(feature = "rpc")]
    #[test]
    fn rpc_adapters_can_be_constructed() {
        let url = rpc_url();

        let _blockhash_provider = RpcBlockhashProvider::new(url.clone());
        let _transaction_sender = RpcTransactionSender::new(url);
    }

    #[cfg(feature = "rpc")]
    #[tokio::test]
    #[ignore = "requires SOLANA_RPC_URL or local validator at http://127.0.0.1:8899"]
    async fn rpc_blockhash_provider_fetches_live_blockhash() {
        let mut blockhash_provider = RpcBlockhashProvider::new(rpc_url());

        let blockhash = blockhash_provider.latest_blockhash().await.unwrap();

        assert_ne!(blockhash, Hash::default());
    }
}
