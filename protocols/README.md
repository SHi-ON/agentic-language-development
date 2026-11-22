# Execution manifests

Canonical manifest: `ala-lv-execution-manifest.v2.json`. Every command
in `docs/reproduction.md` uses it. Its bytes are frozen and pinned:
sha256 `ba6a0d08…` (`ba6a0d08018b75f8b59e1475f9757a1be54c1d22f952f30ed2044d89f06ee304`)
in the paper `results/esd/inputs-manifest.json` anchors, and
`manifest_sha256` in every dyad receipt. Seed root
`ala-ledger-value-v2`; used by the v5 confirmatory and v4 robustness
campaigns (`PROTOCOL_VERSION = 2`; no shipped evidence carries
protocol version 1).

`ala-lv-execution-manifest.v1.json` is retained frozen, not dropped,
because protocol version 1 is still a supported run mode: `ala run
--protocol-version 1` selects seed root `ala-ledger-value-v1`
(`crates/ald-orchestrator/src/run.rs`), and `ala manifest` must serve
versions 1 and 2 (P0-FORK-02). Nothing in the shipped campaigns uses
it; it exists so the v1 mode keeps resolving to its frozen record.

`manifestVersion` names the envelope schema (`ala-lv-execution-manifest/v1`
in both files), not the protocol version. The protocol version is
carried by `seedPolicy.root` (`ala-ledger-value-v1` vs `-v2`) and the
file name. `ala manifest` checks all three agree with the requested
`--protocol-version` and exits 2 naming the file on mismatch.
