type OutputFormat = "markdown" | "json";

type AccountSpec = {
  name: string;
  bytes: number;
};

type MeasureOptions = {
  accounts: AccountSpec[];
  lamportsPerByteYear: number;
  exemptionThreshold: number;
  format: OutputFormat;
};

type AccountSizeReport = {
  name: string;
  bytes: number;
  rentExemptLamports: number;
};

const DEFAULT_LAMPORTS_PER_BYTE_YEAR = 3_480;
const DEFAULT_EXEMPTION_THRESHOLD = 2;
const ACCOUNT_STORAGE_OVERHEAD_BYTES = 128;

function parseArgs(argv: string[]): MeasureOptions {
  const accounts: AccountSpec[] = [];
  let lamportsPerByteYear = DEFAULT_LAMPORTS_PER_BYTE_YEAR;
  let exemptionThreshold = DEFAULT_EXEMPTION_THRESHOLD;
  let format: OutputFormat = "markdown";

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = argv[index + 1];

    if (arg === "--account" && next) {
      accounts.push(parseAccountSpec(next));
      index += 1;
      continue;
    }

    if (arg === "--lamports-per-byte-year" && next) {
      lamportsPerByteYear = parsePositiveNumber(next, "lamports per byte year");
      index += 1;
      continue;
    }

    if (arg === "--exemption-threshold" && next) {
      exemptionThreshold = parsePositiveNumber(next, "exemption threshold");
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

  if (accounts.length === 0) {
    throw new Error("Provide at least one --account <name>:<bytes> argument");
  }

  return { accounts, lamportsPerByteYear, exemptionThreshold, format };
}

function parseAccountSpec(value: string): AccountSpec {
  const [name, bytesText] = value.split(":");

  if (!name || !bytesText) {
    throw new Error(`Account must use <name>:<bytes> format: ${value}`);
  }

  return {
    name,
    bytes: parsePositiveInteger(bytesText, "account bytes"),
  };
}

function parsePositiveInteger(value: string, label: string): number {
  const number = Number(value);

  if (!Number.isInteger(number) || number <= 0) {
    throw new Error(`${label} must be a positive integer: ${value}`);
  }

  return number;
}

function parsePositiveNumber(value: string, label: string): number {
  const number = Number(value);

  if (!Number.isFinite(number) || number <= 0) {
    throw new Error(`${label} must be positive: ${value}`);
  }

  return number;
}

function printHelp(): void {
  console.log(`Usage:
  npm run measure-account-size -- --account <name>:<bytes> [--account <name>:<bytes> ...]

Defaults:
  lamports per byte year: ${DEFAULT_LAMPORTS_PER_BYTE_YEAR}
  exemption threshold: ${DEFAULT_EXEMPTION_THRESHOLD}

Examples:
  npm run measure-account-size -- --account RedemptionRequest:137 --account AdminActionLog:130
  npm run measure-account-size -- --account ProtocolConfig:90 --format json`);
}

function rentExemptLamports(
  dataBytes: number,
  lamportsPerByteYear: number,
  exemptionThreshold: number,
): number {
  return Math.ceil((dataBytes + ACCOUNT_STORAGE_OVERHEAD_BYTES) * lamportsPerByteYear * exemptionThreshold);
}

function buildReports(options: MeasureOptions): AccountSizeReport[] {
  return options.accounts.map((account) => ({
    name: account.name,
    bytes: account.bytes,
    rentExemptLamports: rentExemptLamports(
      account.bytes,
      options.lamportsPerByteYear,
      options.exemptionThreshold,
    ),
  }));
}

function renderMarkdown(reports: AccountSizeReport[]): string {
  const rows = reports.map(
    (report) =>
      `| ${report.name} | ${report.bytes} | ${report.rentExemptLamports} | ${lamportsToSol(report.rentExemptLamports)} |`,
  );

  return [
    "| Account | Bytes | Rent Exempt Lamports | Rent Exempt SOL |",
    "| --- | ---: | ---: | ---: |",
    ...rows,
  ].join("\n");
}

function lamportsToSol(lamports: number): string {
  return (lamports / 1_000_000_000).toFixed(9);
}

function main(): void {
  const options = parseArgs(process.argv.slice(2));
  const reports = buildReports(options);

  if (options.format === "json") {
    console.log(JSON.stringify(reports, null, 2));
    return;
  }

  console.log(renderMarkdown(reports));
}

main();
