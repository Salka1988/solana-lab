import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";

function runScript(script, args = [], input = undefined) {
  const result = spawnSync(process.execPath, [`dist/${script}.js`, ...args], {
    cwd: process.cwd(),
    encoding: "utf8",
    input,
  });

  assert.equal(result.status, 0, result.stderr || result.stdout);

  return result.stdout.trim();
}

function runScriptFailure(script, args = [], input = undefined) {
  const result = spawnSync(process.execPath, [`dist/${script}.js`, ...args], {
    cwd: process.cwd(),
    encoding: "utf8",
    input,
  });

  assert.notEqual(result.status, 0, "expected command to fail");

  return `${result.stdout}\n${result.stderr}`;
}

test("measure-cu parses runtime CU logs from stdin", () => {
  const output = runScript(
    "measure-cu",
    ["--name", "transfer_hook", "--format", "json"],
    [
      "Program Hook111111111111111111111111111111111111111 consumed 42000 of 200000 compute units",
      "Program TokenzQdBNbLqP5VEhdkAS6EPF2Ke6ZgL7nL7 consumed 18000 of 200000 compute units",
    ].join("\n"),
  );
  const report = JSON.parse(output);

  assert.equal(report.instruction, "transfer_hook");
  assert.equal(report.totalConsumed, 60_000);
  assert.equal(report.maxLimit, 200_000);
  assert.equal(report.samples.length, 2);
  assert.equal(report.samples[0].programId, "Hook111111111111111111111111111111111111111");
  assert.equal(report.samples[1].consumed, 18_000);
});

test("measure-account-size reports rent estimate for an account", () => {
  const output = runScript("measure-account-size", [
    "--account",
    "RedemptionRequest:137",
    "--format",
    "json",
  ]);
  const report = JSON.parse(output);

  assert.equal(report.length, 1);
  assert.equal(report[0].name, "RedemptionRequest");
  assert.equal(report[0].bytes, 137);
  assert.equal(report[0].rentExemptLamports, 1_844_400);
});

test("build-versioned-tx reports v0 transaction size", () => {
  const output = runScript("build-versioned-tx", [
    "--transfers",
    "4",
    "--format",
    "json",
  ]);
  const report = JSON.parse(output);

  assert.equal(report.version, "v0");
  assert.equal(report.instructions, 4);
  assert.equal(report.staticAccountKeys, 6);
  assert.equal(report.signatures, 1);
  assert.equal(report.serializedBytes, 364);
  assert.equal(report.maxLegacyPacketBytes, 1_232);
  assert.equal(report.fitsPacket, true);
});

test("create-alt reports explicit address without generated padding", () => {
  const output = runScript("create-alt", [
    "--address",
    "11111111111111111111111111111111",
    "--format",
    "json",
  ]);
  const report = JSON.parse(output);

  assert.equal(report.totalAddresses, 1);
  assert.equal(report.writableLookups, 0);
  assert.equal(report.readonlyLookups, 1);
  assert.equal(report.estimatedStaticBytesWithoutAlt, 32);
  assert.equal(report.estimatedLookupIndexBytesWithAlt, 1);
  assert.equal(report.estimatedBytesSaved, 31);
  assert.deepEqual(report.addresses, ["11111111111111111111111111111111"]);
});

test("packed-flow-demo reports full lifecycle packing recommendation", () => {
  const output = runScript("packed-flow-demo", [
    "--flow",
    "full_lifecycle",
    "--format",
    "json",
  ]);
  const report = JSON.parse(output);

  assert.equal(report.flow, "full_lifecycle");
  assert.equal(report.instructions, 6);
  assert.equal(report.staticAccountKeys, 21);
  assert.equal(report.writableAccounts, 12);
  assert.equal(report.signatures, 1);
  assert.equal(report.serializedBytes, 909);
  assert.equal(report.fitsPacket, true);
  assert.equal(report.altCandidateAccounts, 17);
  assert.equal(report.estimatedAltBytesSaved, 527);
  assert.equal(report.splitRecommendation, "fits, ALT likely useful for repeated accounts");
});

test("packed-flow-demo reports redeem flow as small enough without required ALT", () => {
  const output = runScript("packed-flow-demo", [
    "--flow",
    "redeem",
    "--format",
    "json",
  ]);
  const report = JSON.parse(output);

  assert.equal(report.flow, "redeem");
  assert.equal(report.instructions, 2);
  assert.equal(report.staticAccountKeys, 7);
  assert.equal(report.writableAccounts, 4);
  assert.equal(report.serializedBytes, 367);
  assert.equal(report.altCandidateAccounts, 3);
  assert.equal(report.splitRecommendation, "fits, ALT optional");
});

test("create-alt rejects writable count greater than address count", () => {
  const failure = runScriptFailure("create-alt", [
    "--address",
    "11111111111111111111111111111111",
    "--writable",
    "2",
  ]);

  assert.match(failure, /--writable cannot exceed total lookup address count/);
});

test("measure-account-size rejects malformed account specs", () => {
  const failure = runScriptFailure("measure-account-size", ["--account", "MissingBytes"]);

  assert.match(failure, /Account must use <name>:<bytes> format/);
});
