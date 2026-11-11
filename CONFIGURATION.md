# Configuration

The `ala` CLI (`crates/ald-cli`) is configured entirely by command-line
flags and the execution manifest; a dyad run needs no environment
variables. Signature:

```text
ala run --manifest <path> --cohort <C> --kind <K> --index <N>
        --evidence-root <dir>
        [--treatment-batch-version N]   (default 2)
        [--protocol-version N]          (default 2)
ala analyze select|score --dyad <dir>... --cohort <C> --out <path>
        [--selection <path>] [--coverage F]
        [--bootstrap-seed S] [--bootstrap-resamples N] [--attempted N]
ala verify <evidence.sqlite>
ala manifest --protocol-version <1|2>
ala journal --journal <path> --line <json>
```

Unknown or bare arguments are rejected with exit code 2 (a silently
dropped `--dyad` would corrupt the cohort count). `analyze` refuses
to overwrite an existing `--out` file.

The manifest (`protocols/ala-lv-execution-manifest.v2.json`) must
carry `manifestVersion: "ala-lv-execution-manifest/v1"`,
`studyId: "ALA-LV"`, and a `workload` block with positive
`trainingCases`, `validationFitCases`, `validationSelectionCases`,
and `withinSupportTestCases`, each evenly divisible across the two
roles. Study cohorts `A` and `B` additionally require
`<evidence-root>/admission.json` with a GO decision; other cohorts
run without admission. `--index` must fit a zero-padded 4-digit slot
(0–9999).

## Runtime environment (`ald-config`)

`crates/ald-config` provides `load_runtime_environment`, the
environment contract for hosts that embed the workspace crates
(`ala` itself does not read the process environment). Variables are
read from a caller-supplied map; defaults apply when a variable is
absent or empty.

| Variable | Type | Default |
|---|---|---|
| `ALD_DTSF_PORT` | Positive integer (1–65535) | `4123` |
| `ALD_TWIN_PACKS_DIR` | Path | `./twin-packs` |
| `ALD_DEPLOYMENT_MODE` | `prototype` or `research-grade` | `prototype` |
| `ALD_EVIDENCE_DIR` | Path | `./evidence` |
| `ALD_DATABASE_PATH` | Path | `./evidence/evidence.sqlite` |
| `ALD_SIGNER_SEEDS_FILE` | Path (optional) | none |
| `ALD_LOG_LEVEL` | `debug`, `info`, `warn`, or `error` | `info` |
| `ALD_ANCHOR_CLASS` | String | `simulated` |
| `ALD_BASE_NETWORK` | String | `base-sepolia` |
| `ALD_BASE_RPC_URL_FILE` | Path (optional) | none |
| `ALD_ANCHOR_KEY_FILE` | Path (optional) | none |

Invalid ports, deployment modes, and log levels fail loading with a
named error instead of falling back silently.

## Secret handling

- Secret values never appear directly in environment variables.
  `load_runtime_environment` rejects `ALD_RUN_SIGNER_SEEDS_JSON`,
  `ALD_BASE_RPC_URL`, and `ALD_ANCHOR_KEY` outright; supply the
  corresponding `*_FILE` paths instead, materialized by `fort` in
  files mode.
- Do not commit `.env` files; the repository ignores `.env` and
  `.env.*`.
- The default anchor class is `simulated`: no wallet, RPC credential,
  or chain transaction is used by any dyad run.
- Evidence artifacts contain no secret material: seeds derive case
  streams and are recorded only through their public derivation
  inputs (cohort, kind, index, role, partition).

## Toolchain

Rust only. Build with `cargo build --workspace`, test with
`cargo test --workspace`. The workspace lints with Clippy pedantic
at `warn` (`Cargo.toml`, `[workspace.lints.clippy]`).
