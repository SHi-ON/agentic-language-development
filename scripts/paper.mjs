#!/usr/bin/env node
/**
 * Paper command facade (P2 interface; capabilities land in P3-P6).
 *
 * Each verb either runs today behind reused helpers or fails closed with
 * the owning phase. No verb ever reports success without doing the work.
 */
import { execFileSync } from "node:child_process";

const [verb, ...rest] = process.argv.slice(2);

function runPnpm(args) {
  execFileSync("pnpm", ["run", ...args], { stdio: "inherit" });
}

function notImplemented(phase) {
  console.error(
    `FAIL: paper:${verb} is not implemented until ${phase}; no action taken.`
  );
  process.exit(10);
}

switch (verb) {
  case "validate":
    // Reused helpers available today: secret scan + frozen claim boundary
    // + ALA-LV protocol schema/binding check.
    runPnpm(["scan:secrets"]);
    runPnpm(["audit:data-claims"]);
    runPnpm(["check:ala-protocol"]);
    console.log("paper:validate OK");
    break;
  case "audit":
    // Partial today: frozen claim boundary + status report. Full study-evidence
    // audit arrives with the P6 sealed export.
    runPnpm(["audit:data-claims"]);
    console.log(
      "paper:audit PARTIAL: frozen claim boundary verified; " +
        "study-evidence audit pending P6 sealed export."
    );
    break;
  case "qualify":
    notImplemented("P4");
    break;
  case "calibrate":
    notImplemented("P5");
    break;
  case "freeze":
    notImplemented("P3 (protocol freeze) / P6 (export freeze)");
    break;
  case "run":
    if (!rest.includes("--manifest")) {
      console.error("FAIL: paper:run requires --manifest <path> (P6).");
      process.exit(11);
    }
    notImplemented("P6");
    break;
  case "analyze":
    notImplemented("P6");
    break;
  case "export":
    notImplemented("P6");
    break;
  case "reproduce":
    notImplemented("P6/P9");
    break;
  default:
    console.error(
      `FAIL: unknown verb '${verb}'. ` +
        "Expected one of: validate qualify calibrate freeze run audit analyze export reproduce."
    );
    process.exit(12);
}
