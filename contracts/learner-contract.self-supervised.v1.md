<!-- contract: self-supervised version: 1 -->

> HISTORICAL RESEARCH RECORD — not a runtime dependency. This file specifies
> an alternative learner track from the pre-Rust design era. The workspace
> implements exactly one learner (the emergent `AgentPolicy` in
> `crates/ald-learners`), and no code, manifest, or run path reads this
> contract. Retained for provenance; its tool/ledger/affect/gradient language
> does not describe current behavior.

Operational contract for the self-supervised ungrounded track.

## Scope

The learner begins from a private random initialization and updates a private
predictive model from its own observation and delivery history. Its update
path receives no scalar task reward, outcome label, counterpart state,
counterpart parameters, shared gradient, or external semantic supervision.

## Permitted actions

The learner acts only through the declared tool surface: one bounded public
artifact when sending, one candidate selection when receiving, required
private ledger drafts, and an affect submission only during an open window.
Ordinary messages, prose output, labels, trusted metadata, and additional
communication routes are prohibited.

## Evidence discipline

- Treat unfamiliar marks as semantically unknown at initialization.
- Keep observations distinct from provisional inferences.
- Preserve contradictory evidence and append revisions without rewriting
  prior entries.
- Record uncertainty and the evidence references supporting each hypothesis.
- Make no shared-understanding claim without behavioral evidence.

## Update rule

Updates optimize only the pre-registered self-supervised objective over this
learner's private buffer. Evaluation disables updates. Curriculum transitions
may alter only pre-registered supported knobs and are recorded with policy
hashes. Every update emits a policy checkpoint reference for run evidence.
