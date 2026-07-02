import {
  Keypair,
  PublicKey,
  SystemProgram,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";

type OutputFormat = "markdown" | "json";
type FlowName = "issue" | "transfer_hook" | "redeem" | "full_lifecycle";

type FlowOptions = {
  flow: FlowName;
  format: OutputFormat;
};

type AccountRole = {
  name: string;
  pubkey: PublicKey;
  writable: boolean;
};

type FlowReport = {
  flow: FlowName;
  instructions: number;
  staticAccountKeys: number;
  writableAccounts: number;
  signatures: number;
  serializedBytes: number;
  fitsPacket: boolean;
  altCandidateAccounts: number;
  estimatedAltBytesSaved: number;
  splitRecommendation: string;
};

const MAX_PACKET_BYTES = 1_232;
const DEFAULT_RECENT_BLOCKHASH = "11111111111111111111111111111111";
const LOOKUP_INDEX_BYTES = 1;
const PUBKEY_BYTES = 32;

const PROGRAM_IDS = {
  modularIssuer: new PublicKey("6zKsNTfMRviuMCxkGS1JgbpPzPJC4ZZJFP1qLEKCdNq6"),
  complianceHook: new PublicKey("Hook111111111111111111111111111111111111111"),
  redemptionAdmin: new PublicKey("9Nw7daj1a4bqTL5R9qFCCGUnDWEPfk7zhFbu9V26WuCr"),
  // Placeholder key for packet-size modeling. Exact program identity does not affect serialized size.
  token2022: new PublicKey("D6vhjzvtRvXdnMcuvfBGiDMm1gxdJJvgB21ZNxkvf6yx"),
  associatedToken: new PublicKey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL"),
};

function parseArgs(argv: string[]): FlowOptions {
  let flow: FlowName = "full_lifecycle";
  let format: OutputFormat = "markdown";

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = argv[index + 1];

    if (arg === "--flow" && next) {
      if (!isFlowName(next)) {
        throw new Error(`Unsupported flow: ${next}`);
      }

      flow = next;
      index += 1;
      continue;
    }

    if (arg === "--format" && next) {
      if (next !== "markdown" && next !== "json") {
        throw new Error(`Unsupported format: ${next}`);
      }

      format = next;
      index += 1;
      continue;
    }

    if (arg === "--help" || arg === "-h") {
      printHelp();
      process.exit(0);
    }

    throw new Error(`Unknown or incomplete argument: ${arg}`);
  }

  return { flow, format };
}

function isFlowName(value: string): value is FlowName {
  return value === "issue" || value === "transfer_hook" || value === "redeem" || value === "full_lifecycle";
}

function printHelp(): void {
  console.log(`Usage:
  npm run packed-flow-demo -- [--flow issue|transfer_hook|redeem|full_lifecycle] [--format markdown|json]

Examples:
  npm run packed-flow-demo -- --flow issue
  npm run packed-flow-demo -- --flow redeem --format json
  npm run packed-flow-demo -- --flow full_lifecycle`);
}

function key(): PublicKey {
  return Keypair.generate().publicKey;
}

function account(name: string, writable = false): AccountRole {
  return { name, pubkey: key(), writable };
}

function instruction(programId: PublicKey, dataBytes: number, accounts: AccountRole[]): TransactionInstruction {
  return new TransactionInstruction({
    programId,
    keys: accounts.map((role) => ({
      pubkey: role.pubkey,
      isSigner: false,
      isWritable: role.writable,
    })),
    data: Buffer.alloc(dataBytes, 1),
  });
}

function commonAccounts(): Record<string, AccountRole> {
  return {
    protocolConfig: account("protocolConfig", true),
    stablecoinMint: account("stablecoinMint", true),
    mintAuthority: account("mintAuthority"),
    issuerConfig: account("issuerConfig", true),
    issuerStats: account("issuerStats", true),
    supplyStats: account("supplyStats", true),
    userTokenAccount: account("userTokenAccount", true),
    destinationTokenAccount: account("destinationTokenAccount", true),
    complianceConfig: account("complianceConfig", true),
    sourceCompliance: account("sourceCompliance", true),
    destinationCompliance: account("destinationCompliance"),
    extraAccountMetaList: account("extraAccountMetaList"),
    redemptionVault: account("redemptionVault", true),
    redemptionRequest: account("redemptionRequest", true),
    adminActionLog: account("adminActionLog", true),
    systemProgram: { name: "systemProgram", pubkey: SystemProgram.programId, writable: false },
    token2022: { name: "token2022", pubkey: PROGRAM_IDS.token2022, writable: false },
    associatedToken: { name: "associatedToken", pubkey: PROGRAM_IDS.associatedToken, writable: false },
  };
}

function issueInstructions(accounts: Record<string, AccountRole>): TransactionInstruction[] {
  return [
    instruction(PROGRAM_IDS.modularIssuer, 8, [
      accounts.protocolConfig,
      accounts.stablecoinMint,
      accounts.supplyStats,
      accounts.systemProgram,
      accounts.token2022,
    ]),
    instruction(PROGRAM_IDS.modularIssuer, 16, [
      accounts.protocolConfig,
      accounts.issuerConfig,
      accounts.issuerStats,
      accounts.systemProgram,
    ]),
    instruction(PROGRAM_IDS.modularIssuer, 16, [
      accounts.protocolConfig,
      accounts.stablecoinMint,
      accounts.mintAuthority,
      accounts.issuerConfig,
      accounts.issuerStats,
      accounts.supplyStats,
      accounts.userTokenAccount,
      accounts.token2022,
      accounts.associatedToken,
      accounts.systemProgram,
    ]),
  ];
}

function transferHookInstructions(accounts: Record<string, AccountRole>): TransactionInstruction[] {
  return [
    instruction(PROGRAM_IDS.token2022, 16, [
      accounts.userTokenAccount,
      accounts.stablecoinMint,
      accounts.destinationTokenAccount,
      accounts.complianceConfig,
      accounts.sourceCompliance,
      accounts.destinationCompliance,
      accounts.extraAccountMetaList,
    ]),
  ];
}

function redeemInstructions(accounts: Record<string, AccountRole>): TransactionInstruction[] {
  return [
    instruction(PROGRAM_IDS.redemptionAdmin, 16, [
      accounts.protocolConfig,
      accounts.redemptionVault,
      accounts.redemptionRequest,
      accounts.systemProgram,
    ]),
    instruction(PROGRAM_IDS.redemptionAdmin, 8, [
      accounts.protocolConfig,
      accounts.redemptionVault,
      accounts.redemptionRequest,
      accounts.adminActionLog,
      accounts.systemProgram,
    ]),
  ];
}

function buildFlowInstructions(flow: FlowName): TransactionInstruction[] {
  const accounts = commonAccounts();

  if (flow === "issue") {
    return issueInstructions(accounts);
  }

  if (flow === "transfer_hook") {
    return transferHookInstructions(accounts);
  }

  if (flow === "redeem") {
    return redeemInstructions(accounts);
  }

  return [
    ...issueInstructions(accounts),
    ...transferHookInstructions(accounts),
    ...redeemInstructions(accounts),
  ];
}

function buildReport(flow: FlowName): FlowReport {
  const payer = Keypair.generate();
  const instructions = buildFlowInstructions(flow);
  const message = new TransactionMessage({
    payerKey: payer.publicKey,
    recentBlockhash: DEFAULT_RECENT_BLOCKHASH,
    instructions,
  }).compileToV0Message();
  const transaction = new VersionedTransaction(message);

  transaction.sign([payer]);

  const serializedBytes = transaction.serialize().length;
  const writableAccounts = message.staticAccountKeys.filter((accountKey) =>
    instructions.some((ix) => ix.keys.some((meta) => meta.pubkey.equals(accountKey) && meta.isWritable)),
  ).length;
  const altCandidateAccounts = Math.max(0, message.staticAccountKeys.length - transaction.signatures.length - 3);
  const estimatedAltBytesSaved = altCandidateAccounts * (PUBKEY_BYTES - LOOKUP_INDEX_BYTES);

  return {
    flow,
    instructions: instructions.length,
    staticAccountKeys: message.staticAccountKeys.length,
    writableAccounts,
    signatures: transaction.signatures.length,
    serializedBytes,
    fitsPacket: serializedBytes <= MAX_PACKET_BYTES,
    altCandidateAccounts,
    estimatedAltBytesSaved,
    splitRecommendation: splitRecommendation(serializedBytes, altCandidateAccounts),
  };
}

function splitRecommendation(serializedBytes: number, altCandidateAccounts: number): string {
  if (serializedBytes > MAX_PACKET_BYTES) {
    return "split flow or use ALT";
  }

  if (altCandidateAccounts >= 8) {
    return "fits, ALT likely useful for repeated accounts";
  }

  return "fits, ALT optional";
}

function renderMarkdown(report: FlowReport): string {
  return [
    `Packed flow: ${report.flow}`,
    "",
    "| Metric | Value |",
    "| --- | ---: |",
    `| Instructions | ${report.instructions} |`,
    `| Static account keys | ${report.staticAccountKeys} |`,
    `| Writable accounts | ${report.writableAccounts} |`,
    `| Signatures | ${report.signatures} |`,
    `| Serialized bytes | ${report.serializedBytes} |`,
    `| Fits 1232 bytes | ${report.fitsPacket ? "yes" : "no"} |`,
    `| ALT candidate accounts | ${report.altCandidateAccounts} |`,
    `| Estimated ALT bytes saved | ${report.estimatedAltBytesSaved} |`,
    `| Recommendation | ${report.splitRecommendation} |`,
  ].join("\n");
}

function main(): void {
  const options = parseArgs(process.argv.slice(2));
  const report = buildReport(options.flow);

  if (options.format === "json") {
    console.log(JSON.stringify(report, null, 2));
    return;
  }

  console.log(renderMarkdown(report));
}

main();
