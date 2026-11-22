# Configuration

The `ala` CLI (`crates/ald-cli`) is configured entirely by command-line
flags and the execution manifest; a dyad run needs no environment
variables. Signature:

```text
ala run --manifest <path> --cohort <C> --kind <K> --index <N>
        --evidence-root <dir>
        [--treatment-batch-version N]   (default 2)
        [--protocol-version N]          (default 2)
        [--training-interactions-per-direction N]
        [--learning-mode joint|none|receiver-only]
        [--drop-probability NUM/DEN]
        [--active-sender-tokens 8|16|32]
        [--held-out-types 12,13,14,15]
ala analyze select|score --dyad <dir>... --cohort <C> --out <path>
        [--selection <path>] [--coverage F]
        [--bootstrap-seed S] [--bootstrap-resamples N] [--attempted N]
ala verify <evidence.sqlite>
ala manifest --protocol-version <1|2>
ala journal --journal <path> --line <json>
```

Unknown or bare arguments are rejected with exit code 2 (a silently
dropped `--dyad` would corrupt the cohort count). `analyze` refuses
to overwrite an existing `--out` file. `run` also rejects any repeated
flag with exit code 2 (a duplicated `--drop-probability` would otherwise
silently run the wrong ablation).

The manifest (`protocols/ala-lv-execution-manifest.v2.json`) must
carry `manifestVersion: "ala-lv-execution-manifest/v1"`,
`studyId: "ALA-LV"`, and a `workload` block with positive
`trainingCases`, `validationFitCases`, `validationSelectionCases`,
and `withinSupportTestCases`, each evenly divisible across the two
roles. Study cohorts `A` and `B` additionally require
`<evidence-root>/admission.json` with a GO decision; other cohorts
run without admission. `--index` must fit a zero-padded 4-digit slot
(0–9999).

## Development options (`ala run`)

Five optional `run` flags ablate the dyad for development. They are
parsed in `crates/ald-cli/src/main.rs` (`parse_development_options`),
validated and resolved in `crates/ald-orchestrator/src/lib.rs`
(`parse_*` / `resolve_development_options`), and applied in
`run_dyad_inner`; sender-width support lives in
`crates/ald-learners/src/lib.rs` (`AgentPolicy::with_active_sender_tokens`)
and held-out generation in `crates/ald-scenario/src/lib.rs`
(`generate_cases_excluding`).

| Flag | Default | Semantics |
|---|---|---|
| `--training-interactions-per-direction N` | manifest per-role training count (1500) | Exactly `N` episodes per direction. The persisted folds keep the manifest counts; `N` below the fold trains on the fold's deterministic prefix, `N` above it extends the same seed stream with fresh draws (never duplication). Validation/test folds and eval seed streams are untouched. The sidecar records `N` and `2N`. |
| `--learning-mode joint\|none\|receiver-only` | `joint` | `joint` is the legacy path (both sides REINFORCE). `none` freezes every logit, baseline, and count while consuming the identical sampling draws. `receiver-only` freezes the sender (exactly uniform) while the receiver learns. The mode gates only the update calls. |
| `--drop-probability NUM/DEN` | `1/8` | Exact rational channel-drop probability in `[0,1]`, identical in training and test. The drop test is bit-exact integer arithmetic (`m·DEN < NUM·2⁵³` on the 53-bit draw), so thirds and other non-dyadic values are exact; endpoints `0/1` and `1/1` still consume one channel draw per interaction. `1/8` agrees with the legacy float comparison for every draw. |
| `--active-sender-tokens K` | `32` | Constrains sender sampling, softmax normalization, and REINFORCE updates to the first `K` tokens (`K` in `{8, 16, 32}`); probabilities outside are exact `0.0` and logits outside stay exactly `0.0`. Persisted tables keep the 32-token shape; `K` is recorded in the policy `metadata`. |
| `--held-out-types 12,13,14,15` | empty | Excludes the listed types (distinct codes below 16, at least 4 permitted types left) from all candidate sets — and therefore all targets — in every pre-test fold in both directions, via deterministic per-case replacement fills. Test folds always cover all 16 types. The sidecar persists the per-split per-type exposure census. |

Gating and artifacts:

- Any non-default development option requires `--cohort development`.
  Other cohorts are refused before any directory, journal, or database
  is created, so a rejected run leaves no partial artifacts — even with
  a GO admission record.
- Malformed or out-of-range values are rejected outright (exit 2 from
  the CLI, domain error from `run_dyad`); nothing falls back silently.
- Non-default runs write to a config-suffixed run root
  (`<kind>-<index>-dev-<8 hex>`), so different ablations never share a
  slot directory, and refuse to overwrite an existing root. Legacy runs
  keep the unsuffixed root.
- Non-default runs write a `development-config.json` sidecar in the run
  root: requested options (only the flags actually passed), resolved
  options, digests (`manifestSha256`, `developmentConfigSha256`), the
  per-role per-split generation seeds, the `2N`/`N` counts plus fold
  counts, the exposure census, and the protocol versions. The config
  hash is also bound into the receipt (`developmentConfigSha256`) and
  both journal lines. Default runs write no sidecar and no extra
  receipt/journal fields, so default bytes are unchanged.
- Explicitly passing every default is identical to passing nothing: the
  resolved configuration equals the frozen defaults and the run takes
  the legacy path byte-identically (asserted by
  `dev_explicit_default_identity`).

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

Pinned toolchain: `1.99.1` via `rust-toolchain.toml` (exact patch pin:
byte-identical evidence bodies per `docs/reproduction.md` require a fixed
compiler). Build with `cargo build --workspace`, test with
`cargo test --workspace`. The workspace lints with Clippy pedantic
at `warn` (`Cargo.toml`, `[workspace.lints.clippy]`).

MSRV: `rust-version = "1.87"` on every crate. Floor justification:
locked `thiserror 2.0.21` declares `rust-version = "1.77"`, and
`u64::is_multiple_of` (used in `ald-merkle`, stabilized in 1.87) sets the
floor above it. Verify with `cargo +1.87.0 check --workspace --all-targets`
and reproduce the `docs/reproduction.md` byte-identical dyad diff on the
pinned toolchain.
