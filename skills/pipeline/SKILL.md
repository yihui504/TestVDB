---
name: pipeline
description: TestVDB defect-mining pipeline SOP. Auto-loaded when the Orchestrator coordinates the defect-mining pipeline.
version: 1.1.0
---

# TestVDB Pipeline Skill

## Trigger conditions

Auto-loaded when the Orchestrator coordinates the defect-mining pipeline. Not user-triggered.

## Pipeline SOP

### Phase 1: Knowledge acquisition

1. The Orchestrator dispatches the Knowledge Extractor agent
2. Prefer the Crawl4AI local Docker service for fetching documentation (`python scripts/crawl_fetch.py`)
3. Fall back to WebSearch + WebFetch
4. Extract endpoints, parameters, constraints
5. Extract SDK versions and Docker tags
6. Output `raw_knowledge.json` (v3.4) + `deployment_meta.json`

### Phase 2: Contract formalization

1. The Orchestrator dispatches the Contract Formalizer agent
2. Read `raw_knowledge.json`
3. Convert to the structured contract per the JSON Schema
4. Generate the endpoint_registry (with source_url, doc_version, doc_quote)
5. Output `structured_contract.json`
6. Contract gate checks (core CRUD endpoint coverage ≥ 90%)

### Phase 3: Test generation (v2.0 fan-out)

1. The Orchestrator concurrently dispatches the Attack Trio (boundary + state + semantic)
2. **v2.0 fan-out**: each agent is dispatched once per focus_profile, 3 profiles (9 concurrent total)
   - priority_first: high-severity constraints first
   - coverage_gap: low-coverage endpoints first
   - rejection_pattern: route around known rejection patterns
3. Each agent independently generates test scripts (at most 30 per agent/profile/round)
3a. **v2.0 cross-session strategy injection**: query applicable strategies from the Strategy Registry → inject into the attack agent prompt
    - high-confidence (>0.7) strategies serve as preferred attack templates (note: this is strategy_registry's historical performance score, not the contract's LLM confidence self-rating — the latter has been removed)
    - apply the DB-specific adaptation rules in migration_rules
    - strategies with `status=deprecated` are not injected
4. Inject reflection_context + cross-session strategies (none in the first round)
5. Debate Stage 1: automated review (syntax validation + constraint validation + risky-pattern checks + the retry sub-loop; ADR-0008: script dedup removed)
6. Approved scripts are stored in `results/{target}/{version}/{timestamp}/script_*.py`

### Phase 4: Sandboxed execution

1. The executor picks the Docker Compose template per DB
2. Image-tag pre-check → start containers → health check
3. Install SDK dependencies → run the scripts in independent execution containers
4. Collect results (stdout/stderr/exit code/HTTP responses/container logs)
5. **Containers stay running** (for later reuse by the reporter)

### Phase 5: Defect adjudication (ADR-0008 evidence-chain duo)

1. **Candidate extraction** (extract_candidates.py, mechanical): output_*.log containing `VERDICT: DEFECT_FOUND` → candidates.jsonl
2. **L1 mechanical gate** (verify_live_l1.py, 0 tokens): kills ~90% of historical false-positive patterns; REFUTED eliminated
3. **evidence-builder concurrent fan-out per candidate** (1 builder/candidate):
   - step1 doc verification + execution-evidence review + chain tracing → `evidence_chain/{defect_id}.json`
   - step2 source forensics (local clone Grep + call-chain tracing)
   - no truth judgment; factual evidence only
4. **chain-auditor single-instance close-out** (after all builders' .done): the completeness/consistency/self-consistency triple check + the perspective aggregation (contract/physical override behavioral elegance) → `chain_verdicts.json`
   - verdict ∈ {DEFECT, NOT_DEFECT, NEEDS_MORE_EVIDENCE}
   - FP verdicts require fp_evidence_source (doc/source/both/behavior) + root_cause
   - NEEDS_MORE_EVIDENCE goes back to the builder for one more evidence round (at most once); still contradictory → conservative NOT_DEFECT
5. **novelty final ruling deferred** to pre-submission (after Phase 7): NON_NOVEL archived, not deleted (archived/ + manifest.json)

### Phase 6: Report generation

1. The reporter runs the Pre-Submit Gate (100% reproduction verification)
2. Generate defect-N.md (with the 3-Ring evidence chain + the four-type defect classification)
3. Generate self-contained MRE scripts (no dependency on TestVDB code)
4. Generate summary.md
5. Save session_metadata.json

## Iteration loop

- Each round's end generates reflection_context (key_learnings + rejection_patterns + high_value_endpoints + exhausted_endpoints)
- Injected into the next round's attack agents
- Stalemate detection: 5 consecutive rounds with no new defects → re-search documentation → re-evaluate candidates → adjust strategies
- Termination conditions: stalemate / coverage ≥ 95% / max_rounds reached / min_defects reached

## Inter-agent communication

- All agents communicate through the filesystem (structured_contract.json, pipeline_state.json, debate_logs/*.json)
- `.done` marker files ensure write atomicity
- The orchestrator checks `.done` files rather than the output files directly

## Container lifecycle

- The executor starts containers and does **not** clean up after execution
- The judge (evidence) reuses running containers for reproduction verification
- The reporter reuses running containers for the Pre-Submit Gate
- The orchestrator cleans up uniformly at each round's end / session end
