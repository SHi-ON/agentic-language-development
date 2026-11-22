//! `ala` — the single ALD command-line interface.
//!
//! Single entry point for every ALA operation: `ala run`,
//! `ala analyze select|score`, `ala verify`, `ala manifest`.

use ald_hashing::{canonical_json, hash_canonical};

fn usage() -> ! {
    eprintln!("usage: ala <run|analyze|verify|manifest|journal> [args...]");
    eprintln!("  ala run --manifest <path> --cohort <C> --kind <K> --index <N> --evidence-root <dir> [--treatment-batch-version N] [--protocol-version N]");
    eprintln!("        [--training-interactions-per-direction N] [--learning-mode joint|none|receiver-only]");
    eprintln!("        [--drop-probability NUM/DEN] [--active-sender-tokens 8|16|32] [--held-out-types 12,13,14,15]");
    eprintln!("  ala analyze select|score --dyad <dir>... --cohort <C> --out <path> [--selection <path>] [--coverage F] [--bootstrap-seed S] [--bootstrap-resamples N] [--attempted N]");
    eprintln!("  ala verify <evidence.sqlite>");
    eprintln!("  ala manifest --protocol-version <1|2>");
    std::process::exit(2);
}

fn arg(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

/// Fail closed on malformed numbers: exit 2 naming the flag and the
/// offending value (same UX as `parse_development_options`).
fn bad_number(what: &str, name: &str, value: &str, expected: &str) -> ! {
    eprintln!("ala {what}: invalid value {value:?} for {name}: expected {expected}");
    usage();
}

fn arg_u32_strict(args: &[String], what: &str, name: &str, default: u32) -> u32 {
    match arg(args, name) {
        None => default,
        Some(v) => v
            .parse()
            .unwrap_or_else(|_| bad_number(what, name, &v, "u32")),
    }
}

fn arg_usize_strict(args: &[String], what: &str, name: &str, default: usize) -> usize {
    match arg(args, name) {
        None => default,
        Some(v) => v
            .parse()
            .unwrap_or_else(|_| bad_number(what, name, &v, "usize")),
    }
}

fn arg_f64_strict(args: &[String], what: &str, name: &str, default: f64) -> f64 {
    match arg(args, name) {
        None => default,
        Some(v) => v
            .parse()
            .unwrap_or_else(|_| bad_number(what, name, &v, "f64")),
    }
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
    const RUN_FLAGS: &[&str] = &[
        "--manifest",
        "--cohort",
        "--kind",
        "--index",
        "--evidence-root",
        "--treatment-batch-version",
        "--protocol-version",
        "--training-interactions-per-direction",
        "--learning-mode",
        "--drop-probability",
        "--active-sender-tokens",
        "--held-out-types",
    ];
    reject_stray_args(args, "run", RUN_FLAGS);
    reject_duplicate_flags(args, "run", RUN_FLAGS);
    let development = parse_development_options(args);
    let index: u32 = match arg(args, "--index") {
        None => usage(),
        Some(v) => v
            .parse()
            .unwrap_or_else(|_| bad_number("run", "--index", &v, "u32")),
    };
    let protocol_version = arg_u32_strict(args, "run", "--protocol-version", 2);
    if protocol_version != 1 && protocol_version != 2 {
        eprintln!(
            "ala run: invalid value {protocol_version} for --protocol-version: expected 1 or 2"
        );
        usage();
    }
    let req = ald_orchestrator::DyadRequest {
        manifest_path: arg(args, "--manifest").unwrap_or_else(|| usage()),
        cohort: arg(args, "--cohort").unwrap_or_else(|| usage()),
        kind: arg(args, "--kind").unwrap_or_else(|| usage()),
        index,
        evidence_root: arg(args, "--evidence-root").unwrap_or_else(|| usage()),
        treatment_batch_version: arg_u32_strict(args, "run", "--treatment-batch-version", 2),
        protocol_version,
        development,
    };
    match ald_orchestrator::run_dyad(&req, &inventory()) {
        Ok(receipt) => {
            // INVARIANT: serializing the receipt struct cannot fail
            // (serde_json only errors on maps with non-string keys).
            println!("{}", serde_json::to_string(&receipt).unwrap());
        }
        Err(e) => {
            eprintln!("ala run failed: {e}");
            std::process::exit(1);
        }
    }
}

/// Parse the development flags into requested [`ald_orchestrator::DevelopmentOptions`].
/// Returns `None` when no development flag was passed. Malformed values exit
/// 2 with a naming message; nothing falls back to a default silently.
fn parse_development_options(args: &[String]) -> Option<ald_orchestrator::DevelopmentOptions> {
    let any = [
        "--training-interactions-per-direction",
        "--learning-mode",
        "--drop-probability",
        "--active-sender-tokens",
        "--held-out-types",
    ]
    .iter()
    .any(|f| arg(args, f).is_some());
    if !any {
        return None;
    }
    let bad = |e: ald_orchestrator::OrchestratorError| -> ! {
        eprintln!("ala run: {e}");
        std::process::exit(2);
    };
    Some(ald_orchestrator::DevelopmentOptions {
        training_interactions_per_direction: arg(args, "--training-interactions-per-direction")
            .map(|v| ald_orchestrator::parse_training_interactions(&v).unwrap_or_else(|e| bad(e))),
        learning_mode: arg(args, "--learning-mode")
            .map(|v| ald_orchestrator::parse_learning_mode(&v).unwrap_or_else(|e| bad(e))),
        drop_probability: arg(args, "--drop-probability")
            .map(|v| ald_orchestrator::parse_drop_probability(&v).unwrap_or_else(|e| bad(e))),
        active_sender_tokens: arg(args, "--active-sender-tokens")
            .map(|v| ald_orchestrator::parse_active_sender_tokens(&v).unwrap_or_else(|e| bad(e))),
        held_out_types: arg(args, "--held-out-types")
            .map(|v| ald_orchestrator::parse_held_out_types(&v).unwrap_or_else(|e| bad(e))),
    })
}

/// Reject a repeated scalar flag: a second `--drop-probability` (or any
/// other repeated flag) is almost certainly a copy-paste slip, and silently
/// taking the first value would run the wrong ablation with a zero exit code.
fn reject_duplicate_flags(args: &[String], what: &str, flags: &[&str]) {
    let mut seen = std::collections::HashSet::new();
    let mut i = 0;
    while i < args.len() {
        if flags.contains(&args[i].as_str()) {
            if !seen.insert(args[i].clone()) {
                eprintln!("ala {what}: duplicate flag {:?}", args[i]);
                std::process::exit(2);
            }
            i += 2; // flag + value; a missing value surfaces as usage() below
        } else {
            i += 1; // strays are reported by reject_stray_args
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
    // INVARIANT: serializing a String-keyed JSON value cannot fail.
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
    // Parse numbers BEFORE the overwrite guard so a malformed flag reports
    // exit 2 even when the out-path already exists.
    let coverage = arg_f64_strict(args, "analyze score", "--coverage", 0.975);
    let resamples = arg_usize_strict(args, "analyze score", "--bootstrap-resamples", 10_000);
    let attempted = arg_usize_strict(args, "analyze score", "--attempted", dyads.len());
    refuse_overwrite(out, "score");
    let selection_path = arg(args, "--selection").unwrap_or_else(|| usage());
    let seed = arg(args, "--bootstrap-seed").unwrap_or_else(|| usage());

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
        for (name, map) in [("Brier", &brier_contrasts), ("success", &success_contrasts)] {
            for k in map.keys() {
                if !ald_analysis::CONTRAST_IDS.contains(&k.as_str()) {
                    fail(&format!(
                        "ala analyze score: {id}: analysis returned unknown {name} contrast {k}"
                    ));
                }
            }
        }
        for cid in ald_analysis::CONTRAST_IDS {
            let b = brier_contrasts.get(cid).unwrap_or_else(|| {
                fail(&format!(
                    "ala analyze score: {id}: analysis returned no Brier contrast for {cid}"
                ));
            });
            per_contrast_brier
                .get_mut(cid)
                .unwrap_or_else(|| {
                    fail(&format!(
                        "ala analyze score: {id}: analysis returned unknown contrast {cid}"
                    ));
                })
                .push(*b);
            let s = success_contrasts.get(cid).unwrap_or_else(|| {
                fail(&format!(
                    "ala analyze score: {id}: analysis returned no success contrast for {cid}"
                ));
            });
            per_contrast_success
                .get_mut(cid)
                .unwrap_or_else(|| {
                    fail(&format!(
                        "ala analyze score: {id}: analysis returned unknown contrast {cid}"
                    ));
                })
                .push(*s);
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
        // INVARIANT (covers all three to_value calls in this loop):
        // serializing these plain structs cannot fail (serde_json only
        // errors on maps with non-string keys; the analysis errors are
        // already mapped to fail() above).
        bounds.insert(cid.to_string(), serde_json::to_value(&b).unwrap());
        let ib = ald_analysis::invalidity_bounds(vals, attempted, ald_analysis::PRIMARY_ADMISSIBLE)
            .unwrap_or_else(|e| fail(&format!("ala analyze score: invalidity bounds: {e}")));
        invalid_brier.insert(cid.to_string(), serde_json::to_value(&ib).unwrap());
        let is = ald_analysis::invalidity_bounds(
            &per_contrast_success[cid],
            attempted,
            ald_analysis::SUCCESS_CONTRAST_ADMISSIBLE,
        )
        .unwrap_or_else(|e| fail(&format!("ala analyze score: invalidity bounds: {e}")));
        invalid_success.insert(cid.to_string(), serde_json::to_value(&is).unwrap());
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
    // INVARIANT: serializing these plain structs cannot fail.
    doc.insert("interval".to_string(), serde_json::to_value(&iv).unwrap());
    doc.insert("bootstrap".to_string(), serde_json::to_value(&bs).unwrap());
    doc.insert(
        "controlBounds".to_string(),
        serde_json::Value::Object(bounds),
    );
    let primary_bounds =
        ald_analysis::invalidity_bounds(&primaries, attempted, ald_analysis::PRIMARY_ADMISSIBLE)
            .unwrap_or_else(|e| {
                fail(&format!(
                    "ala analyze score: primary invalidity bounds: {e}"
                ));
            });
    doc.insert(
        "invalidityBounds".to_string(),
        serde_json::json!({
            "primary": primary_bounds,
            "brierContrasts": invalid_brier,
            "successContrasts": invalid_success,
        }),
    );
    let h = hash_canonical("ala-lv-score-v1", &serde_json::Value::Object(doc.clone()))
        .unwrap_or_else(|e| fail(&format!("ala analyze score: hashing failed: {e}")));
    doc.insert("docSha256".to_string(), serde_json::json!(h));
    // INVARIANT: serializing a String-keyed JSON value cannot fail.
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
            // INVARIANT: serializing the verify report struct cannot fail.
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

/// Compile-time copy of the frozen protocol files, so `ala manifest`
/// works from any working directory. The live `protocols/` file is
/// preferred when present (see `cmd_manifest`).
fn embedded_manifest(v: u32) -> &'static str {
    match v {
        1 => include_str!("../../../protocols/ala-lv-execution-manifest.v1.json"),
        _ => include_str!("../../../protocols/ala-lv-execution-manifest.v2.json"),
    }
}

fn cmd_manifest(args: &[String]) {
    let v = arg_u32_strict(args, "manifest", "--protocol-version", 2);
    if v != 1 && v != 2 {
        eprintln!("ala manifest: invalid value {v} for --protocol-version: expected 1 or 2");
        usage();
    }
    let rel = format!("protocols/ala-lv-execution-manifest.v{v}.json");
    // Prefer the live file when run from the repo root (corruption on disk
    // must fail); otherwise validate the shipped bytes embedded at build.
    let text = match std::fs::read_to_string(&rel) {
        Ok(t) => t,
        Err(_) => embedded_manifest(v).to_string(),
    };
    let proto: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| {
        eprintln!("ala manifest: protocol file {rel} is not valid JSON: {e}");
        std::process::exit(2);
    });
    // LIMIT (P0-FORK-02): existence + parse + studyId only. Both frozen
    // files declare manifestVersion ala-lv-execution-manifest/v1, so no
    // version-string-vs-v consistency check is possible here; that belongs
    // to P2-CLEAN-FORK-04 (do not "fix" the frozen bytes in M1).
    if proto.get("studyId").and_then(|s| s.as_str()) != Some("ALA-LV") {
        eprintln!("ala manifest: protocol file {rel} is not an ALA-LV manifest");
        std::process::exit(2);
    }
    let doc = serde_json::json!({
        "protocolVersion": v,
        "manifest": rel,
    });
    // INVARIANT: serializing a String-keyed JSON value cannot fail; the
    // from_str above parses what to_string just produced.
    let text = serde_json::to_string(&doc).unwrap();
    let canonical =
        canonical_json(&serde_json::from_str::<serde_json::Value>(&text).unwrap()).unwrap();
    println!("{canonical}");
}

fn cmd_journal(args: &[String]) {
    // Append one hash-chained entry to a journal file. The --line body is a
    // JSON object WITHOUT chain fields; seq/previousHash/entryHash are
    // assigned by the verified append, so output is verifiable by the same
    // canonical/hash rules run_dyad uses.
    let path = arg(args, "--journal").unwrap_or_else(|| usage());
    let line = arg(args, "--line").unwrap_or_else(|| usage());
    let body: serde_json::Value = serde_json::from_str(&line).unwrap_or_else(|e| {
        eprintln!("ala journal: --line must be a JSON object: {e}");
        std::process::exit(2);
    });
    let map = body.as_object().unwrap_or_else(|| {
        eprintln!("ala journal: --line must be a JSON object, not an array or scalar");
        std::process::exit(2);
    });
    for f in ["seq", "previousHash", "entryHash"] {
        if map.contains_key(f) {
            eprintln!("ala journal: --line must not carry chain field {f:?} (assigned on append)");
            std::process::exit(2);
        }
    }
    match ald_orchestrator::journal_append_entry(&path, map.clone()) {
        Ok(hash) => println!("{hash}"),
        Err(e) => {
            eprintln!("ala journal: cannot append to {path}: {e}");
            std::process::exit(1);
        }
    }
}
