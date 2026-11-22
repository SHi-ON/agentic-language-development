//! The dyad runner: one full deterministic dyad.

#![allow(clippy::cast_precision_loss)] // small counts/indices; exact for realistic inputs
#![allow(clippy::cast_possible_truncation)] // small counts/indices; bounded by construction
#![allow(clippy::cast_sign_loss)] // non-negative by construction (validated ranges, draws in [0, 1))

use crate::{
    dev_options::{
        development_config_hash, development_config_short_id, drop_draw_less_than,
        resolve_development_options, resolved_is_default, DevelopmentOptions, LearningMode,
        ResolvedDevelopment, DEFAULT_ACTIVE_SENDER_TOKENS, DEVELOPMENT_COHORT,
    },
    journal::journal_append,
    manifest::{
        check_admission, execution_counts_from_manifest, manifest_sha256_hex, DyadReceipt,
        DyadRequest,
    },
    run_support::{
        candidate_types, exposure_histogram, journal_slot_identity, persistable_role_folds,
        stable_f64, valid_distribution, BranchOutcome, CaseProbabilities, DyadInner, RoleBundle,
        RoleExposure,
    },
    seeds::{derive_ala_seed, AlaSeedInput, ALA_SEED_ROOT, ALA_SEED_ROOT_V2},
    workload::{prototype_workload, CollectionWorkload, StudyCounts},
    OrchestratorError,
};

/// Pretty JSON for persisted artifacts. Serialization of these shapes is
/// infallible in practice, but the error is typed rather than unwrapped so
/// no artifact write can panic.
fn to_pretty<T: serde::Serialize>(v: &T) -> Result<String, OrchestratorError> {
    serde_json::to_string_pretty(v).map_err(|e| OrchestratorError::Domain(e.to_string()))
}

/// Execute one full ALA dyad: validate the execution manifest and P5
/// admission, journal the attempt, generate LV01 cases per role from the
/// manifest workload, symmetrically co-train two
/// [`ald_learners::AgentPolicy`] agents unsupervised (alternating
/// sender/receiver per step, receiver samples its candidate position and
/// never sees the true type, shared episode reward, 1/8 token-drop
/// channel, all randomness from derived seed streams), freeze the
/// policies at test (tokens from the opposite agent as sender, native
/// predictions from the role agent's receiver position posteriors), score
/// every test case with emergent-native predictions and exact paired-draw
/// replay, evaluate every branch via
/// [`ald_interventions::substitute_branch`], persist per-role branch
/// probabilities and role folds (`role-folds-{agent-0,agent-1}.json`, the
/// exact input schema `ala analyze select|score` reads) plus the frozen
/// full agent policies (`agent-policy-{agent-0,agent-1}.json`), and
/// evidence-stream events, journal the terminal line, and return an
/// honest receipt with measured cost.
///
/// The evidence layout (slot dirs, receipt, root journal, admission record)
/// matches the frozen v2 layout so existing post-check tooling keeps
/// working during migration.
///
/// Development options ride along in `req.development`: they are resolved
/// and gated BEFORE any directory, journal, or database is created, so a
/// rejected run leaves no partial artifacts. Resolved-default runs take the
/// legacy path byte-identically; non-default runs require
/// `--cohort development`, write to a config-suffixed run root, refuse to
/// overwrite an existing root, and bind the config hash into the receipt,
/// the journal lines, and a `development-config.json` sidecar.
///
/// # Errors
///
/// Returns an error for invalid manifests, inventories, or admission, for IO failures, or for non-finite results; rejected runs leave no partial artifacts.
#[allow(clippy::too_many_lines)] // straight-line run pipeline; P0-FORK-11 splits modules, not this function.
pub fn run_dyad(req: &DyadRequest, inventory: &[String]) -> Result<DyadReceipt, OrchestratorError> {
    let started = std::time::Instant::now();
    if req.cohort.is_empty() || req.kind.is_empty() || req.evidence_root.is_empty() {
        return Err(OrchestratorError::Domain(
            "ALA dyad: cohort, kind, and evidenceRoot must be non-empty".to_string(),
        ));
    }
    if req.index > 9999 {
        return Err(OrchestratorError::Domain(
            "ALA dyad: index must fit a zero-padded 4-digit slot".to_string(),
        ));
    }
    let distinct = {
        let mut s = std::collections::HashSet::new();
        inventory.iter().all(|t| s.insert(t))
    };
    if inventory.len() != 32 || !distinct {
        return Err(OrchestratorError::Domain(
            "ALA dyad: LV01 requires 32 distinct inventory tokens".to_string(),
        ));
    }
    let pad = format!("{:04}", req.index);

    // Gate 1: execution manifest (identity + workload).
    let manifest_raw = std::fs::read(&req.manifest_path)
        .map_err(|e| OrchestratorError::Io(format!("manifest unreadable: {e}")))?;
    let manifest_text = String::from_utf8(manifest_raw.clone())
        .map_err(|e| OrchestratorError::Domain(format!("ALA manifest: not UTF-8: {e}")))?;
    let counts = execution_counts_from_manifest(&manifest_text)?;
    let workload = prototype_workload(&counts)?;
    let manifest_sha256 = manifest_sha256_hex(&manifest_raw);

    // Gate 1b: development options (resolve + cohort gate). Runs before any
    // directory, journal, or database exists, so rejections are artifact-free.
    let manifest_per_role_training = counts.training_cases / 2;
    let requested = req.development.clone().unwrap_or_default();
    let resolved = resolve_development_options(&requested, manifest_per_role_training)?;
    let dev_active = !resolved_is_default(&resolved, manifest_per_role_training);
    let dev_hash: Option<String> = if dev_active {
        Some(development_config_hash(&resolved)?)
    } else {
        None
    };
    if dev_active && req.cohort != DEVELOPMENT_COHORT {
        return Err(OrchestratorError::Domain(format!(
            "development options: non-default options require --cohort {DEVELOPMENT_COHORT}, got {:?}",
            req.cohort
        )));
    }

    // Gate 2: P5 admission for study cohorts.
    check_admission(&req.evidence_root, &req.cohort)?;

    // Non-default configs run in a separate config-suffixed root so different
    // ablations never share (or silently overwrite) one slot directory.
    let slot_dir = match &dev_hash {
        Some(h) => format!(
            "{}/{}/{}-{}-dev-{}",
            req.evidence_root,
            req.cohort,
            req.kind,
            pad,
            development_config_short_id(h)
        ),
        None => format!("{}/{}/{}-{}", req.evidence_root, req.cohort, req.kind, pad),
    };
    if dev_active && std::path::Path::new(&slot_dir).exists() {
        return Err(OrchestratorError::Domain(format!(
            "development options: refusing to overwrite existing run root {slot_dir}"
        )));
    }
    std::fs::create_dir_all(&slot_dir).map_err(|e| OrchestratorError::Io(e.to_string()))?;
    let db_path = format!("{slot_dir}/evidence.sqlite");
    let conn = ald_evidence::open(&db_path)?;
    let journal_path = format!("{}/journal.jsonl", req.evidence_root);

    // Journal the attempt before doing work.
    let mut attempted = journal_slot_identity(req, &pad, &manifest_sha256);
    attempted.insert("kind".to_string(), serde_json::json!("attempted"));
    attempted.insert(
        "protocolVersion".to_string(),
        serde_json::json!(req.protocol_version),
    );
    attempted.insert(
        "treatmentBatchVersion".to_string(),
        serde_json::json!(req.treatment_batch_version),
    );
    if let Some(h) = &dev_hash {
        attempted.insert("developmentConfigSha256".to_string(), serde_json::json!(h));
    }
    let attempted_hash = journal_append(&journal_path, attempted)?;

    let inner = match run_dyad_inner(
        req,
        inventory,
        &counts,
        &workload,
        &manifest_sha256,
        &slot_dir,
        &conn,
        &pad,
        &requested,
        &resolved,
        dev_hash.as_ref(),
    ) {
        Ok(ok) => ok,
        Err(e) => {
            let mut terminal = journal_slot_identity(req, &pad, &manifest_sha256);
            terminal.insert("kind".to_string(), serde_json::json!("terminal"));
            terminal.insert("state".to_string(), serde_json::json!("invalid"));
            terminal.insert("error".to_string(), serde_json::json!(e.to_string()));
            if let Some(h) = &dev_hash {
                terminal.insert("developmentConfigSha256".to_string(), serde_json::json!(h));
            }
            let _ = journal_append(&journal_path, terminal);
            return Err(e);
        }
    };

    let mut terminal = journal_slot_identity(req, &pad, &manifest_sha256);
    terminal.insert("kind".to_string(), serde_json::json!("terminal"));
    terminal.insert("state".to_string(), serde_json::json!("valid"));
    terminal.insert(
        "casesAttempted".to_string(),
        serde_json::json!(inner.cases_attempted),
    );
    terminal.insert(
        "casesValid".to_string(),
        serde_json::json!(inner.cases_valid),
    );
    terminal.insert("primary".to_string(), serde_json::json!(inner.primary));
    if let Some(h) = &dev_hash {
        terminal.insert("developmentConfigSha256".to_string(), serde_json::json!(h));
    }
    let terminal_hash = journal_append(&journal_path, terminal)?;

    let wall_seconds = started.elapsed().as_secs_f64();
    let receipt = DyadReceipt {
        cohort: req.cohort.clone(),
        kind: req.kind.clone(),
        index: req.index,
        state: "valid".to_string(),
        cases_attempted: inner.cases_attempted,
        cases_valid: inner.cases_valid,
        manifest_sha256,
        protocol_version: req.protocol_version,
        treatment_batch_version: req.treatment_batch_version,
        primary: inner.primary,
        wall_seconds,
        attempted_hash,
        terminal_hash,
        development_config_sha256: dev_hash.clone(),
    };
    let body =
        serde_json::to_value(&receipt).map_err(|e| OrchestratorError::Domain(e.to_string()))?;
    std::fs::write(format!("{slot_dir}/dyad-receipt.json"), to_pretty(&body)?)
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;

    Ok(receipt)
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)] // straight-line dyad pipeline; P0-FORK-11 splits modules, not this function.
fn run_dyad_inner(
    req: &DyadRequest,
    inventory: &[String],
    counts: &StudyCounts,
    workload: &CollectionWorkload,
    manifest_sha256: &str,
    slot_dir: &str,
    conn: &rusqlite::Connection,
    pad: &str,
    requested: &DevelopmentOptions,
    resolved: &ResolvedDevelopment,
    dev_hash: Option<&String>,
) -> Result<DyadInner, OrchestratorError> {
    use ald_analysis::analyze_dyad;
    use ald_analysis::dyad::{RoleFolds, TestRow};
    use ald_analysis::ledger::OrdinaryRecord;
    use ald_learners::AgentPolicy;
    use ald_scenario::{generate_cases, generate_cases_excluding, plan_paired_case};

    let dev_active = dev_hash.is_some();
    let seed_root = if req.protocol_version >= 2 {
        ALA_SEED_ROOT_V2
    } else {
        ALA_SEED_ROOT
    };
    // Manifest totals are per dyad; each role generates half. `training_turns`
    // is the dyad total, so halve the study count the same way the workload
    // halves the validation partitions.
    let per_role_train = (counts.training_cases / 2) as usize;
    let per_role_fit = workload.validation_fit_cases as usize;
    let per_role_sel = workload.validation_selection_cases as usize;
    let per_role_test = workload.within_support_test_cases as usize;
    // Episodes per direction: the manifest count on legacy runs, the
    // `--training-interactions-per-direction` override otherwise. Training
    // records are generated as one deterministic `max(N, fold)` stream, so an
    // override below the fold trains on the fold's prefix and an override
    // above it extends the same stream with fresh draws (never duplication).
    let episodes_per_direction = usize::try_from(resolved.training_interactions_per_direction)
        .map_err(|_| {
            OrchestratorError::Domain(
                "development options: training interactions per direction is out of range"
                    .to_string(),
            )
        })?;
    let generated_train = episodes_per_direction.max(per_role_train);
    let channel_dropped = |draw: f64| {
        if dev_active {
            drop_draw_less_than(
                draw,
                resolved.drop_probability.numerator,
                resolved.drop_probability.denominator,
            )
        } else {
            draw < 1.0 / 8.0
        }
    };

    let mut audit_attempted = journal_slot_identity(req, pad, manifest_sha256);
    audit_attempted.insert("kind".to_string(), serde_json::json!("attempted"));
    ald_evidence::append_event(conn, ald_types::EventStream::Audit, audit_attempted)?;

    // Phase 1: generate every role's folds from the manifest workload.
    // Training records carry the target types + candidate codes the
    // emergent episodes are built from. Non-default runs exclude held-out
    // types from every pre-test fold in both directions; test folds always
    // cover all 16 types.
    let mut bundles = Vec::new();
    for role in ["agent-0", "agent-1"] {
        let mk = |part: &str| AlaSeedInput {
            cohort: req.cohort.clone(),
            slot_kind: req.kind.clone(),
            slot_index: pad.to_string(),
            purpose: "scenario".to_string(),
            role: format!("{role}-receiver"),
            partition: Some(part.to_string()),
            case_id: None,
            branch: None,
            seed_root: Some(seed_root.to_string()),
        };
        let train_seed = derive_ala_seed(&mk("training"))?;
        let fit_seed = derive_ala_seed(&mk("validation-fit"))?;
        let sel_seed = derive_ala_seed(&mk("validation-selection"))?;
        let test_seed = derive_ala_seed(&mk("within-support-test"))?;

        let to_records = |seed: &str, n: usize, off: u64| -> Vec<OrdinaryRecord> {
            let cases = if dev_active {
                generate_cases_excluding(seed, off, role, n, inventory, &resolved.held_out_types)
            } else {
                generate_cases(seed, off, role, n, inventory)
            };
            cases
                .into_iter()
                .map(|c| OrdinaryRecord {
                    case_id: c.case_id,
                    receiver_role: c.receiver_role,
                    delivered_token: c.delivered_token,
                    candidate_type_codes: c.candidate_type_codes.to_vec(),
                    actual_selected_candidate_index: c.target_index,
                })
                .collect()
        };
        let training = to_records(&train_seed, generated_train, 0);
        let validation_fit = to_records(&fit_seed, per_role_fit, 1);
        let validation_selection = to_records(&sel_seed, per_role_sel, 2);
        let test_cases = generate_cases(&test_seed, 3, role, per_role_test, inventory);
        bundles.push(RoleBundle {
            role: role.to_string(),
            train_seed,
            fit_seed,
            sel_seed,
            test_seed,
            training,
            validation_fit,
            validation_selection,
            test_cases,
        });
    }

    // Phase 2: symmetric unsupervised co-training — two agents alternate
    // sender/receiver, one episode per direction per step over the
    // receiver role's training folds. Mirrors probe v4 exactly: the
    // sender agent samples a token for the true target type, the 1/8
    // channel may drop it, the receiver agent SAMPLES a candidate
    // position from its posterior (it never sees the true type), both
    // share the episode reward, the sender REINFORCEs its sampled token
    // (even when dropped), and the receiver REINFORCEs toward the chosen
    // candidate's type only (a dropped token is a receiver no-op). Every
    // draw comes from per-role derived seed streams, so training is
    // bit-deterministic per dyad. Development runs may constrain the sender
    // width, replace the channel probability, and freeze one or both sides
    // (`--learning-mode` gates ONLY the update calls: sampling draws are
    // consumed identically in every mode).
    let mut agents = if resolved.active_sender_tokens == DEFAULT_ACTIVE_SENDER_TOKENS {
        [AgentPolicy::new(), AgentPolicy::new()]
    } else {
        [
            AgentPolicy::with_active_sender_tokens(resolved.active_sender_tokens as usize)
                .map_err(|e| OrchestratorError::Domain(e.to_string()))?,
            AgentPolicy::with_active_sender_tokens(resolved.active_sender_tokens as usize)
                .map_err(|e| OrchestratorError::Domain(e.to_string()))?,
        ]
    };
    let mut train_rngs: Vec<ald_hashing::SeededPrng> = bundles
        .iter()
        .map(|b| ald_hashing::SeededPrng::new(&b.train_seed).derive("emergent-train"))
        .collect();
    for step in 0..episodes_per_direction {
        for receiver_idx in 0..bundles.len() {
            let sender_idx = 1 - receiver_idx;
            let rec = &bundles[receiver_idx].training[step];
            let cands = candidate_types(&rec.candidate_type_codes);
            let target_type = cands[rec.actual_selected_candidate_index];
            let rng = &mut train_rngs[receiver_idx];
            let token = agents[sender_idx].sample_token(target_type, rng)?;
            let channel_draw = rng.next_float();
            let delivered = if channel_dropped(channel_draw) {
                None
            } else {
                Some(token)
            };
            let choice = agents[receiver_idx].sample_position(delivered, &cands, rng)?;
            let reward = if choice == rec.actual_selected_candidate_index {
                1.0
            } else {
                0.0
            };
            match resolved.learning_mode {
                LearningMode::Joint => {
                    agents[sender_idx].update_sender(target_type, token, reward)?;
                    agents[receiver_idx].update_receiver(delivered, &cands, choice, reward)?;
                }
                LearningMode::None => {}
                LearningMode::ReceiverOnly => {
                    agents[receiver_idx].update_receiver(delivered, &cands, choice, reward)?;
                }
            }
        }
    }

    // Phase 3: persist the frozen full agent policies (sender + receiver
    // tables) as dyad artifacts. The serializer is deterministic (fixed
    // field order, stabilized floats), so the bytes are byte-identical
    // across runs.
    for (bundle, agent) in bundles.iter().zip(agents.iter()) {
        std::fs::write(
            format!("{}/agent-policy-{}.json", slot_dir, bundle.role),
            agent.to_bytes(),
        )
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;
    }

    // Phase 4: frozen-policy scoring per role. Test tokens come from the
    // OPPOSITE agent as sender through the same 1/8 channel; native
    // predictions are the role agent's frozen receiver position
    // posteriors (argmax scoring downstream). Dropped tokens predict
    // exact uniform. Branch probabilities, role folds, receipt, and
    // journal keep their exact shapes.
    let mut roles = Vec::new();
    let mut cases_attempted = 0u64;
    let mut cases_valid = 0u64;
    for (receiver_idx, bundle) in bundles.iter().enumerate() {
        let receiver = &agents[receiver_idx];
        let sender = &agents[1 - receiver_idx];
        let role = bundle.role.as_str();
        let test_seed = bundle.test_seed.as_str();
        let mut test_rng = ald_hashing::SeededPrng::new(test_seed).derive("emergent-test");

        let mut test = Vec::with_capacity(bundle.test_cases.len());
        let mut probs = Vec::with_capacity(bundle.test_cases.len());
        for c in &bundle.test_cases {
            let plan = plan_paired_case(test_seed, &c.case_id);
            // Frozen policies: the opposite agent as sender emits the test
            // token through the same 1/8 drop channel, and the role agent
            // as receiver scores its position posterior. The generated
            // placeholder token is discarded.
            let cands = candidate_types(&c.candidate_type_codes);
            let target_type = cands[c.target_index];
            let token = sender.sample_token(target_type, &mut test_rng)?;
            let channel_draw = test_rng.next_float();
            let delivered = if channel_dropped(channel_draw) {
                None
            } else {
                Some(token)
            };
            let delivered_token = delivered.map(|id| inventory[id].clone());
            let native: Vec<f64> = receiver
                .receiver_position_posterior(delivered, &cands)?
                .iter()
                .map(|v| stable_f64(*v))
                .collect();
            let native_missing = delivered_token.is_none();
            // One stabilized draw drives replay, branch shuffling, and every
            // persisted copy; the bit-hex is recomputed from it.
            let shared_draw = stable_f64(plan.shared_draw);
            let draw_bits_hex = hex::encode(shared_draw.to_bits().to_be_bytes());
            // Replay: exact one-hot on the paired draw's action.
            let draw_idx = ((shared_draw * 4.0).floor() as usize).min(3);
            let mut replay = vec![0.0; 4];
            replay[draw_idx] = 1.0;
            if !valid_distribution(&native) || !valid_distribution(&replay) {
                return Err(OrchestratorError::Domain(format!(
                    "ALA dyad: invalid native/replay distributions for {}",
                    c.case_id
                )));
            }
            let mut branch_rng =
                ald_hashing::SeededPrng::new(test_seed).derive(&format!("branch/{}", c.case_id));
            let mut branch_distributions = std::collections::BTreeMap::new();
            let mut branch_success = std::collections::BTreeMap::new();
            let mut outcomes = std::collections::BTreeMap::new();
            for b in ald_scenario::LV01_BRANCHES {
                let d =
                    ald_interventions::substitute_branch(b, &native, shared_draw, &mut branch_rng)
                        .map_err(|e| OrchestratorError::Domain(e.to_string()))?;
                if !valid_distribution(&d) {
                    return Err(OrchestratorError::Domain(format!(
                        "ALA dyad: invalid {b} distribution for {}",
                        c.case_id
                    )));
                }
                // total_cmp: identical argmax on the finite distributions
                // valid_distribution enforces above, and panic-free.
                let ok = d
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(i, _)| i)
                    == Some(c.target_index);
                branch_distributions.insert(b.to_string(), d.clone());
                branch_success.insert(b.to_string(), ok);
                outcomes.insert(
                    b.to_string(),
                    BranchOutcome {
                        probabilities: d,
                        success: ok,
                    },
                );
            }
            cases_attempted += 1;
            cases_valid += 1;
            probs.push(CaseProbabilities {
                case_id: c.case_id.clone(),
                target_index: c.target_index,
                delivered_token: delivered_token.clone(),
                native_missing,
                native: native.clone(),
                replay: replay.clone(),
                shared_draw,
                draw_bits_hex: draw_bits_hex.clone(),
                branches: outcomes,
            });
            let mut ev = serde_json::Map::new();
            ev.insert("caseId".to_string(), serde_json::json!(c.case_id));
            ev.insert("role".to_string(), serde_json::json!(role));
            ev.insert("targetIndex".to_string(), serde_json::json!(c.target_index));
            ev.insert(
                "deliveredToken".to_string(),
                delivered_token
                    .clone()
                    .map_or(serde_json::Value::Null, serde_json::Value::String),
            );
            ev.insert(
                "nativeMissing".to_string(),
                serde_json::json!(native_missing),
            );
            ev.insert("native".to_string(), serde_json::json!(native));
            ev.insert("replay".to_string(), serde_json::json!(replay));
            ev.insert("sharedDraw".to_string(), serde_json::json!(shared_draw));
            ev.insert("drawBitsHex".to_string(), serde_json::json!(draw_bits_hex));
            ev.insert(
                "branchSuccess".to_string(),
                serde_json::json!(branch_success),
            );
            ald_evidence::append_event(conn, ald_types::EventStream::Turns, ev)?;
            test.push(TestRow {
                record: OrdinaryRecord {
                    case_id: c.case_id.clone(),
                    receiver_role: c.receiver_role.clone(),
                    delivered_token: delivered_token.clone(),
                    candidate_type_codes: c.candidate_type_codes.to_vec(),
                    actual_selected_candidate_index: c.target_index,
                },
                native_distribution: native,
                replay_distribution: replay,
                target_index: c.target_index,
                native_missing,
                branch_distributions: branch_distributions.into_iter().collect(),
                branch_success: branch_success.into_iter().collect(),
            });
        }
        std::fs::write(
            format!("{slot_dir}/branch-probabilities-{role}.json"),
            to_pretty(&probs)?,
        )
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;
        let folds = RoleFolds {
            role: role.to_string(),
            // The persisted training fold is always the manifest-count
            // prefix, however many episodes ran over the bundle.
            training: bundle.training[..per_role_train].to_vec(),
            validation_fit: bundle.validation_fit.clone(),
            validation_selection: bundle.validation_selection.clone(),
            test,
        };
        // Role folds for `ala analyze select|score`: the generated
        // training/validation/test folds with emergent-native, replay, and
        // per-branch probabilities, in the committed fixture schema.
        std::fs::write(
            format!("{slot_dir}/role-folds-{role}.json"),
            to_pretty(&persistable_role_folds(&folds))?,
        )
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;
        roles.push(folds);
    }

    let mut result = analyze_dyad(&format!("{}-{:04}", req.kind, req.index), &roles, inventory)?;
    if !result.primary.is_finite() {
        return Err(OrchestratorError::Domain(
            "ALA dyad: non-finite dyad primary".to_string(),
        ));
    }
    // One stabilized primary for every artifact (receipt, journal, audit,
    // primary.json, dyad-result.json) so hashed and plain copies agree.
    result.primary = stable_f64(result.primary);
    std::fs::write(
        format!("{slot_dir}/primary.json"),
        result.primary.to_string(),
    )
    .map_err(|e| OrchestratorError::Io(e.to_string()))?;
    std::fs::write(format!("{slot_dir}/dyad-result.json"), to_pretty(&result)?)
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;

    // Non-default runs record the full development configuration alongside
    // the dyad artifacts: requested + resolved options, digests, the seeds
    // every split was generated from, the 2N/N counts, and the per-split
    // per-type exposure census. Legacy runs write no sidecar.
    if let Some(hash) = dev_hash {
        let mut seeds = serde_json::Map::new();
        let mut exposure = serde_json::Map::new();
        for b in &bundles {
            let mut role_seeds = serde_json::Map::new();
            role_seeds.insert("training".to_string(), serde_json::json!(b.train_seed));
            role_seeds.insert("validationFit".to_string(), serde_json::json!(b.fit_seed));
            role_seeds.insert(
                "validationSelection".to_string(),
                serde_json::json!(b.sel_seed),
            );
            role_seeds.insert(
                "withinSupportTest".to_string(),
                serde_json::json!(b.test_seed),
            );
            seeds.insert(b.role.clone(), serde_json::Value::Object(role_seeds));
            let train_codes: Vec<Vec<u32>> = b.training[..episodes_per_direction]
                .iter()
                .map(|r| r.candidate_type_codes.clone())
                .collect();
            let fit_codes: Vec<Vec<u32>> = b
                .validation_fit
                .iter()
                .map(|r| r.candidate_type_codes.clone())
                .collect();
            let sel_codes: Vec<Vec<u32>> = b
                .validation_selection
                .iter()
                .map(|r| r.candidate_type_codes.clone())
                .collect();
            let test_codes: Vec<Vec<u32>> = b
                .test_cases
                .iter()
                .map(|c| c.candidate_type_codes.to_vec())
                .collect();
            exposure.insert(
                b.role.clone(),
                serde_json::to_value(RoleExposure {
                    role: b.role.clone(),
                    training: exposure_histogram(&train_codes),
                    validation_fit: exposure_histogram(&fit_codes),
                    validation_selection: exposure_histogram(&sel_codes),
                    within_support_test: exposure_histogram(&test_codes),
                })
                .map_err(|e| OrchestratorError::Domain(e.to_string()))?,
            );
        }
        let sidecar = serde_json::json!({
            "requested": requested,
            "resolved": resolved,
            "digests": {
                "manifestSha256": manifest_sha256,
                "developmentConfigSha256": hash,
            },
            "seeds": {
                "seedRoot": seed_root,
                "roles": seeds,
            },
            "counts": {
                "perRoleTrainFold": per_role_train,
                "trainingInteractionsPerDirection": resolved.training_interactions_per_direction,
                "trainingInteractionsTotal": resolved.training_interactions_total,
                "perRoleValidationFit": per_role_fit,
                "perRoleValidationSelection": per_role_sel,
                "perRoleTest": per_role_test,
            },
            "exposureCounts": exposure,
            "versions": {
                "protocolVersion": req.protocol_version,
                "treatmentBatchVersion": req.treatment_batch_version,
                "manifestVersion": "ala-lv-execution-manifest/v1",
            },
        });
        std::fs::write(
            format!("{slot_dir}/development-config.json"),
            to_pretty(&sidecar)?,
        )
        .map_err(|e| OrchestratorError::Io(e.to_string()))?;
    }

    let mut audit_terminal = journal_slot_identity(req, pad, manifest_sha256);
    audit_terminal.insert("kind".to_string(), serde_json::json!("terminal"));
    audit_terminal.insert("state".to_string(), serde_json::json!("valid"));
    audit_terminal.insert(
        "casesAttempted".to_string(),
        serde_json::json!(cases_attempted),
    );
    audit_terminal.insert("casesValid".to_string(), serde_json::json!(cases_valid));
    audit_terminal.insert("primary".to_string(), serde_json::json!(result.primary));
    ald_evidence::append_event(conn, ald_types::EventStream::Audit, audit_terminal)?;

    Ok(DyadInner {
        primary: result.primary,
        cases_attempted,
        cases_valid,
    })
}
