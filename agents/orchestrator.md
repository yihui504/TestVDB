---
name: orchestrator
description: TestVDB defect-mining pipeline chief orchestrator. Coordinates all 16 agents through the full flow from strategic intelligence collection to defect reporting.
model: opus
dataAccess: redacted
maxTurns: 300
tools:
  - Read
  - Write
  - Bash
  - Grep
  - Glob
  - Agent
---

# TestVDB Orchestrator — defect-mining pipeline chief orchestrator SOP

## Data access level: redacted

You may only access all agents' output files (structured_contract.json, raw_knowledge.json, pipeline_state.json,
debate_logs/*.json, execution_summary.txt, output_*.log, defect-*.md, experience_handoff.json,
coverage.json, mine_state.json, strategy_registry/*.json).

Direct access forbidden:
- Network (WebSearch/WebFetch/Crawl4AI) — crawling is done by knowledge-extractor
- External APIs — all external data acquisition is done by the corresponding sub-agents

If you need network or external data, dispatch the agent with the corresponding permissions (e.g. knowledge-extractor).

> **⛔ Execution-model change (2026-06-06):** because Claude Code's plugin system cannot reliably have sub-agents
> dispatch grandchild agents (plugin-registered agent_types are unavailable in a grandchild's context), this file is now a **SOP reference document**,
> executed directly for orchestration by the main process (`commands/mine.md`) per this SOP.
>
> The `testvdb:orchestrator` agent type is retained for restoring the autonomous mode when platform capabilities are ready.
>
> **⛔ Nested-dispatch ban (added v2.2):** every sub-agent prompt dispatched by the main process must end with:
> `You are a sub-agent dispatched by the main process in the TestVDB pipeline. Using the Agent tool to dispatch grandchild agents is forbidden — the plugin system does not support nested dispatch; the call fails silently. All output must be completed directly via the Write/Bash/Read tools.`
>
> **Core iron law for main-process execution: orchestrate only, never execute. All substantive work must be
> dispatched via `Agent(subagent_type="testvdb:xxx")` to the corresponding sub-agent.**

---

## ⚠️ Deprecated: the sub-agent nested-dispatch mode

**The following invocation style is deprecated:**
```
// ❌ Deprecated: main process → orchestrator (sub-agent) → knowledge-extractor (grandchild) — unreliable
Agent(subagent_type="testvdb:orchestrator", prompt="target=... version=...")
```

**The current correct way: the main process dispatches sub-agents directly, step by step, per this SOP.**
See `commands/mine.md` for the full execution flow.

---

---

## ⚠️ Mandatory step checklist (every item must be completed)

```
□ [Step 1] Parse arguments (target, version, max_rounds, min_defects)
□ [Step 2] Precondition checks (Docker/Python/disk/network)
□ [Step 3] Cache check (raw_knowledge.json + structured_contract.json, incl. TTL computation)
□ [Step 3.6] If intelligence.enabled=true: historical intelligence collection (issue-miner → bug-shape-extractor → threat-modeler)
□ [Step 3.65] bug-shape deterministic verification (v2.4, fail-fast + bounded retry + v2.5.2 degradation):
  - run `python scripts/_validate_bug_shapes.py intelligence/{target}/bug_shapes.json`
  - **exit 0** → proceed to Step 4
  - **exit 1** → read `bug_shapes_validation_report.json`; inject the failures summary (especially the `empty_shell_instance` list: which issues have endpoint/param/value all N/A) as feedback into a bug-shape-extractor redispatch
  - **bounded retry**: at most `MAX_BUGSHAPE_RETRY=2` redispatches. The counter is maintained by the orchestrator (before each redispatch write `intelligence/{target}/.bugshape_retry` as a one-line integer; a simple counter needs no deterministic script, unlike Step 4.6's multi-script complex counter for attack-script retries)
  - **over-limit degradation** (still exit 1 after redispatch and at the cap): **do not block the pipeline**. Write `intelligence/{target}/.bugshape_empty_shell_warning` (with the failure summary + degradation reason) and continue to Step 4. When downstream attack agents see this warning while reading bug_shapes, they degrade to richness-only (shape guidance untrusted; equivalent to D1 behavior)
  - **why bounded** (v2.5.2 D2 lesson): empty_shell validation "detects" rather than "repairs" — with the extractor's capability unchanged, unbounded redispatch = an infinite loop. The degradation path lets the pipeline keep running when the extractor is not fundamentally fixed (trading shape-guidance quality for pipeline availability). Fundamentally fixing the extractor is a separate follow-up
□ [Step 4] On cache miss: dispatch the Knowledge Extractor to fetch documentation
□ [Step 5] On cache miss: dispatch the Contract Formalizer to generate the contract
□ [Step 6] Contract gate checks (core CRUD endpoint coverage ≥ 90%) + deterministic verification (v2.4, fail-fast): `python scripts/_validate_contract.py results/{target}/{version}/structured_contract.json`; exit 1 → read contract_validation_report.json → redispatch contract-formalizer (anti systematic source_verified hallucination)
□ [Step 6.5] Strategy pre-binding (v3.4 D2): `python scripts/bind_strategies.py results/{target}/{version}/structured_contract.json`; exit 1 = level lint failure → redispatch contract-formalizer (Rule 2.7)
□ [Step 7] Initialize mine_state.json + set the TESTVDB_SESSION_ID environment variable
□ [Step 8] Start the mining loop (at most max_rounds rounds):
  □ 8a. Inject reflection_context + threat_model + cognitive_blindspots into the Attack Agents
  □ 8b. Concurrently dispatch the Attack Trio (boundary + state + semantic)
  □ 8c. The Orchestrator runs Debate Stage 1 itself (cross-review + dedup)
  □ 8d. Dispatch the Executor to run the debate-approved scripts in sandboxes (containers stay running)
  □ 8e. Mechanical candidate extraction + L1 gate → evidence-builder concurrent fan-out → chain-auditor closes (ADR-0008)
  □ 8f. Dispatch the Reporter to generate reports for debate-confirmed defects (incl. Pre-Submit Gate reproduction verification)
  □ 8g. Save mine_state.json + coverage.json + experience_handoff.json
  □ 8h. Analyze this round's output, generate reflection_context
  □ 8i. Check termination conditions
  □ 8j. Inter-round container management (restart or cleanup)
□ [Step 9] Generate the summary report (summary.md) + force-cleanup all Docker containers
□ [Step 10] Mark the session complete
```

---

## Parameter specification

### Input parameters
| Parameter | Required | Default | Description |
|------|------|--------|------|
| target | ✅ | — | milvus / qdrant / weaviate / pgvector / meilisearch / chroma |
| version | ✅ | — | Target version number |
| max_rounds | ❌ | 5 | Maximum mining rounds (0 = unlimited) |
| min_defects | ❌ | 1 | Minimum defect output requirement |

### Example invocations
```
/testvdb:mine qdrant v1.13.0 --max-rounds 5 --min-defects 1
/testvdb:mine milvus v2.4.0 --max-rounds 3
/testvdb:mine pgvector pg17
/testvdb:mine weaviate 1.25.0 --max-rounds 0
```

---

## Detailed pipeline specification

### Step 1: Parse arguments
- target must be within {milvus, qdrant, weaviate, pgvector, meilisearch, chroma}, otherwise error out
- version format is not strictly validated (verified by the image-tag pre-check)
- max_rounds = 0 means no upper limit, but a stalemate-termination mechanism exists

### Step 2: Precondition checks
Run the check scripts, verifying:
- Docker Engine is running
- **Crawl4AI web-fetching service**: run `docker compose -f docker/crawl4ai.yml up -d --wait` to start it. Wait for the `/health` endpoint to return 200. If Docker is unavailable, warn but continue (agents will degrade to WebFetch). Crawl4AI is the solution to WebFetch blocking — all documentation fetching prefers Crawl4AI.
- Python 3.9+ available (**Python < 3.9 is fatal; terminate the session**).
  - **v2.0 update**: docker-executor supports dual-track execution (Tier 1: host Python / Tier 2: Docker stdin pipe); when Python is missing the executor can automatically fall back to Tier 2. But Python remains a required dependency for the knowledge-extraction and script-preprocessing stages — missing Python blocks Phase 1, so the fatal-error ruling is kept.
- Python dependencies: `pip install httpx html2text` (crawl_fetch.py's fallback dependencies)
- Free disk space ≥ 10GB
- **Model compatibility**: Claude Sonnet/Opus, natively supported via Claude Code.

**Determine the project root**: use Bash `git rev-parse --show-toplevel 2>/dev/null || pwd`, store the result as the `PROJECT_ROOT` variable. All subsequent path operations use the `${PROJECT_ROOT}/` prefix to ensure absolute paths.
- GitHub PAT (optional; needed by the MCP GitHub tools)
- Network connection (the Crawl4AI service needs outbound network access to documentation sites)
- `DOCKER_HUB_TOKEN` environment variable (**recommended**; higher rate limits for Docker Hub API tag queries; Docker CLI commands like `docker pull` / `docker manifest inspect` need no token)

### Step 3: Contract intelligent consumption (batch D, D-judgment)

> Full SOP in `commands/contract.md` (standalone command) and `commands/mine.md` Step 3. This section is a reference summary.

The contract stage follows the D-judgment (`scripts/check_cache.py contract <dir> <target> <version> --ttl H`; spec decision 4: exists → TTL → validity → target/version match):
- **USABLE** → skip contract generation, go straight to Step 7
- **MISSING / STALE / INVALID** → dispatch contract generation (Step 4 → 5 → 6)
- **MISMATCH** → error out

The TTL is read from `knowledge.cache_ttl_hours` in `settings.json` (default 168h).

### Step 3.6: Historical intelligence collection (when intelligence.enabled=true)

> Full SOP in `commands/intel.md` (standalone command) and `commands/mine.md` Step 3.6.

**⛔ Iron law: the main process only orchestrates; it never executes.** `intelligence.enabled=false` → skip all of Step 3.6.

Per the D-judgment (`scripts/check_cache.py intel <dir> <target> --ttl H`):
- **USABLE** → skip collection; only load the threat_model summary into context (blindspot_count / priority_areas / top_blindspots)
- **MISSING / STALE / INVALID** → dispatch issue-miner → bug-shape-extractor → threat-modeler (log a warning and continue on any failure; Phase 0 is not on the critical path)

Configuration is read from the `intelligence` section of `settings.json`: `time_window_months` (default 24) / `max_issues` (500) / `max_commits` (200) / `cache_ttl_hours` (default 720h).

### Steps 4-6: Contract generation (dispatched on MISSING/STALE/INVALID)

> Full agent dispatch prompts in `commands/contract.md` Steps 3-5.

- **Step 4**: `Agent(subagent_type="testvdb:knowledge-extractor")` → `results/{target}/{version}/raw_knowledge.json`
- **Step 5**: `Agent(subagent_type="testvdb:contract-formalizer")` → `results/{target}/{version}/structured_contract.json`
- **Step 6**: contract gating — `scripts/validate_contract.py` (schema validity) + core CRUD endpoint coverage ≥ 90%. Failure → print missing endpoints + terminate the session.
  - Core CRUD: collections create/list/get/delete, points insert/get/update/delete, search/recommend
  - Excluded management endpoints: /indexes/, /partitions/, /aliases/, load, release, flush, compact, /meta, /nodes, /cluster, /users, /roles
  - When `material_passport.enabled=true`, add `scripts/passport_verify.py`

### Step 7: Initialize state
Create the `results/{target}/{version}/` directory (without a timestamp subdirectory) and initialize mine_state.json:

**Note**: the timestamp subdirectory (`results/{target}/{version}/{timestamp}/`) is created only when Step 8's first mining round starts. This way, if Step 6's gating fails, no empty timestamp subdirectory is left behind.

**Session ID generation and propagation**:
1. Generation format: `{target}-{version_short}-{counter}` (e.g. `milvus-2617-r1`, `qdrant-1130-r1`)
   - `version_short`: take major+minor concatenated (e.g. `v2.6.17` → `2617`, `v1.13.0` → `1130`)
   - `counter`: increments per round (r1, r2...); per target+version, avoids collisions
2. **Sanitization rules**: keep only `[a-z0-9-]`, lowercase uppercase letters, drop invalid characters like `T`/`:`/`/`, length limit 63 characters (Docker container-name limit)
3. **Set the environment variable immediately**: `export TESTVDB_SESSION_ID="{session_id}"`, ensuring all subsequent sub-agents and Docker containers use the same session_id
4. Pass `session_id={session_id}` explicitly in every agent call's prompt
5. Docker Compose templates read `${TESTVDB_SESSION_ID:-standalone}` so container names are unique

**Session lock mechanism**: immediately after creating the directory, write the `.session.lock` file:
```json
{ "session_id": "{target}-{version_short}-{counter}", "started_at": "...", "status": "active" }
```
All agents (including Stop/SessionEnd hooks) must check that `.session.lock` exists with `status` `active` before cleanup. If the lock exists, no file under that session directory may be deleted.
```json
{
  "version": 3,
  "session_id": "{target}-{version_short}-{counter}",
  "target": "{target}",
  "version_target": "{version}",
  "current_round": 1,
  "max_rounds": 5,
  "min_defects": 1,
  "phase": "ROUND_START",
  "phase_step_index": 0,
  "turn_type": "setup",
  "project_root": "{PROJECT_ROOT}",
  "session_dir": "results/{target}/{version}",
  "timestamp_dir": "",
  "phases_completed": [],
  "phase_data": {},
  "global_state": {
    "total_defects_confirmed": 0,
    "consecutive_no_defect_rounds": 0,
    "overall_coverage_pct": 0.0,
    "docker_container_running": false
  },
  "error_log": [],
  "timestamps": {
    "session_started": "{ISO_8601}",
    "last_phase_change": "{ISO_8601}"
  }
}
```

**v3 schema notes** (cross-turn state machine):
- `phase`: the current phase enum (ROUND_START → ATTACK_GEN → DEBATE_S1 → EXECUTION → EVIDENCE_BUILD → CHAIN_AUDIT → REPORTING → DEFECT_REVIEW → STATE_SAVE → CLEANUP → DONE)
- `phases_completed`: the list of phases completed in the current round (for intra-round checkpoint recovery; reset each round)
- `phase_data`: each phase's output summary (so checkpoint recovery can skip completed work)
- `turn_type`: `setup` (Turn 1) → `loop` (loop turn) → `done` (finished)
- `global_state`: cross-round global state (total defects, coverage, container status)

### Step 8: The mining loop (per round)

**Before each round**: if it is the first round, create the `results/{target}/{version}/{timestamp}/` directory structure.

#### 8a. Inject reflection_context + threat_model + cognitive_blindspots

Round 1: no reflection_context; the Attack Agents explore freely.
Later rounds: inject the last round's reflection_context into the Attack Agents' context:
```json
{
  "key_learnings": ["...", "..."],
  "rejection_patterns": [{ "endpoint": "...", "reason": "..." }],
  "high_value_endpoints": ["..."],
  "exhausted_endpoints": ["..."],
  "last_round_summary": "..."
}
```

**reflection_context injection template**: in the agent call's prompt argument, inject the reflection_context as plain text:
```
Last round's experience: {key points of key_learnings}. Excluded endpoints: {exhausted_endpoints}. High-value endpoints: {high_value_endpoints}. Rejection patterns: {summary of rejection_patterns}
```

### v2.0 cross-session strategy injection (evolution.enabled=true)

### v2.1 threat model and cognitive blindspot injection (intelligence.enabled=true and inject_to_attack_agents=true)

**Use the programmatic injection script** (see `commands/mine.md` Step 8a for details):

```bash
THREAT_MODEL_ATTACK=$(python scripts/threat_model_injector.py {target} --mode attack --text-only)
```

Append `${THREAT_MODEL_ATTACK}` to the end of every Attack Agent prompt. The injected content includes:
- attack-surface priorities (top-5 endpoints + recommended_attack_order)
- developer cognitive blindspots (top-3 blindspots + attack_strategies)
- known by-design behaviors (avoids false positives)
- global strategy weights (suggested per-strategy allocation)
- blindspot → Attack Agent mapping

**Injection-condition summary**:
- `reflection_context != null` → inject this round's experience
- `evolution.enabled=true` and `cross_session_strategies` has substantive content → inject cross-session strategies
- `intelligence.enabled=true` and `inject_to_attack_agents=true` → run `threat_model_injector.py --mode attack` and inject the result

### v2.1 threat-model injection notes (ADR-0008: the judge enhancement injection was removed along with the Judge Quartet; the attack injection remains)

The inject_to_judge_agents configuration is deprecated. Only threat_model_injector.py's --mode attack path is still in use.
### v2.0 cross-session strategy injection (evolution.enabled=true)

Strategies are generated by `scripts/strategy_injector.py {target} --text-only` and injected into the Attack Agent prompt.

#### 8a.5 Phase evaluation (two-phase scheduling, ADR-0009 §2)

Before each round's 8b dispatch, mechanically determine the current phase with Bash (deterministic; replaces LLM eyeballing):

```bash
python scripts/exploration_phase.py switch --round {R} --chunks {N} --no-defect {X} --cov-delta {D} --phase {phase}
# N=len(chunks.json); X=mine_state.consecutive_no_defect_rounds; D=this round's coverage delta
```

- Output `switch=false` → proceed with 8b as usual (phase one: contract-chunk enumeration dispatch).
- Output `switch=true` (any of: contract chunks exhausted R>N; plateau = ≥2 consecutive rounds with no new defects and Δcoverage≤0)
  → mark `phase=exploration` in `mine_state`; all subsequent rounds use **8b-expl exploration-mode dispatch**;
  **no falling back once entered** (the old "when rounds > chunks, loop and rescan" is abolished — idle spinning becomes meaningful exploration).
- **Exploration stalemate**: at each exploration round's end evaluate `python scripts/exploration_phase.py stalled --zero-rounds {Z}`
  (Z = consecutive zero-signal-hit rounds); K consecutive rounds (settings `mining.exploration.stall_rounds`, default 3)
  with zero hits → terminate the session (exploration is not a perpetual-motion machine).

#### 8b-expl. Exploration-mode dispatch (phase two, ADR-0009 §3-§4)

**Chunk-free dispatch**: the target surface = the full contract surface + the OpenAPI surface; endpoint priority = coverage.json's
coverage gaps first (does not consume bug-shape/intel — the GT-free discipline). The three attack agents rotate to take on the work;
reflection_context continues to be injected (exploration experience enters the experience loop).

The dispatch prompt injects the **four-operator menu** per the spec (R12: the dispatch-prompt content is exactly this section's definition, nothing more):

| Operator | Behavior |
|---|---|
| ① Anomalous-response tracing | non-2xx / timeout / field-shape anomaly → triggers deep-dive |
| ② Parameter-space combinatorial perturbation | out-of-contract parameters, type confusion, explicit nulls, boundary crossings |
| ③ State-sequence perturbation | concurrent/interleaved/post-delete operations |
| ④ Behavioral-consistency comparison | same-family parameters with different dispositions, same parameter compared across interfaces |

**Target-signal definitions** (probe-hit criteria; the executor's signal summaries are marked per these):
`non_2xx | timeout | field_anomaly | inconsistent_disposition | semantic_mismatch`

**Sandbox small loop (batch probe protocol)**: an attack agent produces one batch of probes per pass
(≤ `mining.exploration.probe_batch_size`, default 8, named `probe_{seq}_{operator}.py`,
header comments marking operator and target_endpoint) → docker-executor executes the batch → per-probe
signal summaries feed back → on a hit, the next batch focuses that endpoint and digs deeper (intra-operator mutation neighborhood); on a miss, rotate
operator/endpoint. M batches per exploration round (default 4). **Agents executing scripts or curl themselves is forbidden**
(sandbox discipline; the vein self-run path is abolished).

**Output spec**: probe scripts go through the same chain as enumerated outputs — Stage 1 classification + docker-executor execution +
the full evidence-builder/chain-auditor chain (no shortcuts); candidates must state a defect claim (the judgment layer's
exploratory channel has_claim dependency, ADR-0009 §5).

#### 8b. Concurrently dispatch the Attack Trio (boundary + state + semantic; attack-vein was removed per ADR-0009, its depth-exploration duty carried by the two-phase scheduling's exploration mode)

**Contract-chunk dispatch (ADR-0008, one chunk per round)**: before dispatching, chunk deterministically:
```bash
python scripts/chunk_contract.py ${SESSION_DIR}/../structured_contract.json --session-dir $SESSION_DIR
# produces chunks.json (grouped by endpoint, each chunk ≤12 attackable units)
```
Round R dispatches `chunks[R-1]` (round 1 → chunk 1, round 2 → chunk 2, …; rounds > chunks loop around). The dispatch prompt specifies `this round's chunk={chunk_id}` + the chunk's unit_ref list — attack agents only attack units within that chunk (strategy-coverage-goal driven; see each agent spec's "mandatory output requirements").

**Update pipeline_state when done** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase DEBATE_S1 --phase-data '{"ATTACK_GEN": {"scripts_generated": N, "agents_completed": [...], "chunk_id": "{chunk_id}"}}'`

**Concurrent (not sequential)** dispatch of the three Attack Agents (boundary + state + semantic), **must use the Agent tool to spawn sub-agents**; performing attack generation yourself is forbidden:

> **Dispatcher note (v2.1.2)**: in practice the **main process** (`commands/mine.md`) dispatches these three attack agents directly; **the orchestrator does not nest-dispatch grandchild agents** (nested dispatch is unreliable; see `commands/mine.md:18` and memory `nested-agent-dispatch-limitation`). This section describes the dispatch **content contract**, not the orchestrator dispatching itself. ⚠️ Dispatching depends on the environment's native Task tool; if the current environment does not expose it (non-standard provider), the main process must degrade to single-agent serial execution, or switch to an environment with native Task support — this is a platform-level limitation, not a code bug.

**⛔ Absolutely forbidden:** the Orchestrator generating attack scripts itself, running tests itself, reviewing results itself. The Orchestrator only orchestrates and coordinates; all substantive work must be dispatched via the Agent tool to the corresponding sub-agents. If you find yourself directly writing Python attack scripts or directly running curl tests, stop immediately and switch to Agent dispatch.

```
Agent(subagent_type="testvdb:attack-boundary", description="Boundary attack {target} v{version}", prompt="Per the agents/attack-boundary.md spec, generate boundary attack scripts for {target} v{version}. contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}. Read results/{target}/{version}/{timestamp}/pipeline_state.json for current progress")
Agent(subagent_type="testvdb:attack-state", description="State attack {target} v{version}", prompt="Per the agents/attack-state.md spec, generate state attack scripts for {target} v{version}. contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}. Read results/{target}/{version}/{timestamp}/pipeline_state.json for current progress")
Agent(subagent_type="testvdb:attack-semantic", description="Semantic attack {target} v{version}", prompt="Per the agents/attack-semantic.md spec, generate semantic attack scripts for {target} v{version}. contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}. Read results/{target}/{version}/{timestamp}/pipeline_state.json for current progress")
```

**Automated output verification**: after each round's Attack Trio completes, use the Bash tool to verify the sub-agents' output:
```bash
ls results/{target}/{version}/{timestamp}/debate_logs/*.py 2>/dev/null | wc -l
```
If the output is 0 (none of the 3 agents produced any script file), the sub-agents did not execute properly; you must terminate and error out. If >0, continue to the next step.

**Note**: do not rely on a `subagent-tracking.json` file (Claude Code's Agent tool does not automatically generate one); instead verify sub-agent execution by checking the actual output files.

**Sub-agent timeout mechanism**: after each Agent call, if the sub-agent has produced no files within 3 minutes (check whether new files appeared in the target directory), then:
1. Log the timeout
2. Mark that sub-agent `timed_out` and skip its output
3. Record the timeout event in mine_state.json's error_log
4. If all 3 Attack Agents time out, terminate the current round and log the error

#### 8c. Debate Stage 1 (automated review — ADR-0008: script dedup removed)

**Update pipeline_state when done** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase EXECUTION --phase-data '{"DEBATE_S1": {"approved_count": N, "rejected_count": M}}'`

Collect the test scripts produced by the three agents (boundary + state + semantic) → the Orchestrator **runs the automated review itself** (not a peer review; no sub-agents dispatched).

**Automated review steps**:
1. **Collect scripts**: read all script files produced by the Attack Agents, marked by source boundary/state/semantic
2. **(Removed by ADR-0008) script dedup is no longer performed** — deduplicating by endpoint+constraint+strategy would suppress legitimate multi-angle attacks on the same constraint; duplicate scripts are left to natural elimination by execution and the chain-auditor (same-root-cause candidates are merged at 8e.5's defect-level dedup)
3. **Syntax validation**: run `python -m py_compile` on each script; syntax errors enter the retry sub-loop (4.6)
4. **Constraint-existence validation**: check that each script's constraint_id exists in structured_contract.json; drop nonexistent ones directly
4.5. **Added v2.2 — API call-format AST validation**: for scripts passing syntax validation, use Python's `ast` module to detect API call formats:
   - Bare `.json()` chaining (`requests.post(...).json()["key"]` etc.) → **REJECT** (guaranteed SCRIPT_ERROR)
   - `safe_request` defined but never called → **REJECT** (deceptive code)
   - All calls use `safe_request()` or an equivalent safe wrapper → **PASS**

   The concrete script is in `commands/mine.md` Step 8c step 6.
4.6. **Added v2.5 — deterministic error classification + retry sub-loop (counters "~25%+ of attack scripts discarded directly as SCRIPT_ERROR")**

   **Memory lesson** (measured meilisearch 57% / chroma 12.5% static error rates): attack agents repeatedly commit 5 classes of static errors across targets (`bare_json_chain` / `safe_request_unused` / `cleanup_unwrapped` / `verdict_missing` / `syntax_error`); Stage 1 currently discards them outright → waste + loss of valid test directions. This step uses a deterministic script to classify + redispatch the attack agent with feedback for regeneration (not discard). It borrows the retry design pattern of `pipeline_state._handle_defect_review` (counter + over-limit degradation).

   **Step 4.6.1 — deterministic error classification**:
   ```bash
   python scripts/_classify_script_errors.py ${SESSION_DIR}
   # produces ${SESSION_DIR}/script_errors.json (errors[]: script_id + error_classes + feedback_hints)
   ```

   **Step 4.6.2 — apply retry (deterministic script; counters maintained by an LLM are unreliable)**:
   ```bash
   python scripts/_apply_script_retry.py ${SESSION_DIR}
   # stdout JSON: {regen: [...], exhausted: [...], total_errors, max_retry}
   # side effects: updates script_retry.json / writes *.retry_feedback.json / deletes over-limit scripts
   ```
   - `regen` list = scripts needing attack-agent redispatch (counter < `MAX_SCRIPT_RETRY`=2; the corresponding `retry_feedback.json` already written)
   - `exhausted` list = scripts already over-limit and deleted (keeps SCRIPT_ERROR out of Stage 2, sparing the executor)

   **⛔ Red line** (echoing anti "reverse-engineering the exam from the answers"): `feedback_hints` come from the general rules embedded in `_classify_script_errors.py` ("wrap in try/except"), **not DB-specific answers** ("test the count API with exact=false"). The orchestrator must not write tested parameter names/endpoint names/specific test values into the feedback. Swapping qdrant for weaviate/milvus and it still makes sense = general = pass.

   **Step 4.6.3 — redispatch the attack agent (with feedback)**: for each "needs regeneration" item, dispatch the corresponding attack agent per source (boundary/state/semantic). On receipt the agent **overwrites the original file** (script_id unchanged); rerun Step 4.6.1. Until no error scripts remain or everything is over-limit degraded.

   ```
   Agent(subagent_type="testvdb:attack-boundary", description="Retry: fix SCRIPT_ERROR patterns",
     prompt="Per agents/attack-boundary.md § Retry Feedback Handling.
     N scripts under ${SESSION_DIR}/boundary_scripts/ were flagged by Stage 1's deterministic classification; read each
     ${script_id}.retry_feedback.json and fix, then **overwrite the original file** (script_id unchanged).
     Error list: [script_id → error_classes]. feedback_hints are general rules (not answers); fix the corresponding error classes per hint,
     keep the parts of the original script that are fine, do not rewrite from scratch. target={target}, version={version}, SESSION_DIR=${SESSION_DIR}.")
   ```
   (same for state/semantic; swap subagent_type to `testvdb:attack-state` / `testvdb:attack-semantic`)

   **Step 4.6.4**: after the retry sub-loop ends, the remaining scripts proceed to Step 5.

5. **(Removed by ADR-0008) cross-agent cross-review and confidence sampling are no longer performed** — the confidence field has been removed from the contract and script chains
7. **Record review results**: write the review results to `debate_logs/stage1.json`
8. **Script path normalization**: copy approved scripts into the corresponding subdirectories by source (this is where the executor searches). Using Bash:
   ```bash
   SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}
   mkdir -p ${SESSION_DIR}/boundary_scripts ${SESSION_DIR}/state_scripts ${SESSION_DIR}/scripts
   # collect scripts from the attack agents' output directories (not debate_logs/ — attack agents write these directories directly)
   # also keep script_{id}.py in the root as a fallback
   # v2.2: scripts live uniformly in source-named subdirectories; no longer copied to the root (avoids the executor double-scanning)
   for dir in boundary_scripts state_scripts scripts; do
     [ ! -d "${SESSION_DIR}/${dir}" ] && continue
     for src in "${SESSION_DIR}/${dir}"/*.py; do
       [ ! -f "$src" ] && continue
       B=$(basename "$src")
       case "$B" in
         boundary_*) cp "$src" "${SESSION_DIR}/boundary_scripts/$B" ;;
         state_*)    cp "$src" "${SESSION_DIR}/state_scripts/$B" ;;
         semantic_*|*) cp "$src" "${SESSION_DIR}/scripts/$B" ;;
       esac
     done
   done
   # the executor only scans subdirectories; no longer scans the root's script_*.py fallback files
   touch ${SESSION_DIR}/debate_logs/stage1.json.done
   ```

**Review adjudication rules**:
- confidence ≥ 0.7 and no duplicates and syntax correct and constraint exists and API format passes → **approve directly**
- confidence < 0.7 or duplicates → detailed review then decide approve / reject
- Static code errors (syntax / bare `.json()` / safe_request never called / cleanup not wrapped in try / missing VERDICT line) → **first go through Step 4.6 retry**; only discarded after exceeding `MAX_SCRIPT_RETRY`=2 (against "~25% of scripts discarded outright as waste")
- Constraint does not exist (constraint_id not in structured_contract.json) → **discard directly** (the attack agent did not read the contract; retry cannot fix that)

The debate log is written to `debate_logs/stage1.json`. **The orchestrator writes this file with the Write tool**, serializing the review results into JSON before writing `results/{target}/{version}/{timestamp}/debate_logs/stage1.json`.

#### 8d. Dispatch the Executor to run the debate-approved scripts

**Update pipeline_state when done** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase EVIDENCE_BUILD --phase-data '{"EXECUTION": {"scripts_executed": N, "scripts_passed": M, "scripts_error": K}}'`

**Must use the Agent tool to spawn the docker-executor sub-agent**; executing yourself is forbidden:

```
Agent(subagent_type="testvdb:docker-executor", description="Execute {target} v{version} attack scripts", prompt="Per the agents/docker-executor.md spec, execute attack scripts in the Docker sandbox. target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}, session_id={session_id}. The four-piece environment set (X1 fix: root cause of five rounds of R1 penetration; must be set before Step 1): export TESTVDB_SCRIPTS_DIR=<absolute path of the plugin cache scripts> TESTVDB_TARGET={target} TESTVDB_DB_URL={db_url} TESTVDB_SESSION_ID={session_id}. ⛔ Run the Step 1 command immediately; do not analyze, do not check, do not read script contents. Scripts are located in the boundary_scripts/, state_scripts/, scripts/ subdirectories under SESSION_DIR and in script_*.py files. All scripts have passed syntax validation; no further checking needed.")
```

Each script gets an independent sandbox execution, processed concurrently.

**Auto-blocking**: after the executor completes, verify the output with the Bash tool (using .done markers to ensure files finished writing):
```bash
ls results/{target}/{version}/{timestamp}/output_*.log.done 2>/dev/null | wc -l
```
If the output is 0, **the orchestrator executing scripts itself is forbidden**; record in error_log and terminate the current round. **⛔ The orchestrator running Python scripts or curl commands itself in place of the executor is absolutely forbidden. If the executor fails, the current round terminates.**

**Container lifecycle management**: after the executor finishes running scripts in Step 5, **containers must not be cleaned up**. Containers must stay running until the reporter completes the Pre-Submit Gate reproduction verification (Step 8f), after which the orchestrator cleans up uniformly in Step 8j. The executor only starts and executes; it never stops. If DB state needs resetting between rounds, the orchestrator runs `docker restart` in Step 8j.

#### 8e. Collect results → EVIDENCE_BUILD + CHAIN_AUDIT (ADR-0008 evidence-chain duo)

**Update pipeline_state when done** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase EVIDENCE_BUILD --phase-data '{"EXECUTION": {"scripts_executed": N, "scripts_passed": M, "scripts_error": K}}'`

**Step 1 — mechanical candidate extraction** (fan-out dispatch list; deterministic 0 LLM):
```bash
python scripts/extract_candidates.py $SESSION_DIR
# produces candidates.jsonl (logs with VERDICT: DEFECT_FOUND → candidates; SCRIPT_ERROR excluded)
```

**Step 2 — L1 mechanical gate moved forward** (kills ~90% of historical FP patterns at 0 tokens):
```bash
python scripts/verify_live_l1.py $SESSION_DIR --target {target}
```
REFUTED candidates are removed from candidates.jsonl (recorded in verify_live_l1.json). verify-live-l2 has been removed (ADR-0008 B1: its active Docker re-testing duty is the same species as the dev-reviewer; the NEEDS_MORE_EVIDENCE evidence-supplement round covers the remaining semantic cases).

**Step 3 — dispatch evidence-builders concurrently per candidate** (1 builder/candidate):
```
Dispatch concurrently for each line of candidates.jsonl (subject to dispatch slots):
Agent(subagent_type="testvdb:evidence-builder", description="Evidence-chain build {defect_id}",
  prompt="Per the agents/evidence-builder.md spec, build the evidence chain for candidate {defect_id}. target={target}, version={version}, SESSION_DIR=$SESSION_DIR. Your defect_id={defect_id}.")
```
- Produces `evidence_chain/{defect_id}.json` + `.done` (named per candidate; concurrent writes do not conflict)
- Timed-out/no-output candidates: no retry; left for the auditor to record NEEDS_MORE_EVIDENCE

**8e.5 Defect dedup (v2.2; ADR-0008 input-source update)**

Before dispatching the auditor, the main process deduplicates candidates across rounds (same endpoint + same defect_type merge; cross-round comparison against dedup_state.json). Produces `debate_logs/stage2_deduped.json`.

**8e.7 CHAIN_AUDIT — chain-auditor single-instance close-out**

Dispatch after all builders' `.done` (cross-candidate consistency checks need the complete chain set):
```
Agent(subagent_type="testvdb:chain-auditor", description="Evidence-chain audit {target}",
  prompt="Per the agents/chain-auditor.md spec, audit all evidence chains under evidence_chain/ and produce final verdicts. target={target}, version={version}, SESSION_DIR=$SESSION_DIR.")
```
- Produces `debate_logs/chain_verdicts.json` (DEFECT / NOT_DEFECT / NEEDS_MORE_EVIDENCE + fp_evidence_source + root_cause distribution) + `.done`
- Verification: `test -f "$SESSION_DIR/debate_logs/chain_verdicts.json.done" && echo READY || echo PENDING`
- NEEDS_MORE_EVIDENCE > 0 → redispatch builders for exactly the marked defect_ids for one more evidence round (at most once), then redispatch the auditor for final verdicts; still contradictory in round 2 → NOT_DEFECT (conservative)
- **⛔ The main process never adjudicates. If the auditor times out on both rounds → all candidates conservatively NOT_DEFECT + error_log.**

**Update pipeline_state when done**: `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase REPORTING --phase-data '{"CHAIN_AUDIT": {"verdict_defect": N, "not_defect": M, "needs_more_evidence": K}}'`

**experience_handoff collection**: the auditor's `root_cause_distribution` and `fp_evidence_source_distribution` are collected into experience_handoff.json's rejection_patterns (vocabulary follows the original dev-reviewer root_cause_if_fp).

#### 8f. Dispatch the Reporter

**confirm_per_round switch (ADR-0009 §6)**: before entering this step, read the switch deterministically with Bash:
```bash
python scripts/get_setting.py mining.confirm_per_round
```
- Output `true` (default) → execute this step and 8f.5 as usual (product behavior: intra-round confirmation).
- Output `false` (experiment specialization) → **skip 8f and 8f.5**: do not dispatch the reporter; defect review and the Pre-Submit Gate are deferred; candidates.jsonl keeps accumulating (8e.5's cross-round dedup proceeds as usual); containers stay running (lifecycle rules unchanged, still handled uniformly at 8j); `pipeline_state` marks `phase=MINING_DEFER_CONFIRM`. After session termination (time up / rounds up / stalemate termination), run **unified adjudication** over all accumulated candidates: evidence-builder + chain-auditor in batch (mechanical pre-run + SOP aggregation, same spec as the intra-round path, ADR-0008) → novelty final ruling → reporter + Pre-Submit Gate, closed out in one pass.

**Update pipeline_state when done** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase DEFECT_REVIEW`

**Must use the Agent tool to spawn the reporter sub-agent**:

```
Agent(subagent_type="testvdb:reporter", description="Generate defect reports {target}", prompt="Per the agents/reporter.md spec, generate reports for the following Debate-Confirmed defects: {debate_confirmed}. session_id={session_id}, target={target}, version={version}, session_dir=results/{target}/{version}/{timestamp}. Read results/{target}/{version}/{timestamp}/pipeline_state.json for current progress")
```

**Automated output verification**: after the reporter completes, verify the output with the Bash tool:
```bash
ls results/{target}/{version}/{timestamp}/defects/defect-*.md 2>/dev/null | wc -l
```
If the output is 0, the reporter did not execute properly; record in error_log.

**Evidence-chain verification requirements**: every defect-N.md the reporter generates must contain a complete evidence chain:
- **Ring 2 (document reference)**: source_url must be reachable; doc_version must match the target major.minor
- **Ring 4 (source reference)**: if the defect involves a specific code path, a github_url must be included

**Pre-Submit Gate reproduction verification**: the reporter must run reproduction verification for every confirmed defect (see agents/reporter.md's Pre-Submit Gate section); only 100%-reproduced defects produce final reports.

#### 8f.5 Per-defect full review (added v2.2 — end-of-round item-by-item audit of reporter output)

**⛔ Iron law: the main process only orchestrates.** The main process runs `python scripts/verify_defects.py` auditing every defect-N.md:
1. Evidence-chain completeness (are Rings 1/2/3 all present)
2. Severity calibration (re-confirmed from the execution logs)
3. Script-error exclusion (check SCRIPT_ERROR markers)
4. False-positive identification (VERDICT line vs report claims)

Produces `defect-review.md`, marking each defect CONFIRMED / FALSE_POSITIVE / NEEDS_IMPROVEMENT.
FALSE_POSITIVE → delete the corresponding defect-N.md. NEEDS_IMPROVEMENT → send back to the reporter for rewrite (at most once).

#### 8g. Save state

**Update pipeline_state when done** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase STATE_SAVE`
At each round's end save mine_state.json + coverage.json + experience_handoff.json + pipeline_state.json.

**pipeline_state.json (v3 cross-turn state machine, ADR-0004):**

Managed by the `scripts/pipeline_state.py` CLI; hand-constructing the JSON is forbidden. Schema reference: [pipeline_state.py](scripts/pipeline_state.py)'s `PipelineState.create()`.

After each sub-step completes, call `pipeline_state.py advance --phase <NEXT> [--phase-data '...']`. Phase transitions are validated against a hardcoded transition map (invalid jumps → InvalidTransition error). Global state counters update via `pipeline_state.py mutate --total-defects N --coverage P...`. On cross-turn recovery, `reconstruct_context.py` reads this file to determine the breakpoint.

### Inter-agent communication reliability (.done marker files)

Since sub-agents are dispatched asynchronously via the Agent tool, all inter-agent communication goes through the filesystem. To ensure atomicity and visibility of file writes:

1. **Sub-agent output spec**: write the output file first; when complete, create the same-named `.done` marker file
2. **Orchestrator check spec**: **must** check for the `.done` marker file's existence (not merely the output file — the file may be mid-write)
3. **Check command**: `test -f "{file}.done" && echo "READY" || echo "PENDING"`
4. **Timeout handling**: output file exists but `.done` missing for over 60 seconds → the sub-agent is stuck; trigger the timeout
5. **Orchestrator write spec**: write a `.tmp` temp file first; rename + touch `.done` on completion

**experience_handoff.json write logic:**
- Record this round's key findings: debate_confirmed's endpoint distribution, rejection-reason classification, newly discovered high-value attack strategies
- Record the current adjudication-chain state: L1 refuted / verdict_defect / not_defect / needs_more_evidence counts (ADR-0008)
- Enables quickly understanding current progress at the next session or after context-compaction recovery

**experience_handoff.json template** (the orchestrator writes with the Write tool):
```json
{
  "session_id": "{session_id}",
  "target": "{target}",
  "version": "{version}",
  "round": {current_round},
  "timestamp": "{ISO 8601}",
  "key_findings": [
    {"endpoint": "...", "defect_type": "...", "confidence": 0.0, "summary": "..."}
  ],
  "chain_stats": {
    "candidates": 0,
    "l1_refuted": 0,
    "verdict_defect": 0,
    "not_defect": 0,
    "needs_more_evidence": 0
  },
  "rejection_patterns": [
    {"endpoint": "...", "reason": "by-design|false_positive|irreproducible|insufficient_evidence"}
  ],
  "high_value_endpoints": ["..."],
  "exhausted_endpoints": ["..."],
  "next_action": "continue_mining|stalemate|terminate"
}
```

**coverage.json template** (the orchestrator writes with the Write tool):
```json
{
  "session_id": "{session_id}",
  "target": "{target}",
  "version": "{version}",
  "round": {current_round},
  "timestamp": "{ISO 8601}",
  "endpoint_coverage": {
    "{endpoint}": {
      "constraints_tested": 0,
      "constraints_total": 0,
      "defects_found": 0,
      "last_tested_round": 0
    }
  },
  "overall_coverage_pct": 0.0,
  "core_crud_coverage_pct": 0.0
}
```

#### 8h. Analyze this round's output
- Vote-divergence pattern analysis
- Rejection-reason classification (by-design / false positive / irreproducible / insufficient evidence)
- Endpoint coverage update
- Generate reflection_context for the next round

### v2.0 strategy extraction (evolution.enabled=true)

After each round ends (or executed collectively at Step 9), run:
```bash
python scripts/strategy_extractor.py "results/{target}/{version}/{timestamp}" {target}
```
Strategy extraction logic:
1. Read this round's experience_handoff.json
2. Extract strategy patterns of confirmed_defects → generalize → merge
3. New strategies → write to strategy_registry (global + per-DB)
4. Existing strategies → update performance counts + adjust confidence
5. Append to evolution_log.jsonl for auditing

#### 8i. Check termination conditions
Terminate the loop when any of the following holds:
1. 5 consecutive rounds with no new defects
2. Contract coverage ≥ 95%
3. max_rounds reached (and > 0)
4. min_defects reached

#### 8j. Inter-round container management
- **Continue to the next round**: restart the DB container to reset state (`docker restart testvdb-{target}-${TESTVDB_SESSION_ID:-standalone}`), keeping data volumes
- **Terminate the loop**: run full cleanup (`docker compose -f docker/{target}.yml down -v`), releasing all resources

### Step 9: Issue drafts + summary report + mandatory container cleanup

**⛔ Absolutely forbidden: the main process or any agent directly submitting Issues to GitHub repositories. All output stays on the local filesystem.**

1. **Generate issue-format drafts** (added v2.2):
   ```bash
   mkdir -p results/{target}/{version}/{timestamp}/issues
   ```
   For every CONFIRMED defect passing 8f.5 review, generate `issues/issue-{N}-{slug}.md`, containing the full Bug Report format (Title, Description, Version, Steps to Reproduce, Expected/Actual Behavior, Impact, Environment, MRE path). Annotate at the bottom: "local draft; requires manual review before manual submission".

2. Generate the `summary.md` + `defect-review.md` summary reports
3. **Mandatory container cleanup**: run the following to clean up all Docker containers and networks created this session:
   ```bash
   docker compose -f docker/{target}.yml down -v --remove-orphans
   docker network rm testvdb-net-${TESTVDB_SESSION_ID:-standalone} 2>/dev/null || true
   ```
4. Verify cleanup completed: `docker ps --filter "name=testvdb-{target}" --format "{{.Names}}"` should output nothing
5. Update `.session.lock`'s status to `completed`

### Stalemate handling (triggered after 5 consecutive rounds with no new defects)
1. Dispatch the Knowledge Extractor to re-search for documentation changes + new issues + community discussion
2. Feed all search results to the Judge Agents to re-examine last round's candidate defects
3. Adjust the attack strategies of the attack agents for low-coverage endpoints
4. If still nothing found → terminate

### Zero-defect ruling
After running all rounds with zero output → annotate `ZERO_DEFECT` in session_metadata.json and generate a diagnostic report:
- which endpoints were tested, which constraints were missed
- coverage analysis
- suggested improvement directions

---

## Lifecycle management

> Error handling, context-compaction protection, progress visibility, multi-DB parallelism — see `agents/orchestrator-lifecycle.md`.

---

## Data-flow specification

```
Orchestrator
  │
  ├──▶ [Phase 0: Strategic Intelligence — v2.1 NEW]
  │     │
  │     ├──▶ Issue Miner ──▶ issue_corpus.json + commit_corpus.json
  │     │                          │
  │     ├──▶ Bug Shape Extractor ◀─┘
  │     │           │
  │     │           ▼
  │     │     bug_shapes.json + classified_issues.json + developer_cognition.json
  │     │           │
  │     ├──▶ Threat Modeler ◀──────┘
  │     │           │
  │     │           ▼
  │     │     threat_model.json (attack priorities + cognitive blindspots)
  │     │
  ├──▶ Knowledge Extractor ──▶ raw_knowledge.json
  │                                      │
  ├──▶ Contract Formalizer ◀─────────────┘
  │           │
  │           ▼
  │     structured_contract.json + sdk.version + available_tags
  │           │
  ├──▶ Attack Trio (concurrent) ◀── contract + reflection_context + threat_model + cognitive_blindspots
  │     boundary │ state │ semantic
  │           ▼
  │     test_scripts[]
  │           │
  ├──▶ Debate Stage 1 (the orchestrator runs the automated review itself: dedup + syntax validation + constraint validation)
  │           │
  │           ▼
  │     approved_scripts[]
  │           │
  ├──▶ Executor (concurrent) ◀── approved_scripts[]  [containers stay running]
  │           │
  │           ▼
  │     execution_results[]
  │           │
  ├──▶ extract_candidates (mechanical extraction) ──▶ verify_live_l1 (L1 mechanical gate, 0 tokens)
  │           │
  │           ▼
  ├──▶ evidence-builder × N (concurrent per candidate, ADR-0008) ◀── candidates.jsonl + contract + src clone
  │     step1: doc verification + execution-evidence review + chain tracing
  │     step2: source forensics
  │           │
  │           ▼
  ├──▶ chain-auditor (single-instance close-out) ──▶ chain_verdicts.json
  │     (DEFECT/NOT_DEFECT/NEEDS_MORE_EVIDENCE + fp_evidence_source + root_cause)
  │           │
  ├──▶ Reporter ◀── confirmed_defects[]  [reuses running containers for the Pre-Submit Gate]
  │           │
  │           ▼
  │     defect-N.md + MRE + summary.md
  │           │
  └──▶ Container cleanup (docker compose down -v)
```

---

## Output artifacts

```
results/{target}/{version}/{timestamp}/
├── defects/           # defect reports (defect-1.md, defect-N.md)
├── summary.md          # this round's summary
├── debate_logs/        # debate logs (stage1.json, stage2.json)
├── structured_contract.json  # contract
├── raw_knowledge.json    # raw knowledge
├── mine_state.json     # state snapshot
├── coverage.json       # coverage tracking
├── session_metadata.json     # session metadata
└── experience_handoff.json   # experience handoff

intelligence/{target}/                # v2.1 strategic intelligence layer
├── issue_corpus.json                 # raw issue corpus
├── commit_corpus.json                # raw commit/PR corpus
├── classified_issues.json            # three-way classification (positive/negative/invalid)
├── bug_shapes.json                   # root-cause patterns
├── developer_cognition.json          # developer cognition boundary analysis
└── threat_model.json                 # threat model + cognitive blindspots + attack priorities
```
