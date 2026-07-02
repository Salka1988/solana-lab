import {
  Keypair,
  PublicKey,
  SystemProgram,
  TransactionInstruction,
  TransactionMessage,
  VersionedTransaction,
} from "@solana/web3.js";

type OutputFormat = "markdown" | "json";

type BuildOptions = {
  transfers: number;
  memoBytes: number;
  format: OutputFormat;
};

type VersionedTxReport = {
  version: "v0";
  instructions: number;
  staticAccountKeys: number;
  signatures: number;
  serializedBytes: number;
  maxLegacyPacketBytes: number;
  fitsPacket: boolean;
};

const MAX_LEGACY_PACKET_BYTES = 1_232;
const DEFAULT_RECENT_BLOCKHASH = "11111111111111111111111111111111";

function parseArgs(argv: string[]): BuildOptions {
  let transfers = 1;
  let memoBytes = 0;
  let format: OutputFormat = "markdown";

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = argv[index + 1];

    if (arg === "--transfers" && next) {
      transfers = parseNonNegativeInteger(next, "transfers");
      index += 1;
      continue;
    }

    if (arg === "--memo-bytes" && next) {
      memoBytes = parseNonNegativeInteger(next, "memo bytes");
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

  if (transfers === 0 && memoBytes === 0) {
    throw new Error("Build at least one transfer or memo instruction");
  }

  return { transfers, memoBytes, format };
}

function parseNonNegativeInteger(value: string, label: string): number {
  const number = Number(value);

  if (!Number.isInteger(number) || number < 0) {
    throw new Error(`${label} must be a non-negative integer: ${value}`);
  }

  return number;
}

function printHelp(): void {
  console.log(`Usage:
  npm run build-versioned-tx -- [--transfers <count>] [--memo-bytes <bytes>] [--format markdown|json]

Examples:
  npm run build-versioned-tx -- --transfers 4
  npm run build-versioned-tx -- --transfers 2 --memo-bytes 256
  npm run build-versioned-tx -- --transfers 8 --format json`);
}

function buildInstructions(payer: PublicKey, options: BuildOptions): TransactionInstruction[] {
  const instructions: TransactionInstruction[] = [];

  for (let index = 0; index < options.transfers; index += 1) {
    instructions.push(
      SystemProgram.transfer({
        fromPubkey: payer,
        toPubkey: Keypair.generate().publicKey,
        lamports: 1,
      }),
    );
  }

  if (options.memoBytes > 0) {
    instructions.push(
      new TransactionInstruction({
        programId: new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"),
        keys: [],
        data: Buffer.alloc(options.memoBytes, "x"),
      }),
    );
  }

  return instructions;
}

function buildReport(options: BuildOptions): VersionedTxReport {
  const payer = Keypair.generate();
  const instructions = buildInstructions(payer.publicKey, options);
  const message = new TransactionMessage({
    payerKey: payer.publicKey,
    recentBlockhash: DEFAULT_RECENT_BLOCKHASH,
    instructions,
  }).compileToV0Message();
  const transaction = new VersionedTransaction(message);

  transaction.sign([payer]);

  const serializedBytes = transaction.serialize().length;

  return {
    version: "v0",
    instructions: instructions.length,
    staticAccountKeys: message.staticAccountKeys.length,
    signatures: transaction.signatures.length,
    serializedBytes,
    maxLegacyPacketBytes: MAX_LEGACY_PACKET_BYTES,
    fitsPacket: serializedBytes <= MAX_LEGACY_PACKET_BYTES,
  };
}

function renderMarkdown(report: VersionedTxReport): string {
  return [
    "| Version | Instructions | Static Accounts | Signatures | Serialized Bytes | Fits 1232 Bytes |",
    "| --- | ---: | ---: | ---: | ---: | --- |",
    `| ${report.version} | ${report.instructions} | ${report.staticAccountKeys} | ${report.signatures} | ${report.serializedBytes} | ${report.fitsPacket ? "yes" : "no"} |`,
  ].join("\n");
}

function main(): void {
  const options = parseArgs(process.argv.slice(2));
  const report = buildReport(options);

  if (options.format === "json") {
    console.log(JSON.stringify(report, null, 2));
    return;
  }

  console.log(renderMarkdown(report));
}

main();
