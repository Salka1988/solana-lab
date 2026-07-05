import assert from "node:assert/strict";
import { createHash } from "node:crypto";

class MockBridgeReceiverModel {
  constructor({ trustedBridge, registeredMint, localChainId, perMessageLimit }) {
    this.trustedBridge = trustedBridge;
    this.registeredMint = registeredMint;
    this.localChainId = localChainId;
    this.perMessageLimit = perMessageLimit;
    this.consumedMessages = new Set();
  }

  messageId(message) {
    return createHash("sha256")
      .update(`${message.sourceChainId}:${message.destinationChainId}:${message.nonce}`)
      .digest("hex");
  }

  consumeMintMessage(sender, message) {
    if (sender !== this.trustedBridge) throw new Error("UnauthorizedBridge");
    if (message.destinationChainId !== this.localChainId) throw new Error("InvalidDestinationChain");
    if (message.mint !== this.registeredMint) throw new Error("InvalidMint");
    if (message.recipient === "0x0000000000000000000000000000000000000000") {
      throw new Error("InvalidRecipient");
    }
    if (message.amount === 0n) throw new Error("InvalidAmount");
    if (message.amount > this.perMessageLimit) throw new Error("BridgeLimitExceeded");

    const messageId = this.messageId(message);
    if (this.consumedMessages.has(messageId)) throw new Error("MessageAlreadyConsumed");

    this.consumedMessages.add(messageId);
    return messageId;
  }
}

const trustedBridge = "0x1111111111111111111111111111111111111111";
const registeredMint = "0x2222222222222222222222222222222222222222";
const recipient = "0x3333333333333333333333333333333333333333";

const receiver = new MockBridgeReceiverModel({
  trustedBridge,
  registeredMint,
  localChainId: 1,
  perMessageLimit: 1_000n,
});

const message = {
  sourceChainId: 2,
  destinationChainId: 1,
  nonce: 77n,
  mint: registeredMint,
  recipient,
  amount: 500n,
};

const firstMessageId = receiver.consumeMintMessage(trustedBridge, message);

assert.ok(receiver.consumedMessages.has(firstMessageId));
assert.throws(
  () => receiver.consumeMintMessage(trustedBridge, message),
  /MessageAlreadyConsumed/,
);

assert.throws(
  () => receiver.consumeMintMessage("0x4444444444444444444444444444444444444444", { ...message, nonce: 78n }),
  /UnauthorizedBridge/,
);
assert.throws(
  () => receiver.consumeMintMessage(trustedBridge, { ...message, nonce: 79n, destinationChainId: 9 }),
  /InvalidDestinationChain/,
);
assert.throws(
  () => receiver.consumeMintMessage(trustedBridge, { ...message, nonce: 80n, mint: recipient }),
  /InvalidMint/,
);
assert.throws(
  () => receiver.consumeMintMessage(trustedBridge, { ...message, nonce: 81n, amount: 1_001n }),
  /BridgeLimitExceeded/,
);

console.log("evm replay model ok");
