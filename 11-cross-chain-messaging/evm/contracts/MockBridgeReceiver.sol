// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

contract MockBridgeReceiver {
    error UnauthorizedBridge();
    error InvalidDestinationChain();
    error InvalidMint();
    error InvalidRecipient();
    error InvalidAmount();
    error BridgeLimitExceeded();
    error MessageAlreadyConsumed();

    struct CrossChainMintMessage {
        uint16 sourceChainId;
        uint16 destinationChainId;
        uint64 nonce;
        address mint;
        address recipient;
        uint256 amount;
    }

    address public immutable trustedBridge;
    address public immutable registeredMint;
    uint16 public immutable localChainId;
    uint256 public immutable perMessageLimit;

    mapping(bytes32 messageId => bool consumed) public consumedMessages;

    event CrossChainMintConsumed(
        bytes32 indexed messageId,
        uint16 indexed sourceChainId,
        uint64 indexed nonce,
        address mint,
        address recipient,
        uint256 amount
    );

    constructor(
        address trustedBridge_,
        address registeredMint_,
        uint16 localChainId_,
        uint256 perMessageLimit_
    ) {
        if (trustedBridge_ == address(0) || registeredMint_ == address(0)) {
            revert InvalidRecipient();
        }
        if (perMessageLimit_ == 0) {
            revert InvalidAmount();
        }

        trustedBridge = trustedBridge_;
        registeredMint = registeredMint_;
        localChainId = localChainId_;
        perMessageLimit = perMessageLimit_;
    }

    function consumeMintMessage(CrossChainMintMessage calldata message) external returns (bytes32) {
        if (msg.sender != trustedBridge) {
            revert UnauthorizedBridge();
        }
        if (message.destinationChainId != localChainId) {
            revert InvalidDestinationChain();
        }
        if (message.mint != registeredMint) {
            revert InvalidMint();
        }
        if (message.recipient == address(0)) {
            revert InvalidRecipient();
        }
        if (message.amount == 0) {
            revert InvalidAmount();
        }
        if (message.amount > perMessageLimit) {
            revert BridgeLimitExceeded();
        }

        bytes32 messageId = keccak256(
            abi.encode(message.sourceChainId, message.destinationChainId, message.nonce)
        );
        if (consumedMessages[messageId]) {
            revert MessageAlreadyConsumed();
        }

        consumedMessages[messageId] = true;

        emit CrossChainMintConsumed(
            messageId,
            message.sourceChainId,
            message.nonce,
            message.mint,
            message.recipient,
            message.amount
        );

        return messageId;
    }
}
