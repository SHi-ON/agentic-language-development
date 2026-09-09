# Verifiable-Core Completion Plan

**Status:** Local execution complete; external evidence gates remain
**Scope:** `agentic-language-development` only  
**Starting commit:** `a2a11453e4d4e1c384993b6541ad9c61da13ee90`

This plan follows the implemented backlog into research-execution readiness. It
does not turn software qualification into an empirical result, and it does not
mark organization-, funding-, or independent-human gates complete without the
required evidence.

## Execution sequence

1. [x] **Close the dependency-integrity gap.** Remove all high/critical npm
   advisories from the locked graph and make the audit a blocking consolidated
   CI step with a retained machine-readable report.
   -> **Check:** `npm ci` succeeds from the lockfile, `npm audit
   --audit-level=high` exits zero, and the workflow uploads the audit JSON even
   when another check fails.
2. [x] **Make the E03 design calculation reproducible.** Implement the
   deterministic Monte Carlo calculation and seed/reserve manifest described by
   `RESEARCH.md` Appendix D rather than leaving its reported power values as
   prose-only calculations.
   -> **Check:** fixed-seed tests reproduce identical output, cover every
   sample-size/SD row, and fail if a row falls below the registered 90% power
   floor.
3. [x] **Compile canonical E03 preregistration artifacts.** Produce a validated,
   canonical `PreRegistrationArtifact`, its domain-separated hash, the complete
   primary/reserve seed manifest, and run-config templates that all carry that
   exact hash.
   -> **Check:** schema validation passes; canonical output is byte-identical on
   repeat; changing a registered field changes the hash; run IDs, realized
   random seeds, and the hash itself are excluded from the hashed parameter
   template.
4. [x] **Add a fail-closed research preflight.** Evaluate an artifact and its
   execution binding before confirmatory collection, reporting every blocker in
   both JSON and human-readable form.
   -> **Check:** qualification/confirmatory class, immutable commit, non-placeholder
   hashes, Mode R, independent learners, seed count, external registration, and
   confirmed matching pre-run anchor are each independently exercised by tests.
5. [x] **Exercise the frozen-LLM path with real open weights.** Add an operator
   command for a loopback OpenAI-compatible server, hash the local weight file,
   run both Baby roles through the shared learner contract, and retain a
   qualification report carrying the exact provenance.
   -> **Check:** the command rejects non-loopback endpoints and missing weights;
   a public 3B-8B open-weight model completes the qualification locally; the
   report is explicitly labeled non-confirmatory and contains no model prompt or
   private output.
6. [x] **Synchronize the public project state and regenerate derived artifacts.**
   Correct stale implementation claims in `README.md` and `RESEARCH.md`, add a
   drift check for plan/version/status surfaces, and regenerate the research
   book after the manuscript change.
   -> **Check:** the drift check, full consolidated suite, real-container Mode R
   suite, book generation, secret scan, and a clean Git status all pass on the
   exact pushed commit.

## External evidence gates

These are part of the real project finish line but cannot be satisfied by local
code or by substituting simulated evidence:

- Base Sepolia public transaction evidence (`ALD-020`) requires a funded signer
  provisioned through the workspace's `si fort` secret boundary.
- Base mainnet staging evidence (`ALD-022`) requires explicit spending authority,
  an approved custody decision, and funds.
- Required-check enforcement (`ALD-078`) requires an upstream maintainer to
  approve the fork workflow and configure branch protection.
- Restore validation (`ALD-079`) requires a second human operator.
- External registration and governance decisions remain prerequisites to any
  confirmatory experiment, even after the local artifact and preflight tooling
  are complete.

## Completion rule

Each implementation step receives its own logical commit and patch-version bump.
The branch is pushed only after its local checks pass. Hosted consolidated and Mode R
results are reported against the exact pushed commit rather than written back into
that commit, which would create a self-referential verification loop.
