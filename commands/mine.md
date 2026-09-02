---
description: Launch the vector-database automated defect-mining pipeline
allowed-tools: Read, Write, Bash, Grep, Glob, Agent
---

# /testvdb:mine

Launch the vector-database automated defect-mining pipeline.

---

## ⚠️ Architecture constraint (CRITICAL — for technical reasons)

**A technical limitation of the Claude Code plugin system: sub-agents cannot reliably dispatch grandchild agents.**

This means:
- ✅ main process → `testvdb:knowledge-extractor` (works — the main process dispatches directly)
- ✅ main process → `testvdb:orchestrator` (works — but the orchestrator's internal dispatching of grandchildren is unreliable)
- ❌ orchestrator (sub) → `testvdb:knowledge-extractor` (grandchild) (unreliable — the agent_type gets lost as "unknown")

**Therefore this command's design: the main process directly plays the orchestrator role, dispatching sub-agents step by step per the SOP in `agents/orchestrator.md`.**
The `testvdb:orchestrator` agent definition is retained as an SOP reference document.

---

## ⛔ Core iron law

**The main process only ever orchestrates; it never executes. Violating any single item below fails the pipeline immediately.**

| Forbidden | Correct approach |
|---------|---------|
| ❌ Using WebSearch/WebFetch to crawl documentation | ✅ `Agent(subagent_type="testvdb:knowledge-extractor")` |
| ❌ Generating structured_contract.json yourself | ✅ `Agent(subagent_type="testvdb:contract-formalizer")` |
| ❌ Writing Python attack scripts yourself | ✅ `Agent(subagent_type="testvdb:attack-boundary/state/semantic")` |
| ❌ Running Python scripts or curl yourself | ✅ `Agent(subagent_type="testvdb:docker-executor")` |
| ❌ Judging defect validity yourself | ✅ `Agent(subagent_type="testvdb:chain-auditor")` (ADR-0008) |
| ❌ Generating defect reports yourself | ✅ `Agent(subagent_type="testvdb:reporter")` |

**The main process uses only these tools for orchestration:** `Read` (read files), `Write` (write state files), `Bash` (verify outputs), `Grep` (search), `Glob` (match), `Agent` (dispatch sub-agents). Across turns, the Stop hook (`pipeline_gate.py`) drives everything; the main process needs no scheduling tool.

> **⚠️ Dispatch tool discipline (CRITICAL — avoid repeating historical mistakes)**: dispatching `testvdb:*` sub-agents may **only** use `Agent(subagent_type="testvdb:xxx", ...)`.
> - ❌ **`TaskCreate` is forbidden**: it does not recognize plugin agent_types; the dispatch record reads `Spawning agent: unknown (inherit)`, the task stays `pending` forever (a ghost entry that `TaskStop` cannot delete), and **no real agent executes behind it**.
> - ✅ `Agent(subagent_type=...)` is a **core built-in tool** — no `ToolSearch` loading needed (ToolSearch only indexes the deferred tool list; **not found ≠ unavailable**); call it directly.
> - ✅ Outside the v2.1.166 regression environment, plugin subagents genuinely work (measured 2026-06-17: reporter-mre dispatched successfully; weaviate's 3 confirmed defects produced `mre/*.done`).
> - `TaskCreate`/`TaskList` etc. are only for OMC task tracking; **never dispatch plugin agents with them**. Probe dispatching also uses only `Agent(subagent_type=...)`; do not use TaskCreate probes.

---

## Usage

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, `pgvector`, `meilisearch`, or `chroma` |
| `<version>` | Yes | — | Target version number |
| `--max-rounds N` | No | `5` | Maximum mining rounds. `0` = unlimited |
| `--min-defects N` | No | `1` | Minimum defect output requirement |
| `--intel true\|false` | No | `auto` | Intelligence-stage control. `true` = force recollection; `false` = disable collection (C-boundary: none → error, expired → use + warn); absent = `auto` (valid cache → reuse, otherwise collect) |
| `--contract true\|false` | No | `auto` | Contract-stage control. `true` = force regeneration; `false` = disable generation (C-boundary: none → error, expired → use + warn); absent = `auto` (valid cache → reuse, otherwise generate) |
| `--new` | No | — | Force a new session, ignoring the automatic RESUME of unfinished runs (for when old interruptions are obsolete) |

---

## Execution model: the Stop-hook-driven cross-turn loop

> **📖 Full SOP references**: `agents/orchestrator.md` (phase details, voting rules, error handling), `skills/pipeline/SKILL.md` (the six-phase pipeline spec). This file keeps only the orchestration/scheduling commands and does not repeat SOP descriptions.

This command uses a **Stop-hook-driven cross-turn iteration model** (`scripts/hooks/pipeline_gate.py` wired as the Stop hook in `.claude/settings.local.json`; reference: ralph's "boulder never stops"). Each mining round is an independent turn:

```
Turn 1 (FRESH_START):  Steps 1-7 (setup) + Round 1 (8a→8j) + update state → actively end the turn
                          ↓ Stop hook: phase != DONE → exit 2 → the harness forces a new turn
Turn N (RESUME):       reconstruct_context.py → Round N (8a→8j) + update state → actively end the turn
                          ↓ (same as above)
Final Turn:            termination condition met → phase=DONE → the Stop hook releases (exit 0) → Steps 9-10
```

**Mechanism**: at each round's end the main process updates `pipeline_state.json` (`phase=ROUND_START`, `current_round+1`) and then **actively ends the turn**. The harness's Stop hook calls `pipeline_gate.py`:
- `phase != DONE` → `exit 2` (forces Claude to continue in a new turn)
- `phase == DONE` + quality gate passed → `exit 0` (allows stopping)

**State persistence**: `pipeline_state.json` (v3 schema) is the sole cross-turn state source. It is updated immediately after each phase completes, ensuring resume precision down to the step.

> ⚠️ **autoCompact is required**: the cross-turn loop depends on `autoCompactEnabled: true` in `~/.claude/settings.json` (compacts context between turns). Preflight checks and warns about this.

---

## Execution entry

### Entry judgment

At the start of every turn, run the entry judgment first. **Main process: if the `/mine` command line contains `--new`, run `export TESTVDB_FORCE_NEW=1` before the script below** (force_new forces FRESH_START and clears any leftover `.resume_target` marker).

```bash
python -c "
import sys, os, json
# Lock the plugin root (same logic as Step 1; guards against cwd drift)
root = os.environ.get('TESTVDB_PLUGIN_ROOT', '')
if not (root and os.path.isdir(root)):
    cur = os.getcwd()
    for _ in range(7):
        if os.path.isfile(os.path.join(cur, 'commands', 'mine.md')):
            root = cur; break
        parent = os.path.dirname(cur)
        if parent == cur: break
        cur = parent
if not root:
    print(json.dumps({'decision':'FRESH_START','reason':'no plugin root'}, ensure_ascii=False)); sys.exit(0)
os.chdir(root)
sys.path.insert(0, os.path.join(root, 'scripts'))
import _entry_dispatch as ed
# Entry judgment: scan all unfinished (loop+setup), resume the latest; --new forces fresh; the resume command's .resume_target marker takes priority
result = ed.dispatch('', '', force_new=os.environ.get('TESTVDB_FORCE_NEW') == '1')
print(json.dumps(result, ensure_ascii=False))
"
```

Read the dispatch result JSON:
- `decision=RESUME` (with `session_dir`/`phase`/`target`/`version`) → run [Loop Turn: Resume Round](#loop-turn-resume-round) from `session_dir`
  - If `target`/`version` does not match this `/mine <db> <version>` request (Turn-1 scenario) → print a notice: "the latest unfinished run is `{target}/{version}`, which does not match the requested `{db}/{version}`; resume it or start new? Suggest `/testvdb:resume {session_id}` or add `--new`"
- `decision=FRESH_START` → run [Turn 1: Setup + First Round](#turn-1-setup--first-round)
  - If `incomplete` is non-empty (an unfinished run exists for the same target/version) → print a notice: "detected unfinished `{session_id}` (`{phase}`); suggest `/testvdb:resume {session_id}`; a new session was created (use resume if you want the old one)"

> **`--new`**: when the main process parses `--new`, it exports `TESTVDB_FORCE_NEW=1` before the entry judgment (force_new=True forces FRESH_START while still returning `incomplete` for awareness).
> **Dispatch discipline**: substantive resumed work still goes through `Agent(subagent_type=...)`; `TaskCreate` is disabled (see "dispatch tool discipline" in this file).

---

## Turn 1: Setup + First Round

> Executed only on FRESH_START. After all initialization work, enter the first mining round.

### Step 1: Parse arguments
- Verify `target` ∈ {milvus, qdrant, weaviate, pgvector, meilisearch, chroma}
- Parse `version`, `max_rounds`, `min_defects`
- **Version normalization**: unify to `vX.Y.Z` (user input `1.38.0` or `v1.38.0` both normalize to `v1.38.0`), used for session_id and the `results/{target}/{version}/` directory name — history saw `2.6.17`/`1.38.0` (no v) mixed with `v1.18.2` (with v), causing scripts to fail finding outputs stored in the other format:
```bash
version="${version#v}"   # strip a possible v prefix
version="v${version}"    # uniformly add the v prefix back
```
- Determine `PROJECT_ROOT` (**`git rev-parse --show-toplevel` is forbidden**: the user's home directory `~/` is itself a git repo; when claude is launched from a parent directory it drifts to `~/`, writing `results/` to the wrong root — the root cause of the historical directory misplacement):
```bash
# Verified lock-in: must be the testvdb plugin root containing commands/mine.md
PROJECT_ROOT="${TESTVDB_PLUGIN_ROOT:-}"
if [ -z "$PROJECT_ROOT" ] || [ ! -f "$PROJECT_ROOT/commands/mine.md" ]; then
  cur="$PWD"
  for _ in 1 2 3 4 5 6; do
    [ -f "$cur/commands/mine.md" ] && PROJECT_ROOT="$cur" && break
    cur="$(dirname "$cur")"
  done
fi
if [ -z "$PROJECT_ROOT" ] || [ ! -f "$PROJECT_ROOT/commands/mine.md" ]; then
  echo "FATAL: cannot find the testvdb plugin root (containing commands/mine.md). Launch from the TestVDB directory, or set TESTVDB_PLUGIN_ROOT."; exit 1
fi
cd "$PROJECT_ROOT"          # lock cwd; all subsequent relative paths results/... are relative to this root
export TESTVDB_PLUGIN_ROOT="$PROJECT_ROOT"   # for hook scripts (_session_utils/pipeline_gate) to read
echo "[TestVDB] PROJECT_ROOT=$PROJECT_ROOT"
```

### Step 2: Precondition checks
Check Docker/Python/disk/network yourself:
```bash
python scripts/preflight.py
# Set the container-version env per target (avoids compose defaulting to old versions, e.g. chroma 0.6.3 breaking mine 1.5.9's server-version-vs-scripts-API match)
# Image tag format per target: chroma/weaviate without v (1.5.9), milvus/qdrant with v (v2.4.0)
case "$TARGET" in
  chroma)    export CHROMA_VERSION="${VERSION#v}" ;;
  milvus)    export MILVUS_VERSION="$VERSION" ;;
  qdrant)    export QDRANT_VERSION="$VERSION" ;;
  weaviate)  export WEAVIATE_VERSION="${VERSION#v}" ;;
esac
docker compose -f docker/crawl4ai.yml up -d --wait 2>/dev/null || true
```

**Auto-compact check**:
```bash
python -c "
import json, sys, os
settings_path = os.path.expanduser('~/.claude/settings.json')
try:
    with open(settings_path, encoding='utf-8') as f:
        s = json.load(f)
    if s.get('autoCompactEnabled'):
        print('[Preflight] autoCompactEnabled: OK')
    else:
        print('[Preflight] autoCompactEnabled: MISSING — the multi-round pipeline will break on context overflow (one of the root causes of restarting from scratch after compaction)')
        print('[Preflight] Fix: set \"autoCompactEnabled\": true in ~/.claude/settings.json')
        if os.environ.get('TESTVDB_ALLOW_NO_AUTOCOMPACT') == '1':
            print('[Preflight] TESTVDB_ALLOW_NO_AUTOCOMPACT=1 → continuing (at your own risk; single-round usable)')
        else:
            print('[Preflight] Aborting. Set TESTVDB_ALLOW_NO_AUTOCOMPACT=1 to force continuation.')
            sys.exit(1)
except FileNotFoundError:
    print('[Preflight] ~/.claude/settings.json not found; skipping the autoCompact check')
except json.JSONDecodeError:
    print('[Preflight] settings.json malformed; skipping the autoCompact check')
"
```

### Step 3: Contract intelligent consumption (batch D, D-judgment)

The `--contract` parameter decides the contract stage's behavior (spec decision 4: exists → TTL → validity → target/version match). Logic identical to the `/testvdb:contract` command.

**Smart judgment (no `--contract`, default auto)**:
```bash
python scripts/check_cache.py contract "results/{target}/{version}" {target} {version} --ttl {knowledge.cache_ttl_hours}
```
- **USABLE** (exit 0) → skip contract generation, go straight to [Step 7](#step-7-initialize-state) (pure mining)
- **MISSING / STALE / INVALID** → dispatch contract generation (Step 4 → Step 5 → Step 6)
- **MISMATCH** (target/version mismatch) → error out

**`--contract true`**: skip check_cache; forcibly dispatch Step 4 → Step 5 → Step 6 (regenerate)

**`--contract false` (C-boundary)**:
- **MISSING** → error out ("contract missing; --contract false skips generation; run `/testvdb:contract {target} {version}` first")
- **STALE / INVALID** → use the existing contract + warning ("the contract may be expired/invalid; --contract false skips refresh"), continue to Step 7
- **USABLE** → normal use

**Passport hash verification** (when `material_passport.enabled=true` and the contract stage ran Steps 4-5):
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```

### Step 3.5: Cross-session strategy injection prep (when evolution.enabled=true)

Read the Strategy Registry:
```bash
python scripts/strategy_injector.py {target} --text-only
```

### Step 3.6: Historical intelligence collection (when intelligence.enabled=true)

**⛔ Iron law: the main process only orchestrates; it never executes.**

If `intelligence.enabled=false`, skip all of Step 3.6.

**Read the intelligence settings**:
```bash
python -c "
import json
with open('settings.json', encoding='utf-8') as f:
    c = json.load(f).get('intelligence', {})
print(f'INTEL_TW={c.get(\"time_window_months\", 24)}')
print(f'INTEL_MI={c.get(\"max_issues\", 500)}')
print(f'INTEL_MC={c.get(\"max_commits\", 200)}')
print(f'INTEL_TTL={c.get(\"cache_ttl_hours\", 720)}')
"
```

#### 3.6a: Intelligence intelligent consumption (batch D, D-judgment)

The `--intel` parameter decides the intelligence stage's behavior (spec decision 4: exists → TTL → validity). Logic identical to the `/testvdb:intel` command.

**Smart judgment (no `--intel`, default auto)**:
```bash
python scripts/check_cache.py intel "intelligence/{target}" {target} --ttl {INTEL_TTL}
```
- **USABLE** (exit 0) → skip intelligence collection, go straight to [3.6e](#36e-load-the-intelligence-summary) (pure mining)
- **MISSING / STALE / INVALID** → dispatch intelligence collection (3.6b → 3.6c → 3.6d)
- **MISMATCH** → error out

**`--intel true`**: skip check_cache; forcibly dispatch 3.6b → 3.6c → 3.6d (recollect)

**`--intel false` (C-boundary)**:
- **MISSING** → error out ("intelligence missing; --intel false skips collection; run `/testvdb:intel {target}` first")
- **STALE / INVALID** → use existing intelligence + warning ("intelligence may be expired/invalid; --intel false skips refresh"), continue to 3.6e
- **USABLE** → normal use

#### 3.6b: Dispatch issue-miner
```
Agent(subagent_type="testvdb:issue-miner", description="Collect {target} historical Issues and Commits",
  prompt="Per the agents/issue-miner.md spec, collect historical Issues and merged fix PRs for {target}. Input parameters: target={target}, version={version}, intelligence_dir=intelligence/{target}/, time_window_months={INTEL_TW}, max_issues={INTEL_MI}, max_commits={INTEL_MC}. Write the results to intelligence/{target}/issue_corpus.json and intelligence/{target}/commit_corpus.json.")
```
On failure → log a warning and skip the subsequent 3.6c/3.6d.

#### 3.6c: Dispatch bug-shape-extractor
```
Agent(subagent_type="testvdb:bug-shape-extractor", description="Extract {target} historical Bug Shapes",
  prompt="Per the agents/bug-shape-extractor.md spec, classify and extract root-cause patterns from intelligence/{target}/issue_corpus.json and intelligence/{target}/commit_corpus.json. Write the results to intelligence/{target}/classified_issues.json, bug_shapes.json, developer_cognition.json.")
```

#### 3.6d: Dispatch threat-modeler
```
Agent(subagent_type="testvdb:threat-modeler", description="Build the {target} threat model",
  prompt="Per the agents/threat-modeler.md spec, build the threat model from bug_shapes.json, classified_issues.json, developer_cognition.json. Write the result to intelligence/{target}/threat_model.json.")
```

#### 3.6e: Load the intelligence summary
```bash
python -c "
import json
with open('intelligence/{target}/threat_model.json', encoding='utf-8') as f:
    tm = json.load(f)
print(json.dumps({
    'blindspot_count': len(tm.get('cognitive_blindspots', {}).get('blindspots', [])),
    'high_priority_areas': [a['area'] for a in tm.get('attack_surface', {}).get('high_priority_areas', [])],
    'top_blindspots': [b['blindspot_id'] for b in tm.get('cognitive_blindspots', {}).get('blindspots', [])[:3]],
}, indent=2, ensure_ascii=False))
" 2>/dev/null || echo "THREAT_MODEL_NOT_AVAILABLE"
```

### Step 4: Dispatch the Knowledge Extractor (Task 4a: reuse + mark on failure)

> **P3-20 glm proxy mode (env flag triggers early)**: when preflight's `check_glm_proxy` detects the `TESTVDB_PROXY=glm` env flag, the pipeline knows at startup it is in a glm proxy environment → knowledge-extractor goes straight to the Task 4a fallback (sparing N Stop-hook retries before degrading). Standard proxy environments still trigger via the "after agent failure" path. Users in glm proxy environments set `TESTVDB_PROXY=glm` before SessionStart.

```
# First check whether an older version's knowledge can be reused
OLD_VERSION=$(find results/{target} -maxdepth 2 -name "raw_knowledge.json" -printf "%T@ %p\n" 2>/dev/null | sort -rn | head -1 | cut -d" " -f2- | sed 's|/raw_knowledge.json||')

if [ -n "$OLD_VERSION" ] && [ -f "$OLD_VERSION/raw_knowledge.json" ]; then
  OLD_VER=$(basename "$OLD_VERSION" | sed 's/^v//')
  echo "[Knowledge Extractor] degrading: reusing v${OLD_VER} knowledge from an older version (under glm proxy the agent gets frequent HTTP 400s)"
  # Task 4a: reuse the older version + forced marker
  cp "$OLD_VERSION/raw_knowledge.json" "results/{target}/{version}/raw_knowledge.json"
  [ -f "$OLD_VERSION/deployment_meta.json" ] && cp "$OLD_VERSION/deployment_meta.json" "results/{target}/{version}/deployment_meta.json"
  # mark KNOWLEDGE_DEGRADED (written into mine_state.json later)
  export KNOWLEDGE_DEGRADED="true"
  export OLD_KNOWLEDGE_VERSION="$OLD_VER"
else
  # no older version to reuse; dispatch normally
  Agent(subagent_type="testvdb:knowledge-extractor", description="Extract {target} {version} documentation knowledge",
    prompt="Per the agents/knowledge-extractor.md spec, extract API documentation knowledge for {target} {version}. Write the result to results/{target}/{version}/raw_knowledge.json (SDK/Docker info goes separately to deployment_meta.json, v3.4 §B)")

  # after dispatch, check success (whether raw_knowledge.json was created/updated)
  if [ ! -f "results/{target}/{version}/raw_knowledge.json" ] || [ ! -s "results/{target}/{version}/raw_knowledge.json" ]; then
    echo "[Knowledge Extractor] failed: could not extract knowledge and no older version is available for reuse"
    exit 1
  fi
fi
```

**Verification:** `ls -la results/{target}/{version}/raw_knowledge.json results/{target}/{version}/deployment_meta.json`

**Step 4.5: OpenAPI spec prefetch + mechanical coverage verification (root-cause fix 2026-08-20)**

Measured on pilot qdrant v1.18.2: the extractor fetched only 12 endpoint pages yet self-reported `doc_coverage_pct: 100% (70/70)` — Step 6b's "skip when the spec is absent" was an unconditional escape hatch (the qdrant spec was never fetched), and LLM self-reported coverage had no verification. Fixed as two deterministic main-process actions:

```bash
# (a) prefetch the spec before dispatching the extractor (qdrant/weaviate have rules; other targets exit code 3 = no rule, non-blocking)
python scripts/fetch_openapi_spec.py {target} {version}

# (b) after the extractor returns, verify mechanically: compute coverage with spec paths as the denominator, overwriting the LLM's self-reported number;
#     the report lands at results/{target}/{version}/doc_coverage_report.json
python scripts/validate_doc_coverage.py {target} {version}
```

- doc_coverage_report.json's `missing_endpoints` is the deterministic basis for the extractor's supplementary fetches (fed to the extractor's retry prompt or manual review)
- spec missing (fetch exit 1/3) → skip verification and log a warning, non-blocking (preserves the original Step 6b semantics, but "not fetched" goes from silent to explicit)

**Task 4a: when reusing an older version, mark `KNOWLEDGE_DEGRADED` when writing mine_state.json at Step 7**:
```python
if os.environ.get("KNOWLEDGE_DEGRADED") == "true":
    mine_state["knowledge_degraded"] = {
        "reused_from": os.environ.get("OLD_KNOWLEDGE_VERSION"),
        "reason": "knowledge-extractor agent failed (glm proxy HTTP 400)",
        "error_log": "Agent dispatch failed or timed out; reused older-version knowledge may be stale"
    }
```

### Step 5: Dispatch the Contract Formalizer
```
Agent(subagent_type="testvdb:contract-formalizer", description="Formalize the {target} v{version} API contract",
  prompt="Per the agents/contract-formalizer.md spec, convert results/{target}/{version}/raw_knowledge.json into structured_contract.json (every constraint graded with level, Rule 2.7; v3.4). Write the result to results/{target}/{version}/structured_contract.json")
```
**Verification:** `ls -la results/{target}/{version}/structured_contract.json`

**Step 5.5: spec parameter mechanical backfill (mechanized 2026-08-21; replaces the rerun pilot's manual patching)**

Measured in the rerun pilot: the formalizer consumes raw_knowledge.md's "Spec-derived Endpoints" skeleton entries
(endpoints mechanically filled at Step 4.5) unreliably — 65 endpoints with 0 parameters. Rather than teaching the LLM to read
skeletons, after the formalizer the main process backfills deterministically (same philosophy as Step 4.5 — 0 tokens,
no fabrication, fields marked
`source: openapi (mechanical backfill)`):

```bash
py -3 scripts/enrich_contract_from_spec.py results/{target}/{version} --fill-missing-fields
# exit 0 = backfill done or spec unavailable (the spec exists only if Step 4.5's fetch ran); includes passport re-signing
```

### Step 6: Contract gate checks
Check that `structured_contract.json`'s core CRUD endpoint coverage ≥ 90%. Failure → print missing endpoints + terminate.

**Passport hash verification** (when material_passport.enabled=true):
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```

**Step 6.2: Contract documentation-asset preflight (Rule P1.0, 2026-09-02 — consumed by evidence-builder layer A's first two layers)**:
```bash
python scripts/preflight_contract_docs.py "results/{target}/{version}/structured_contract.json"
```
Produces the sidecar `doc_preflight.json` (the contract file itself is untouched; 0-LLM, 8-way concurrent after URL dedup, seconds).
- exit 0 = PASS or skipped (`TESTVDB_OFFLINE=1` skips; the builder falls back to WebFetch)
- exit 1 = dead/mismatched documentation exists → **record in pipeline_state without interrupting** (documentation death is an
  environmental fact; the evidence-builder forensics it honestly; no redispatch, no hard block)

**Step 6.5: Strategy pre-binding (v3.4 D2; deterministic main-process step — 0 LLM)**:
```bash
python scripts/bind_strategies.py "results/{target}/{version}/structured_contract.json"
```
- exit 1 (level lint failure: the contract lacks `level` fields) → redispatch contract-formalizer (Rule 2.7 was not executed)
- normal → every constraint carries `bound_strategies` + the top-level `_strategy_binding` summary; attack agents generate directly per the binding list (no longer matching strategies themselves)

### Step 7: Initialize state

- Generate `session_id`: `{target}-{version_short}-{counter}` (sanitize: `[a-z0-9-]`, ≤63 chars)
- **Generate TIMESTAMP (the single authoritative entry)**: all subsequent `{timestamp}` references use this variable; ad-hoc generation is forbidden — history saw three formats mixed (`2026-06-06T14-26-53Z` ISO-style / `20260611T013818` compact-T / `20260614-173709` compact-dash), the root cause being no unified entry. The format is fixed as `YYYY-MM-DDTHH-MM-SSZ` (ISO-style, colons→dashes, NTFS-safe, lexicographically sortable):
```bash
TIMESTAMP="$(python -c "from datetime import datetime,timezone;print(datetime.now(timezone.utc).strftime('%Y-%m-%dT%H-%M-%SZ'))")"
SESSION_DIR="results/${target}/${version}/${TIMESTAMP}"
mkdir -p "$SESSION_DIR"
export TESTVDB_TIMESTAMP="$TIMESTAMP" TESTVDB_SESSION_DIR="$SESSION_DIR"
echo "[TestVDB] TIMESTAMP=$TIMESTAMP SESSION_DIR=$SESSION_DIR"
```
- Create the session directory `$SESSION_DIR` (i.e. `results/{target}/{version}/{TIMESTAMP}/`)
- Write `mine_state.json` and `.session.lock`
- **Initialize `pipeline_state.json` via the CLI (v3 schema, ADR-0004)**:

```bash
python scripts/pipeline_state.py init \
    --target "{target}" \
    --version "{version}" \
    --session-dir "$SESSION_DIR" \
    --max-rounds {max_rounds} \
    --min-defects {min_defects} \
    --project-root "{PROJECT_ROOT}"
```

> Equivalent to the following v3 schema JSON (for reference; no need to hand-write):

```python
# pipeline_state.json v3 — cross-turn state machine
{
    "version": 3,
    "session_id": "{session_id}",
    "target": "{target}",
    "version_target": "{version}",
    "current_round": 1,
    "max_rounds": {max_rounds},
    "min_defects": {min_defects},
    "phase": "ROUND_START",
    "phase_step_index": 0,
    "turn_type": "setup",
    "project_root": "{PROJECT_ROOT}",
    "session_dir": "results/{target}/{version}/{TIMESTAMP}",
    "timestamp_dir": "{TIMESTAMP}",
    "phases_completed": [],
    "phase_data": {},
    "global_state": {
        "total_defects_confirmed": 0,
        "consecutive_no_defect_rounds": 0,
        "overall_coverage_pct": 0.0,
        "docker_container_running": False
    },
    "error_log": [],
    "timestamps": {
        "session_started": "{ISO_8601}",
        "last_phase_change": "{ISO_8601}"
    }
}
```

- Set the environment variable: `export TESTVDB_SESSION_ID="{session_id}"`

### Step 8: First mining round (Round 1)

> **Round 1 runs directly inside Turn 1, not across turns.** Start from [Run one full mining round](#run-one-full-mining-round).
>
> When done:
> - If a termination condition is met → run [Final Turn: Cleanup](#final-turn-cleanup) directly in the current turn
> - If continuing → update `pipeline_state.json`: first `advance --phase ROUND_START` (with the STATE_SAVE → ROUND_START transition), then `mutate --current-round {N+1}`, then **actively end the current turn** (calling no scheduling tools).

The Stop hook (`scripts/hooks/pipeline_gate.py`) detects `phase != DONE` → `exit 2` → the harness automatically opens a new turn. The new turn's entry judgment recognizes `turn_type=loop` and enters [Loop Turn: Resume Round](#loop-turn-resume-round).

> **ScheduleWakeup is not used** — it is unavailable when the gate is closed in non-`/loop` runtime environments (e.g. glm proxy). The Stop hook's `exit 2` is the reliable cross-turn driver (reference: ralph's "boulder never stops").

---

## Loop Turn: Resume Round

> Executed in the new turn forced by the Stop hook's `exit 2` (after the main process ends its turn, pipeline_gate detects `phase != DONE` → `exit 2` → the harness restarts a turn). Rebuild context from disk and continue the next mining round.

### Phase 0: Context reconstruction

1. **Run the context-reconstruction script**:
```bash
python scripts/reconstruct_context.py --session-dir "{session_dir}" --format text
```

2. **Extract the key information from the output**:
   - current phase (if intra-round compaction happened mid-phase, resume from that phase)
   - completed phases (skip them; do not redo)
   - this round's key information (reflection_context, high-value endpoints, etc.)
   - global progress (total defects, coverage)

3. **Check the Docker container's status**:
```bash
docker ps --filter "name=testvdb-{target}" --format "{{.Names}}" 2>/dev/null
```
If the container is not running but `global_state.docker_container_running` is true → run `docker restart` or start it again.

### Phase 1: Run the mining round

Per the `phases_completed` list, start from the first incomplete phase of [Run one full mining round](#run-one-full-mining-round).

**Checkpoint-resume rules**:
- If `phases_completed` contains `ROUND_START` but not `ATTACK_GEN` → start from ATTACK_GEN
- If `phases_completed` contains `ATTACK_GEN` but not `DEBATE_S1` → start from DEBATE_S1 (scripts already generated; collect them directly)
- And so on. Each completed phase's output files are already persisted to disk; use them directly.

### Phase 2: Round end

- If a termination condition is met → run [Final Turn: Cleanup](#final-turn-cleanup)
- If continuing → update `pipeline_state.json` (`current_round` += 1, `phase` = `"ROUND_START"`, `phases_completed` = []), then **end the turn**. The Stop hook triggers the next round (same mechanism as at the end of Step 8).

---

## Run one full mining round

> This is a sub-procedure of Step 8. Turn 1's Round 1 and the loop turn's Round N both run this flow.
> After each sub-step completes you **must** update `pipeline_state.json`'s `phase`, `phases_completed`, and `phase_data`.

Before each round: if it is the first round, create the `results/{target}/{version}/{timestamp}/` directory structure.

### 8a. ROUND_START — inject reflection_context + threat_model

**Update pipeline_state**: `phase` = `"ATTACK_GEN"`, append `"ROUND_START"` to `phases_completed`

Round 1: no reflection_context. Later rounds inject the previous round's experience.

**reflection_context structure**:
```json
{
  "key_learnings": ["...", "..."],
  "rejection_patterns": [{"endpoint": "...", "reason": "..."}],
  "high_value_endpoints": ["..."],
  "exhausted_endpoints": ["..."],
  "last_round_summary": "..."
}
```

**Cross-session strategy injection** (evolution.enabled=true): `python scripts/strategy_injector.py {target} --text-only`

**Threat-model injection** (intelligence.enabled=true and inject_to_attack_agents=true):
```bash
THREAT_MODEL_ATTACK=$(python scripts/threat_model_injector.py {target} --mode attack --text-only 2>/dev/null || echo "")
```

(ADR-0008: the judge enhancement injection was removed along with the Judge Quartet; only threat_model's --mode attack injection remains in use.)

### 8b. ATTACK_GEN — contract chunking + concurrently dispatch the Attack Trio + Explorer

**Contract chunking (ADR-0008, one chunk per round)**:
```bash
python scripts/chunk_contract.py results/{target}/{version}/structured_contract.json --session-dir ${PROJECT_ROOT}/results/{target}/{version}/{timestamp}
```
Round R dispatches `chunks[R-1]`. The dispatch prompt carries `this round's chunk={chunk_id}` + the chunk's unit_ref list.

**⛔ Absolutely forbidden: the main process generating attack scripts itself. It must dispatch via the Agent tool.**

```
Agent(subagent_type="testvdb:attack-boundary", description="Boundary attack {target} v{version}",
  prompt="Per the agents/attack-boundary.md spec, generate boundary attack scripts for {target} v{version}. contract=${PROJECT_ROOT}/results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}, reflection_context={reflection_context}, this round's chunk={chunk_id} (the chunk's unit_ref list is in chunks.json; only attack units within this chunk). {THREAT_MODEL_ATTACK}")

Agent(subagent_type="testvdb:attack-state", description="State attack {target} v{version}",
  prompt="Per the agents/attack-state.md spec... (same format as above) {THREAT_MODEL_ATTACK}")

Agent(subagent_type="testvdb:attack-semantic", description="Semantic attack {target} v{version}",
  prompt="Per the agents/attack-semantic.md spec... (same format as above) {THREAT_MODEL_ATTACK}")
```

> **attack-vein has been removed per ADR-0009** (v2.5 → deprecated): its condition-driven deep-mining duty is carried by the two-phase scheduling's exploration mode (ADR-0009 §2-§4); deep-exploration output goes uniformly through the sandbox small loop (the exception path of agents running curl themselves no longer exists).

**Verify outputs**:
```bash
# the old trio's script output
ls results/{target}/{version}/{timestamp}/debate_logs/*.py 2>/dev/null | wc -l
```

**Update pipeline_state**: `phase` = `"DEBATE_S1"`, append `"ATTACK_GEN"` to `phases_completed`, `phase_data.ATTACK_GEN` = `{scripts_generated: N, agents_completed: [...]}`

### 8c. DEBATE_S1 — Debate Stage 1 (static review + D3b pre-verification)

> **gate versions**: v3 (R1-R3, 7 checks, replayable via `TESTVDB_PREVERIFY=NONE`) / **v4 (R4+, + D3b pre-verification 5 classes: oracle_missing / oracle_degenerate / transport_probe_wrong / oracle_shape_conflict / request_required_missing, preverify_version=D3b-R4.0)**

The main process runs the automated review itself (orchestration/coordination work):

1. Collect scripts (ADR-0008: script dedup removed — duplicate attacks are left to natural elimination by execution and the chain-auditor; defect-level dedup happens at 8e.5)
2. Syntax validation (`python -m py_compile`)
3. Constraint-existence validation
4. Script-error heuristic detection: `python scripts/detect_risky_scripts.py "results/{target}/{version}/{timestamp}"`
5. **API call-format AST validation**: `python scripts/validate_api_format.py "results/{target}/{version}/{timestamp}"`
6. **Target-neutrality validation**: `python scripts/validate_target_neutrality.py "results/{target}/{version}/{timestamp}"`
   Scripts containing DB signatures inconsistent with the current target (e.g. target=weaviate but the script hits :6333) → send back to the Attack Agent for fixing (same send-back mechanism as 8d.5).
7. **D3b pre-verification classes A/B** (v3.4, 2026-08-26; oracle-line existence + the transport liveness-probe trichotomy):
   ```bash
   python scripts/_classify_script_errors.py "results/{target}/{version}/{timestamp}"
   ```
8. **D3b pre-verification classes C/D** (spec cross-check: assertion×response-shape compatibility matrix + request-body×nested-required tree; includes meta.oracle single-writer derivation and the WARN sidecar):
   ```bash
   python scripts/_preverify_spec_shape.py "results/{target}/{version}/{timestamp}" --db {target} --version {version}
   ```
9. **Severity-aware retry dispatch** (WARN does not consume budget; REJECT is sent back per the 8d.5 ticket pattern):
   ```bash
   python scripts/_apply_script_retry.py "results/{target}/{version}/{timestamp}"
   ```
10. Write review results to `debate_logs/stage1.json`
11. Script path normalization
12. **Shape-exploration gate (conditional — only when this round's attack dispatch carried a "Shape generalization exploration directive" via threat_model_injector)**: if the shape_exploration_{shape_id}.md list was not produced, or novel_candidate-marked script count < 3 (default; the attack specs' §5 Gate) → DEBATE_S1 rejection of the whole batch back to the Attack Agent for rerun:
    ```bash
    python scripts/validate_shape_exploration.py "results/{target}/{version}/{timestamp}"
    ```
    (Mechanically wired 2026-09-02 — claim audit found the gate was referenced by the three attack specs but never called from the main flow.)

**Update pipeline_state**: `phase` = `"EXECUTION"`, append `"DEBATE_S1"` to `phases_completed`, `phase_data.DEBATE_S1` = `{approved_count: N, rejected_count: M}`

### 8d. EXECUTION — dispatch the Docker Executor + send-back fixes

```
Agent(subagent_type="testvdb:docker-executor", description="Execute {target} v{version} attack scripts",
  prompt="Per the agents/docker-executor.md spec, execute attack scripts in the Docker sandbox. target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}, session_id={session_id}. ⛔ Run the Step 1 command immediately... \n\nYou are a sub-agent dispatched by the main process in the TestVDB pipeline. Using the Agent tool to dispatch grandchild agents is forbidden.")
```

**Verify outputs**: `ls results/{target}/{version}/{timestamp}/output_*.log.done 2>/dev/null | wc -l`

**Send-back fix mechanism** (8d.5; ADR-0008 wired the v2.5 deterministic retry sub-loop):
```bash
# pre-execution static checks (AST classification of 5 error classes) + counter/feedback/over-limit degradation, all deterministic:
python scripts/_classify_script_errors.py "results/{target}/{version}/{timestamp}"
python scripts/_apply_script_retry.py "results/{target}/{version}/{timestamp}"
```
If there is `regen` output → dispatch the corresponding Attack Agent grouped by source (read `${script_id}.retry_feedback.json`, fix, and **overwrite the original file**; see `agents/orchestrator.md` §4.6). Post-execution log-scan fallback (runtime errors):
```bash
python scripts/scan_script_errors.py "results/{target}/{version}/{timestamp}"
```

**Update pipeline_state**: `phase` = `"EVIDENCE_BUILD"`, append `"EXECUTION"` to `phases_completed`, `phase_data.EXECUTION` = `{scripts_executed: N, scripts_passed: M, scripts_error: K}`

### 8e. EVIDENCE_BUILD — candidate extraction + L1 mechanical gate + evidence-builder concurrent fan-out (ADR-0008)

**Step 1 — mechanical candidate extraction** (the builder fan-out's dispatch list; deterministic 0 LLM):
```bash
python scripts/extract_candidates.py "results/{target}/{version}/{timestamp}"
# produces candidates.jsonl (logs with VERDICT: DEFECT_FOUND → candidates; SCRIPT_ERROR excluded)
```

**Step 2 — L1 mechanical gate moved forward** (kills ~90% of historical FP patterns at 0 tokens; REFUTED does not enter fan-out):
```bash
python scripts/verify_live_l1.py "results/{target}/{version}/{timestamp}" --target {target}
```
REFUTED candidates are removed from candidates.jsonl (recorded in verify_live_l1.json for experiment statistics).

**Step 2.5 — evidence-chain quote pre-check (before the auditor; mechanized R2 lesson 2026-08-26)**:
```bash
python scripts/verify_chain_quotes.py "results/{target}/{version}/{timestamp}"
```
Run after all builders' `.done` and before dispatching the auditor. Mismatched chains are sent back per the R2 ticket pattern to the corresponding builder to re-quote the original-text substring (a 1-minute fix << the 3-builders+1-auditor cost of an NME evidence round; R2 measured 3 bracket-annotated quotes landing in NME).

**Step 3 — dispatch evidence-builders concurrently per candidate** (1 builder/candidate; task dedup for efficiency):
```
For each line of candidates.jsonl, dispatch concurrently (subject to dispatch slots; queue beyond that):
Agent(subagent_type="testvdb:evidence-builder", description="Evidence-chain build {defect_id}",
  prompt="Per the agents/evidence-builder.md spec, build the evidence chain for candidate {defect_id}. target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}. Your defect_id={defect_id}.")
```
- Produces `evidence_chain/{defect_id}.json` + `.done` (named per candidate; concurrent writes do not conflict)
- Timed-out (60s/candidate) or no-output candidates: no retry into fan-out; left for the auditor to record NEEDS_MORE_EVIDENCE
- **NEEDS_MORE_EVIDENCE evidence round + send-back ticket (after the auditor's verdicts; extended 2026-08-18)**:
  redispatch builders only for the auditor-marked defect_ids. The redispatch prompt **must carry the auditor's
  rework_order ticket** (type/claim/chain_covered/drift_point/targeted_instruction);
  the builder redoes the work targeted by the ticket (not a full rerun).
  **Send-back cap of 3 rounds** (rework_state file counts per defect_id; same pattern as the defect-level retry counter):
  still mismatched after round 3 → the auditor conservatively judges NOT_DEFECT.
  The ticket's three types: PHENOMENON_MISMATCH (forensic drift → re-read the full log and rebuild around the claim) /
  EVIDENCE_GAP (incomplete chain → targeted section supplements) / SUSPECTED_HALLUCINATION (quotes do not match → re-verify the original lines)

**Update pipeline_state**: `phase` = `"CHAIN_AUDIT"`, append `"EVIDENCE_BUILD"` to `phases_completed`, `phase_data.EVIDENCE_BUILD` = `{candidates: N, l1_refuted: M, builders_done: K}`

### 8e.7. CHAIN_AUDIT — chain-auditor single-instance close-out

Dispatch **after all builders' `.done` close out** (cross-candidate consistency checks need the complete chain set):
```
Agent(subagent_type="testvdb:chain-auditor", description="Evidence-chain audit {target}",
  prompt="Per the agents/chain-auditor.md spec, audit all evidence chains under evidence_chain/ and produce final verdicts (four perspectives A/B/C/D; perspective D consumes intelligence/{target}/developer_cognition.json). target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}.")
```
- Produces `debate_logs/chain_verdicts.json` (DEFECT / NOT_DEFECT / NEEDS_MORE_EVIDENCE +
  fp_evidence_source + root_cause distribution) + `.done`
- Verification: `test -f "results/{target}/{version}/{timestamp}/debate_logs/chain_verdicts.json.done" && echo READY || echo PENDING`
- NEEDS_MORE_EVIDENCE > 0 → back to 8e Step 3 for an evidence round (at most once), redispatch the auditor for final verdicts
- **⛔ The main process never adjudicates. Auditor timeout → all candidates conservatively recorded as NEEDS_MORE_EVIDENCE and one redispatch; second-round timeout → NOT_DEFECT (conservative) + error_log.**

**Cross-round dedup** (8e.5, retained; input changed to the verdict list):
```bash
python scripts/dedup_defects.py "results/{target}/{version}/{timestamp}"
```

**Update pipeline_state**: `phase` = `"REPORTING"`, append `"CHAIN_AUDIT"` to `phases_completed`, `phase_data.CHAIN_AUDIT` = `{verdict_defect: N, not_defect: M, needs_more_evidence: K}`

### 8f. REPORTING — dispatch the Reporter

**confirm_per_round switch (ADR-0009 §6)**: before entering this step run `python scripts/get_setting.py mining.confirm_per_round`.
- `true` (default) → execute 8f/8f.5 as usual.
- `false` (experiment specialization) → **skip 8f and 8f.5**: candidates.jsonl keeps accumulating (8e.5 cross-round dedup as usual), `pipeline_state` marks `phase=MINING_DEFER_CONFIRM`; after session termination run unified adjudication over all accumulated candidates (evidence-builder + chain-auditor in batch + novelty final ruling + reporter, closed out in one pass; same ADR-0008 spec).

```
Agent(subagent_type="testvdb:reporter", description="Generate defect reports {target}",
  prompt="Per the agents/reporter.md spec, generate reports for the following Debate-Confirmed defects: {debate_confirmed}. session_id={session_id}, target={target}, version={version}, session_dir=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}")
```
**Verification:** `ls results/{target}/{version}/{timestamp}/defects/defect-*.md 2>/dev/null | wc -l`

**⛔ summary.md main-process measured gate (2026-08-21 mechanism-level fix — agent self-verification repeatedly reproduced false claims)**:

After the reporter returns, the main process **must measure** (never trusting the reply text):
```bash
test -s "results/{target}/{version}/{timestamp}/summary.md" && echo SUMMARY-OK || echo SUMMARY-MISSING
```
- `SUMMARY-MISSING` → redispatch the reporter once (dispatch prompt appended: "last round's summary.md never landed on disk; this time only write summary.md + the closing Bash test -s measurement"); still missing → the main process writes it on the reporter's behalf, annotating the top of the summary with `⚠️ reporter-generated=false (main-process fallback)`, recorded in error_log
- This gate also applies before 9b (generating summary.md): the Final Turn path measures the same way

**Update pipeline_state**: `phase` = `"DEFECT_REVIEW"`, append `"REPORTING"` to `phases_completed`

### 8f.5. DEFECT_REVIEW — per-defect review

```bash
python scripts/verify_defects.py "results/{target}/{version}/{timestamp}"
```
Produces `defect-review.md`. FALSE_POSITIVE → delete. NEEDS_IMPROVEMENT → send back to the reporter for rewrite (at most once).

**Update pipeline_state**: `phase` = `"STATE_SAVE"`, append `"DEFECT_REVIEW"` to `phases_completed`

### 8g-8i. STATE_SAVE — save state + analyze output + termination check

Completed by the main process itself:

1. **Save mine_state.json + coverage.json + experience_handoff.json + pipeline_state.json**
2. **Analyze this round's output**: vote-divergence patterns, rejection-reason classification, endpoint coverage update, generate reflection_context
3. **Strategy extraction** (evolution.enabled=true): `python scripts/strategy_extractor.py "results/{target}/{version}/{timestamp}" {target}`
4. **Termination-condition check** (any one terminates):
   - consecutive_no_defect_rounds >= 5
   - overall_coverage_pct >= 95
   - current_round >= max_rounds (and max_rounds > 0)
   - total_defects_confirmed >= min_defects (and min_defects > 0; `--min-defects 0` = no floor, never triggers)

**Update pipeline_state**: append `"STATE_SAVE"` to `phases_completed`

### 8j. Inter-round container management

- **Continue to the next round**: `docker restart testvdb-{target}-${TESTVDB_SESSION_ID:-standalone}`
- **Terminate the loop**: `docker compose -f docker/{target}.yml down -v`

---

## Final Turn: Cleanup

> Executed when a termination condition is met (possibly triggered at the end of Turn 1 or any loop turn).

### Step 9: Issue drafts + summary + cleanup

#### 9a. Run the Novelty Gate (ADR-0008: post-hoc final ruling + NON_NOVEL archived, not deleted)

**Before generating issue drafts, the Novelty Gate must run on all chain-verdict DEFECT candidates (input source: chain_verdicts.json; adapted per ADR-0008). Only the Gate's Gate-Endorsed (endorsement=true) output are genuinely submittable defects (ADR-0001).**

```bash
python scripts/novelty_gate.py --session-dir results/{target}/{version}/{timestamp}
```

**Exit-code handling**:
- `0` (NOVEL endorsements exist) → continue to 9b; generate issue drafts only for `endorsement=true` defects
- `1` (all rejected) → skip issue generation, generate the summary directly
- `2` (UNVERIFIED present) → skip issue generation, log a warning

**Read the Gate results**:
```bash
cat results/{target}/{version}/{timestamp}/debate_logs/novelty_gate.json | python -c "
import json, sys
data = json.load(sys.stdin)
endorsed = [d for d, r in data.items() if r.get('endorsement')]
print(json.dumps({'endorsed_defects': endorsed}, ensure_ascii=False))
"
```

**NON_NOVEL archiving (ADR-0008; replaces the old "rejected = discarded")**: executed once after the Gate's final ruling — archived
candidates' defect-N.md are moved into `archived/`, and `archived/manifest.json` records
`related_issue_numbers` (extracted from gate_evidence_url). This is the data source of RQ1's "found already-reported bugs" column.
No archiving intra-round (during the REPORTING→Step 9 window, defect-N.md stay in place).

#### 9b. Generate issue drafts (endorsed NOVEL only, candidate-level)

**⛔ Absolutely forbidden: submitting issues directly to GitHub. All output stays on the local filesystem.**

```bash
mkdir -p results/{target}/{version}/{timestamp}/issues
```

**Granularity mapping rules (ADR-0002)**: the Novelty Gate rules at candidate/script level (one defect aggregation may contain multiple candidates, e.g. defect-2 with 7 parameters). Mapping:
- **Issue drafts**: generated at **candidate level**, only candidates with `endorsement=true` → `issues/issue-{param-slug}-novel.md`. Rejected candidates do **not** get issue drafts and are **moved to `archived/` (ADR-0008, see 9a's archiving paragraph)**.
- **Archive manifest**: rejected candidates are recorded in `archived/manifest.json` (candidate + param + grade + evidence_url + related_issue_numbers). Candidates with `judge_discrepancy=true` must be annotated (the gate overturned the initial NOVEL — this is the gate's core value).
- **Defect aggregation reports** (`defects/defect-N.md`) are still generated, but the header must carry the gate summary: containing N candidates, M endorsed / (N-M) rejected, so the aggregate narrative does not obscure candidate-level gate decisions.

#### 9b.5 Issue review reminder

> ⚠️ **Manual review required**: issue drafts are AI-generated; a human must review them before manual submission.

#### 9a.6 Generate MRE scripts (dispatch reporter-mre)

The main process dispatches reporter-mre for the debate-confirmed defects that passed review, generating self-contained MRE scripts (one independent Python script per defect, with no dependency on TestVDB code). The reporter focuses on defect-N.md reports; MRE scripts are generated independently by reporter-mre (v2.1.1 reporter split).

```
Agent(subagent_type="testvdb:reporter-mre", description="Generate MRE scripts {target}",
  prompt="Per the agents/reporter-mre.md spec, generate self-contained MRE scripts for the following Debate-Confirmed defects: {debate_confirmed}. session_id={session_id}, target={target}, version={version}, session_dir=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}")
```

**Verification:** `ls results/{target}/{version}/{timestamp}/mre/defect-*-script.py.done 2>/dev/null | wc -l` (should be ≥1; reporter-mre touches `.done` after each script and passes `py_compile`)

> The reporter's Pre-Submit Gate reproduction verification uses the curl fallback (reporter.md supports it) — MRE scripts are generated independently by this step's reporter-mre for external one-command reproduction.

#### 9b. Generate summary.md + defect-review.md

#### 9c. Cleanup

```bash
# strategy extraction (evolution.enabled=true)
python scripts/strategy_extractor.py "results/{target}/{version}/{timestamp}" {target}

# container cleanup
docker compose -f docker/{target}.yml down -v --remove-orphans
docker network rm testvdb-net-${TESTVDB_SESSION_ID:-standalone} 2>/dev/null || true

# update state
# update .session.lock status to completed
```

### Step 10: Mark completion

Update `pipeline_state.json`: `phase` = `"DONE"`, `turn_type` = `"done"`

---

## Phase update commands

> After each sub-step completes, the main process must run the following updates. Use the `pipeline_state.py` CLI (ADR-0004).

### advance — phase advancement

```bash
python scripts/pipeline_state.py advance \
    --session-dir "{session_dir}" \
    --phase "{NEXT_PHASE}" \
    --phase-data '{"{COMPLETED_PHASE}": {PHASE_OUTPUT}}'
```

### mutate — update global state counters

```bash
python scripts/pipeline_state.py mutate \
    --session-dir "{session_dir}" \
    --total-defects {total_defects} \
    --coverage {coverage} \
    --docker-running {docker_running} \
    --consecutive-no-defect {consecutive_no_defect}
```

### status — query the current state

```bash
python scripts/pipeline_state.py status --session-dir "{session_dir}"
```

> Equivalent to the original manual JSON editing, but adds seam validation (invalid phase transitions → InvalidTransition error).

---

## Termination Conditions

1. **Stalemate**: 5 consecutive rounds with no new defects
2. **Coverage**: contract coverage ≥ 95%
3. **Max Rounds**: `--max-rounds` reached (and > 0)
4. **Min Defects**: `--min-defects` reached (`--min-defects 0` = no floor, never triggers)

## Output

```
results/{target}/{version}/{timestamp}/
├── defects/defect-1.md
├── mre/defect-1-script.py
├── issues/issue-1-batch-atomicity.md
├── defect-review.md
├── summary.md
├── debate_logs/
│   ├── stage1.json
│   ├── stage2_aggregation.json
│   ├── stage2_deduped.json
│   ├── stage2_doc.json
│   ├── stage2_evidence.json
│   ├── stage2_novelty.json
│   └── stage2_severity.json
├── structured_contract.json
├── mine_state.json
├── pipeline_state.json     ← v3 cross-turn state machine
├── coverage.json
├── experience_handoff.json
└── session_metadata.json

intelligence/{target}/
├── issue_corpus.json
├── commit_corpus.json
├── classified_issues.json
├── bug_shapes.json
├── developer_cognition.json
└── threat_model.json
```

## Error Recovery

Re-running the same command resumes an interrupted session. The loop-turn entry automatically detects the breakpoint in `pipeline_state.json` and resumes.

## Multi-DB Mining

```bash
# Terminal 1
/testvdb:mine milvus v2.4.0
# Terminal 2
/testvdb:mine qdrant v1.13.0
```
