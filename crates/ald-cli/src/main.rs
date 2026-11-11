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
    arg(args, name)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
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
    reject_stray_args(
        args,
        "run",
        &[
            "--manifest",
            "--cohort",
            "--kind",
            "--index",
            "--evidence-root",
            "--treatment-batch-version",
            "--protocol-version",
        ],
    );
    let req = ald_orchestrator::DyadRequest {
        manifest_path: arg(args, "--manifest").unwrap_or_else(|| usage()),
        cohort: arg(args, "--cohort").unwrap_or_else(|| usage()),
        kind: arg(args, "--kind").unwrap_or_else(|| usage()),
        index: arg(args, "--index")
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| usage()),
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

/// Reject unknown or bare arguments: silently dropping a `--dyad`
/// value would corrupt `n` with a zero exit code.
fn reject_stray_args(args: &[String], what: &str, flags: &[&str]) {
    let mut i = 0;
    while i < args.len() {
        if flags.contains(&args[i].as_str()) {
            i += 2; // flag + value; a missing value surfaces as usage() below
        } else {
            eprintln!("ala {what}: unexpected argument {:?}", args[i]);
            std::process::exit(2);
        }
    }
}

fn cmd_analyze(args: &[String]) {
    if args.is_empty() {
        usage();
    }
    let stage = args[0].clone();
    reject_stray_args(
        &args[1..],
        "analyze",
        &[
            "--dyad",
            "--cohort",
            "--out",
            "--selection",
            "--coverage",
            "--bootstrap-seed",
            "--bootstrap-resamples",
            "--attempted",
        ],
    );
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
        "select" => cmd_select(&dyads, &cohort, &out),
        "score" => cmd_score(args, &dyads, &cohort, &out),
        _ => usage(),
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(1);
}

fn read_json(path: &str, what: &str) -> serde_json::Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|_| {
        fail(&format!("ala analyze: missing {what} at {path}"));
    });
    serde_json::from_str(&text).unwrap_or_else(|e| {
        fail(&format!("ala analyze: invalid {what} at {path}: {e}"));
    })
}

fn refuse_overwrite(out: &str, stage: &str) {
    if std::path::Path::new(out).exists() {
        fail(&format!(
            "ala analyze {stage}: refusing to overwrite existing {out}"
        ));
    }
}

/// Stable dyad identity: the directory's own name, so locks do not depend on
/// the path the caller happened to pass.
fn dyad_id(dir: &str) -> String {
    std::path::Path::new(dir)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| fail(&format!("ala analyze: unusable --dyad path {dir:?}")))
        .to_string()
}

fn predictor_id_string(id: ald_analysis::PredictorId) -> String {
    serde_json::to_value(id)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_else(|| fail("ala analyze: cannot render predictor id"))
}

fn read_role_folds(dir: &str, role: &str) -> ald_analysis::dyad::RoleFolds {
    let path = format!("{dir}/role-folds-{role}.json");
    let v = read_json(&path, &format!("role folds for {role}"));
    let folds: ald_analysis::dyad::RoleFolds = serde_json::from_value(v).unwrap_or_else(|e| {
        fail(&format!("ala analyze: invalid role folds at {path}: {e}"));
    });
    if folds.role != role {
        fail(&format!(
            "ala analyze: {path} carries role {} folds",
            folds.role
        ));
    }
    folds
}

fn read_branch_probabilities(dir: &str, role: &str) -> Vec<ald_analysis::CaseProbabilities> {
    let path = format!("{dir}/branch-probabilities-{role}.json");
    let v = read_json(&path, &format!("branch probabilities for {role}"));
    serde_json::from_value(v).unwrap_or_else(|e| {
        fail(&format!(
            "ala analyze: invalid branch probabilities at {path}: {e}"
        ));
    })
}

/// Validation-fold predictor selection per role: the ordinary predictor with
/// the lowest realized Brier on validation-selection wins (1e-12 tie
/// tolerance, predictor list order), refit on all permitted folds. Each
/// record's hash is printed for lock-in.
fn cmd_select(dyads: &[String], cohort: &str, out: &str) {
    refuse_overwrite(out, "select");
    let inventory = inventory();
    let mut records = Vec::new();
    for d in dyads {
        let id = dyad_id(d);
        for role in ald_analysis::ROLES {
            let folds = read_role_folds(d, role);
            let sel = ald_analysis::select_ordinary_predictor(
                &inventory,
                &folds.training,
                &folds.validation_fit,
                &folds.validation_selection,
            )
            .unwrap_or_else(|e| {
                fail(&format!("ala analyze select: {id}/{role}: {e}"));
            });
            let mut briers = serde_json::Map::new();
            for (pid, b) in &sel.validation_brier_by_predictor {
                briers.insert(predictor_id_string(*pid), serde_json::json!(*b));
            }
            let mut record = serde_json::Map::new();
            record.insert("dyad".to_string(), serde_json::json!(id));
            record.insert("role".to_string(), serde_json::json!(role));
            record.insert(
                "selectedPredictorId".to_string(),
                serde_json::json!(predictor_id_string(sel.selected)),
            );
            record.insert(
                "validationBrierByPredictor".to_string(),
                serde_json::Value::Object(briers),
            );
            record.insert(
                "trainingCases".to_string(),
                serde_json::json!(folds.training.len()),
            );
            record.insert(
                "validationFitCases".to_string(),
                serde_json::json!(folds.validation_fit.len()),
            );
            record.insert(
                "validationSelectionCases".to_string(),
                serde_json::json!(folds.validation_selection.len()),
            );
            record.insert("testCases".to_string(), serde_json::json!(folds.test.len()));
            let h = hash_canonical(
                "ala-lv-selection-v1",
                &serde_json::Value::Object(record.clone()),
            )
            .unwrap_or_else(|e| fail(&format!("ala analyze select: hashing failed: {e}")));
            println!("LOCK {id} {role} {} {h}", predictor_id_string(sel.selected));
            record.insert("recordSha256".to_string(), serde_json::json!(h));
            records.push(serde_json::Value::Object(record));
        }
    }
    let doc = serde_json::json!({"stage": "select", "cohort": cohort, "records": records});
    std::fs::write(out, serde_json::to_string_pretty(&doc).unwrap())
        .unwrap_or_else(|e| fail(&format!("ala analyze select: cannot write {out}: {e}")));
    println!("SELECT OK: {} record(s) -> {out}", records.len());
}

/// Cohort scoring: per-dyad primaries via `analyze_dyad`, paired-Brier
/// control contrasts, simultaneous bounds, invalidity bounds, fidelity, and
/// the cohort t-interval plus bootstrap sensitivity interval. The locked
/// selection is re-verified role by role: scoring refuses to proceed when the
/// recomputed winner disagrees with the locked record.
fn cmd_score(args: &[String], dyads: &[String], cohort: &str, out: &str) {
    refuse_overwrite(out, "score");
    let selection_path = arg(args, "--selection").unwrap_or_else(|| usage());
    let coverage: f64 = arg(args, "--coverage")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.975);
    let seed = arg(args, "--bootstrap-seed").unwrap_or_else(|| usage());
    let resamples: usize = arg(args, "--bootstrap-resamples")
        .and_then(|v| v.parse().ok())
        .unwrap_or(10_000);
    let attempted: usize = arg(args, "--attempted")
        .and_then(|v| v.parse().ok())
        .unwrap_or(dyads.len());

    let selection = read_json(&selection_path, "selection");
    if selection.get("stage").and_then(|v| v.as_str()) != Some("select") {
        fail(&format!(
            "ala analyze score: {selection_path} is not a select record"
        ));
    }
    if selection.get("cohort").and_then(|v| v.as_str()) != Some(cohort) {
        fail(&format!(
            "ala analyze score: selection cohort does not match {cohort}"
        ));
    }
    let records = selection
        .get("records")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| {
            fail(&format!(
                "ala analyze score: {selection_path} has no records"
            ))
        });
    let locked = |dyad: &str, role: &str| -> &serde_json::Value {
        records
            .iter()
            .find(|r| {
                r.get("dyad").and_then(|v| v.as_str()) == Some(dyad)
                    && r.get("role").and_then(|v| v.as_str()) == Some(role)
            })
            .unwrap_or_else(|| {
                fail(&format!(
                    "ala analyze score: no locked selection for {dyad}/{role}"
                ));
            })
    };

    let mut seen = std::collections::HashSet::new();
    for d in dyads {
        if !seen.insert(dyad_id(d)) {
            fail(&format!(
                "ala analyze score: duplicate dyad directory name {d}"
            ));
        }
    }

    let inventory = inventory();
    let mut primaries = Vec::with_capacity(dyads.len());
    let mut per_contrast_brier: std::collections::BTreeMap<String, Vec<f64>> =
        ald_analysis::CONTRAST_IDS
            .iter()
            .map(|c| (c.to_string(), Vec::new()))
            .collect();
    let mut per_contrast_success: std::collections::BTreeMap<String, Vec<f64>> =
        ald_analysis::CONTRAST_IDS
            .iter()
            .map(|c| (c.to_string(), Vec::new()))
            .collect();
    let mut dyad_docs = Vec::with_capacity(dyads.len());

    for d in dyads {
        let id = dyad_id(d);
        let folds: Vec<ald_analysis::dyad::RoleFolds> = ald_analysis::ROLES
            .iter()
            .map(|r| read_role_folds(d, r))
            .collect();
        let result = ald_analysis::analyze_dyad(&id, &folds, &inventory)
            .unwrap_or_else(|e| fail(&format!("ala analyze score: {id}: {e}")));
        for role_result in &result.roles {
            let rec = locked(&id, &role_result.role);
            let want = rec
                .get("selectedPredictorId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let got = predictor_id_string(role_result.selected_predictor_id);
            if want != got {
                fail(&format!(
                    "ala analyze score: {id}/{} selection mismatch: locked {want}, recomputed {got}",
                    role_result.role
                ));
            }
        }
        let mut cases = Vec::new();
        let mut native_all = Vec::new();
        let mut replay_all = Vec::new();
        for (role, f) in ald_analysis::ROLES.iter().zip(folds.iter()) {
            let probs = read_branch_probabilities(d, role);
            cases.extend(
                ald_analysis::branch_cases(&id, role, &probs)
                    .unwrap_or_else(|e| fail(&format!("ala analyze score: {id}: {e}"))),
            );
            for row in &f.test {
                native_all.push(row.native_distribution.clone());
                replay_all.push(row.replay_distribution.clone());
            }
        }
        let brier_contrasts = ald_analysis::brier_control_contrasts(&id, &cases)
            .unwrap_or_else(|e| fail(&format!("ala analyze score: {id}: {e}")));
        let success_contrasts = ald_analysis::success_control_contrasts(&id, &cases)
            .unwrap_or_else(|e| fail(&format!("ala analyze score: {id}: {e}")));
        let fid = ald_analysis::fidelity(&native_all, &replay_all)
            .unwrap_or_else(|e| fail(&format!("ala analyze score: {id}: {e}")));
        for (k, v) in &brier_contrasts {
            per_contrast_brier.get_mut(k).unwrap().push(*v);
        }
        for (k, v) in &success_contrasts {
            per_contrast_success.get_mut(k).unwrap().push(*v);
        }
        primaries.push(result.primary);
        dyad_docs.push(serde_json::json!({
            "dyad": id,
            "primary": result.primary,
            "roles": result.roles,
            "brierContrasts": brier_contrasts,
            "successContrasts": success_contrasts,
            "fidelity": fid,
        }));
    }

    let iv = ald_analysis::dyad::cohort_interval(cohort, &primaries, coverage)
        .unwrap_or_else(|e| fail(&format!("ala analyze score: interval: {e}")));
    let bs = ald_analysis::bootstrap_mean_ci(&primaries, &seed, resamples, 0.95)
        .unwrap_or_else(|e| fail(&format!("ala analyze score: bootstrap: {e}")));
    let mut bounds = serde_json::Map::new();
    let mut invalid_brier = serde_json::Map::new();
    let mut invalid_success = serde_json::Map::new();
    for cid in ald_analysis::CONTRAST_IDS {
        let vals = &per_contrast_brier[cid];
        let b = ald_analysis::simultaneous_bounds(cid, vals)
            .unwrap_or_else(|e| fail(&format!("ala analyze score: bounds: {e}")));
        bounds.insert(cid.to_string(), serde_json::to_value(&b).unwrap());
        invalid_brier.insert(
            cid.to_string(),
            serde_json::to_value(ald_analysis::invalidity_bounds(
                vals,
                attempted,
                ald_analysis::PRIMARY_ADMISSIBLE,
            ))
            .unwrap(),
        );
        invalid_success.insert(
            cid.to_string(),
            serde_json::to_value(ald_analysis::invalidity_bounds(
                &per_contrast_success[cid],
                attempted,
                ald_analysis::SUCCESS_CONTRAST_ADMISSIBLE,
            ))
            .unwrap(),
        );
    }
    let mut doc = serde_json::Map::new();
    doc.insert("stage".to_string(), serde_json::json!("score"));
    doc.insert("cohort".to_string(), serde_json::json!(cohort));
    doc.insert("selection".to_string(), serde_json::json!(selection_path));
    doc.insert("coverage".to_string(), serde_json::json!(coverage));
    doc.insert("bootstrapSeed".to_string(), serde_json::json!(seed));
    doc.insert(
        "bootstrapResamples".to_string(),
        serde_json::json!(resamples),
    );
    doc.insert("n".to_string(), serde_json::json!(primaries.len()));
    doc.insert("attempted".to_string(), serde_json::json!(attempted));
    doc.insert("dyads".to_string(), serde_json::Value::Array(dyad_docs));
    doc.insert("interval".to_string(), serde_json::to_value(&iv).unwrap());
    doc.insert("bootstrap".to_string(), serde_json::to_value(&bs).unwrap());
    doc.insert(
        "controlBounds".to_string(),
        serde_json::Value::Object(bounds),
    );
    doc.insert(
        "invalidityBounds".to_string(),
        serde_json::json!({
            "primary": ald_analysis::invalidity_bounds(
                &primaries, attempted, ald_analysis::PRIMARY_ADMISSIBLE
            ),
            "brierContrasts": invalid_brier,
            "successContrasts": invalid_success,
        }),
    );
    let h = hash_canonical("ala-lv-score-v1", &serde_json::Value::Object(doc.clone()))
        .unwrap_or_else(|e| fail(&format!("ala analyze score: hashing failed: {e}")));
    doc.insert("docSha256".to_string(), serde_json::json!(h));
    std::fs::write(out, serde_json::to_string_pretty(&doc).unwrap())
        .unwrap_or_else(|e| fail(&format!("ala analyze score: cannot write {out}: {e}")));
    println!("SCORE OK: n={} -> {out}", primaries.len());
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
    let v: u32 = arg(args, "--protocol-version")
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    let doc = serde_json::json!({
        "protocolVersion": v,
        "manifest": format!("protocols/ala-lv-execution-manifest.v{v}.json"),
    });
    let text = serde_json::to_string(&doc).unwrap();
    let canonical =
        canonical_json(&serde_json::from_str::<serde_json::Value>(&text).unwrap()).unwrap();
    println!("{canonical}");
    let _ = PathBuf::from(".");
}

fn cmd_journal(args: &[String]) {
    // Append one JSON line to a journal file.
    let path = arg(args, "--journal").unwrap_or_else(|| usage());
    let line = arg(args, "--line").unwrap_or_else(|| usage());
    use std::io::Write as _;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{line}").unwrap();
}
