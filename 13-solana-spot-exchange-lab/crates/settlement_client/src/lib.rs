#![forbid(unsafe_code)]

use anchor_lang::{
    prelude::Pubkey,
    solana_program::{instruction::Instruction, system_program},
    InstructionData, ToAccountMetas,
};
use solana_ed25519_program::new_ed25519_instruction_with_signature;

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
}
