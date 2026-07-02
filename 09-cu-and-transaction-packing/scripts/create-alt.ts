import { Keypair, PublicKey } from "@solana/web3.js";

type OutputFormat = "markdown" | "json";

type AltPlanOptions = {
  addresses: PublicKey[];
  writableLookups: number;
  format: OutputFormat;
};

type AltPlanReport = {
  lookupTableAddress: string;
  totalAddresses: number;
  writableLookups: number;
  readonlyLookups: number;
  estimatedStaticBytesWithoutAlt: number;
  estimatedLookupIndexBytesWithAlt: number;
  estimatedBytesSaved: number;
  addresses: string[];
};

const PUBKEY_BYTES = 32;
const LOOKUP_INDEX_BYTES = 1;

function parseArgs(argv: string[]): AltPlanOptions {
  let generatedAddressCount: number | undefined;
  const addresses: PublicKey[] = [];
  let writableLookups = 0;
  let format: OutputFormat = "markdown";

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = argv[index + 1];

    if (arg === "--generate" && next) {
      generatedAddressCount = parseNonNegativeInteger(next, "generated address count");
      index += 1;
      continue;
    }

    if (arg === "--address" && next) {
      addresses.push(parsePublicKey(next));
      index += 1;
      continue;
    }

    if (arg === "--writable" && next) {
      writableLookups = parseNonNegativeInteger(next, "writable lookup count");
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

  const targetAddressCount = generatedAddressCount ?? (addresses.length === 0 ? 8 : addresses.length);

  while (addresses.length < targetAddressCount) {
    addresses.push(Keypair.generate().publicKey);
  }

  if (addresses.length === 0) {
    throw new Error("Provide at least one --address or use --generate <count>");
  }

  if (writableLookups > addresses.length) {
    throw new Error("--writable cannot exceed total lookup address count");
  }

  return { addresses, writableLookups, format };
}

function parseNonNegativeInteger(value: string, label: string): number {
  const number = Number(value);

  if (!Number.isInteger(number) || number < 0) {
    throw new Error(`${label} must be a non-negative integer: ${value}`);
  }

  return number;
}

function parsePublicKey(value: string): PublicKey {
  try {
    return new PublicKey(value);
  } catch {
    throw new Error(`Invalid public key: ${value}`);
  }
}

function printHelp(): void {
  console.log(`Usage:
  npm run create-alt -- [--generate <count>] [--address <pubkey> ...] [--writable <count>] [--format markdown|json]

This is an offline Address Lookup Table planner. It does not create an on-chain ALT.

Examples:
  npm run create-alt -- --generate 16
  npm run create-alt -- --generate 24 --writable 4
  npm run create-alt -- --address 11111111111111111111111111111111 --format json`);
}

function buildReport(options: AltPlanOptions): AltPlanReport {
  const totalAddresses = options.addresses.length;
  const estimatedStaticBytesWithoutAlt = totalAddresses * PUBKEY_BYTES;
  const estimatedLookupIndexBytesWithAlt = totalAddresses * LOOKUP_INDEX_BYTES;

  return {
    lookupTableAddress: Keypair.generate().publicKey.toBase58(),
    totalAddresses,
    writableLookups: options.writableLookups,
    readonlyLookups: totalAddresses - options.writableLookups,
    estimatedStaticBytesWithoutAlt,
    estimatedLookupIndexBytesWithAlt,
    estimatedBytesSaved: estimatedStaticBytesWithoutAlt - estimatedLookupIndexBytesWithAlt,
    addresses: options.addresses.map((address) => address.toBase58()),
  };
}

function renderMarkdown(report: AltPlanReport): string {
  return [
    `ALT plan: ${report.lookupTableAddress}`,
    "",
    "| Metric | Value |",
    "| --- | ---: |",
    `| Total addresses | ${report.totalAddresses} |`,
    `| Writable lookups | ${report.writableLookups} |`,
    `| Readonly lookups | ${report.readonlyLookups} |`,
    `| Static bytes without ALT | ${report.estimatedStaticBytesWithoutAlt} |`,
    `| Lookup index bytes with ALT | ${report.estimatedLookupIndexBytesWithAlt} |`,
    `| Estimated bytes saved | ${report.estimatedBytesSaved} |`,
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
