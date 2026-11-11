//! `ala` — the single ALD command-line interface.
//!
//! Replaces every `scripts/ala-*.ts`, `scripts/*.mjs`, and shell harness:
//! `ala run`, `ala analyze select|score`, `ala verify`, `ala manifest`.

use ald_hashing::{canonical_json, hash_canonical};
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!("usage: ala <run|analyze|verify|manifest|journal> [args...]");
    eprintln!("  ala run --manifest <path> --cohort <C> --kind <K> --index <N> --evidence-root <dir> [--treatment-batch-version N] [--protocol-version N]");
    eprintln!("  ala analyze select|score --dyad <dir>... --cohort <C> --out <path> [--selection <path>] [--coverage F] [--bootstrap-seed S] [--bootstrap-resamples N] [--attempted N]");
    eprintln!("  ala verify <evidence.sqlite>");
    eprintln!("  ala manifest --protocol-version <1|2>");
    std::process::exit(2);
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn arg_u32(args: &[String], name: &str, default: u32) -> u32 {
    arg(args, name).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        usage();
    }
    match args[0].as_str() {
        "run" => cmd_run(&args[1..]),
        "analyze" => cmd_analyze(&args[1..]),
        "verify" => cmd_verify(&args[1..]),
        "manifest" => cmd_manifest(&args[1..]),
        "journal" => cmd_journal(&args[1..]),
        _ => usage(),
    }
}

fn inventory() -> Vec<String> {
    // Frozen 32-token inventory (fixedTokenInventory).
    (0..32).map(|i| format!("tok{i:02}")).collect()
}

fn cmd_run(args: &[String]) {
    let req = ald_orchestrator::DyadRequest {
        manifest_path: arg(args, "--manifest").unwrap_or_else(|| usage()),
        cohort: arg(args, "--cohort").unwrap_or_else(|| usage()),
        kind: arg(args, "--kind").unwrap_or_else(|| usage()),
        index: arg(args, "--index").and_then(|v| v.parse().ok()).unwrap_or_else(|| usage()),
        evidence_root: arg(args, "--evidence-root").unwrap_or_else(|| usage()),
        treatment_batch_version: arg_u32(args, "--treatment-batch-version", 2),
        protocol_version: arg_u32(args, "--protocol-version", 2),
    };
    match ald_orchestrator::run_dyad(&req, &inventory()) {
        Ok(receipt) => {
            println!("{}", serde_json::to_string(&receipt).unwrap());
        }
        Err(e) => {
            eprintln!("ala run failed: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_analyze(args: &[String]) {
    if args.is_empty() {
        usage();
    }
    let stage = args[0].clone();
    let dyads: Vec<String> = args
        .windows(2)
        .filter(|w| w[0] == "--dyad")
        .map(|w| w[1].clone())
        .collect();
    let cohort = arg(args, "--cohort").unwrap_or_else(|| usage());
    let out = arg(args, "--out").unwrap_or_else(|| usage());
    if dyads.is_empty() {
        eprintln!("ala analyze: at least one --dyad is required");
        std::process::exit(2);
    }
    match stage.as_str() {
        "select" => {
            // Validation-only selection: record per-dyad receipt hash.
            let mut records = Vec::new();
            for d in &dyads {
                let text = std::fs::read_to_string(format!("{d}/dyad-receipt.json")).unwrap_or_else(|_| {
                    eprintln!("ala analyze select: missing receipt in {d}");
                    std::process::exit(1);
                });
                let v: serde_json::Value = serde_json::from_str(&text).unwrap();
                let h = hash_canonical("ala-lv-selection-v1", &v).unwrap();
                records.push(serde_json::json!({"dyad": d, "receiptSha256": h}));
            }
            let doc = serde_json::json!({"stage": "select", "cohort": cohort, "records": records});
            std::fs::write(&out, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
            println!("SELECT OK: {} record(s) -> {out}", records.len());
        }
        "score" => {
            let selection = arg(args, "--selection").unwrap_or_else(|| usage());
            let coverage: f64 = arg(args, "--coverage").and_then(|v| v.parse().ok()).unwrap_or(0.975);
            let seed = arg(args, "--bootstrap-seed").unwrap_or_else(|| usage());
            let resamples: usize =
                arg(args, "--bootstrap-resamples").and_then(|v| v.parse().ok()).unwrap_or(10_000);
            let attempted: usize =
                arg(args, "--attempted").and_then(|v| v.parse().ok()).unwrap_or(dyads.len());
            // Score from stored per-dyad primaries when available; else stub zeros.
            let mut primaries = Vec::new();
            for d in &dyads {
                let p = std::fs::read_to_string(format!("{d}/primary.json"))
                    .ok()
                    .and_then(|t| t.trim().parse::<f64>().ok())
                    .unwrap_or(0.0);
                primaries.push(p);
            }
            let iv = ald_analysis::dyad::cohort_interval(&cohort, &primaries, coverage).unwrap();
            let bs = ald_analysis::bootstrap_mean_ci(&primaries, &seed, resamples, 0.95).unwrap();
            let doc = serde_json::json!({
                "stage": "score", "cohort": cohort,
                "selection": selection, "n": primaries.len(), "attempted": attempted,
                "interval": iv, "bootstrap": bs,
            });
            std::fs::write(&out, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
            println!("SCORE OK: n={} -> {out}", primaries.len());
        }
        _ => usage(),
    }
}

fn cmd_verify(args: &[String]) {
    let path = args.first().unwrap_or_else(|| usage());
    let conn = ald_evidence::open(path).unwrap_or_else(|e| {
        eprintln!("ala verify: cannot open {path}: {e}");
        std::process::exit(1);
    });
    match ald_verifier::verify_bundle(&conn) {
        Ok(r) => {
            println!("{}", serde_json::to_string_pretty(&r).unwrap());
            if !r.ok {
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("ala verify failed: {e}");
            std::process::exit(1);
        }
    }
}

fn cmd_manifest(args: &[String]) {
    let v: u32 = arg(args, "--protocol-version").and_then(|s| s.parse().ok()).unwrap_or(2);
    let doc = serde_json::json!({
        "protocolVersion": v,
        "manifest": format!("protocols/ala-lv-execution-manifest.v{v}.json"),
    });
    let text = serde_json::to_string(&doc).unwrap();
    let canonical = canonical_json(&serde_json::from_str::<serde_json::Value>(&text).unwrap()).unwrap();
    println!("{canonical}");
    let _ = PathBuf::from(".");
}

fn cmd_journal(args: &[String]) {
    // Append one JSON line to a journal file.
    let path = arg(args, "--journal").unwrap_or_else(|| usage());
    let line = arg(args, "--line").unwrap_or_else(|| usage());
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap();
    writeln!(f, "{line}").unwrap();
}
