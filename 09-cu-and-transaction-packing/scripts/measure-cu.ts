import { readFileSync } from "node:fs";

type OutputFormat = "markdown" | "json";

type MeasureOptions = {
  name: string;
  inputPath?: string;
  format: OutputFormat;
};

type ComputeUnitSample = {
  programId: string;
  consumed: number;
  limit: number;
  raw: string;
};

type ComputeUnitReport = {
  instruction: string;
  totalConsumed: number;
  maxLimit: number;
  samples: ComputeUnitSample[];
};

const CU_LOG_PATTERN = /^Program ([1-9A-HJ-NP-Za-km-z]+) consumed (\d+) of (\d+) compute units$/;

function parseArgs(argv: string[]): MeasureOptions {
  let name = "unknown_instruction";
  let inputPath: string | undefined;
  let format: OutputFormat = "markdown";

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = argv[index + 1];

    if (arg === "--name" && next) {
      name = next;
      index += 1;
      continue;
    }

    if (arg === "--log" && next) {
      inputPath = next;
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

  return { name, inputPath, format };
}

function printHelp(): void {
  console.log(`Usage:
  npm run measure-cu -- --name <instruction> --log <path> [--format markdown|json]

Input:
  A simulation/test log containing lines like:
  Program <program_id> consumed <used> of <limit> compute units

Examples:
  npm run measure-cu -- --name transfer_hook --log ./reports/transfer-hook.log
  npm run measure-cu -- --name mint_to_user --log ./reports/mint.log --format json`);
}

function readInput(inputPath?: string): string {
  if (inputPath) {
    return readFileSync(inputPath, "utf8");
  }

  return readFileSync(0, "utf8");
}

function parseComputeUnitSamples(logText: string): ComputeUnitSample[] {
  return logText
    .split(/\r?\n/)
    .map((line) => line.trim())
    .flatMap((line) => {
      const match = CU_LOG_PATTERN.exec(line);

      if (!match) {
        return [];
      }

      return [
        {
          programId: match[1],
          consumed: Number(match[2]),
          limit: Number(match[3]),
          raw: line,
        },
      ];
    });
}

function buildReport(instruction: string, samples: ComputeUnitSample[]): ComputeUnitReport {
  return {
    instruction,
    totalConsumed: samples.reduce((sum, sample) => sum + sample.consumed, 0),
    maxLimit: samples.reduce((max, sample) => Math.max(max, sample.limit), 0),
    samples,
  };
}

function renderMarkdown(report: ComputeUnitReport): string {
  const sampleCount = report.samples.length;
  const programBreakdown = report.samples
    .map((sample) => `${sample.programId}: ${sample.consumed}/${sample.limit}`)
    .join("<br>");

  return [
    "| Instruction | Total CU | Max Limit | Program Samples | Notes |",
    "| --- | ---: | ---: | ---: | --- |",
    `| ${report.instruction} | ${report.totalConsumed} | ${report.maxLimit} | ${sampleCount} | ${programBreakdown || "No CU log lines found"} |`,
  ].join("\n");
}

function main(): void {
  const options = parseArgs(process.argv.slice(2));
  const logText = readInput(options.inputPath);
  const report = buildReport(options.name, parseComputeUnitSamples(logText));

  if (options.format === "json") {
    console.log(JSON.stringify(report, null, 2));
    return;
  }

  console.log(renderMarkdown(report));
}

main();
