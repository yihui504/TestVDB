---
description: Collect historical Issue/Commit intelligence for a specified DB and build the threat model
allowed-tools: Read, Write, Bash, Grep, Glob, Agent
---

# /testvdb:intel — intelligence collection + threat modeling

Independently collect historical Issue/Commit intelligence for a specified vector database, building the threat model and cognitive blindspots (`threat_model.json`). **Runs intelligence only**; no contract/attack/execution. Used for refreshing expired intelligence, updating threat models before cross-DB migration, and debugging threat-modeler in isolation.

> Intelligence is **per-target** (not version-bound) — all versions of the same DB share one `intelligence/{target}/threat_model.json`.

---

## ⚠️ Architecture constraint (CRITICAL — for technical reasons)

**Same as `/testvdb:mine`: the main process only ever orchestrates; it never executes.**

| Forbidden | Correct approach |
|---------|---------|
| ❌ Crawling GitHub Issues/Commits yourself | ✅ `Agent(subagent_type="testvdb:issue-miner")` |
| ❌ Classifying/extracting bug shapes yourself | ✅ `Agent(subagent_type="testvdb:bug-shape-extractor")` |
| ❌ Building the threat model yourself | ✅ `Agent(subagent_type="testvdb:threat-modeler")` |

The main process uses only `Read`/`Write`/`Bash`(verification)/`Grep`/`Glob`/`Agent` to orchestrate.

> **Dispatch discipline**: dispatching `testvdb:*` sub-agents uses **only `Agent(subagent_type=...)`**; ❌ `TaskCreate` is disabled (it does not recognize plugin agent_types → `Spawning agent: unknown`, tasks stay `pending` forever as ghost entries that `TaskStop` cannot delete, with no real agent behind them). `Agent` is a core built-in tool; call it directly (`ToolSearch` not finding it ≠ unavailable). See `commands/mine.md` "dispatch tool discipline".

---

## Usage

```
/testvdb:intel <db> [--max-issues N] [--max-commits N] [--force]
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, `pgvector`, `meilisearch`, or `chroma` |
| `--max-issues N` | No | settings.json `intelligence.max_issues` (default 500) | Collect the most recent N issues + merged PRs |
| `--max-commits N` | No | settings.json `intelligence.max_commits` (default 200) | Collect the most recent N commits |
| `--force` | No | — | Force recollection, ignoring the cache |

---

## Execution steps

### Step 1: Parse arguments + preflight

- Verify `target` ∈ {milvus, qdrant, weaviate, pgvector, meilisearch, chroma}
- Parse `max_issues`, `max_commits`, `force`
- Determine `PROJECT_ROOT`: `git rev-parse --show-toplevel 2>/dev/null || pwd`
- Preflight: `python scripts/preflight.py`
- If `intelligence.enabled=false` (settings.json) → print "intelligence feature not enabled" and exit

### Step 2: Read intelligence settings (CLI arguments override defaults)

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

> **CLI overrides**: `--max-issues N` → `INTEL_MI=N`; `--max-commits N` → `INTEL_MC=N`. When not passed, use the settings defaults.

### Step 3: Cache check (D-judgment, batch D)

Check whether `intelligence/{target}/threat_model.json` is reusable:

```bash
python scripts/check_cache.py intel "intelligence/{target}" {target} --ttl {INTEL_TTL}
```

- **USABLE** (exit 0) and **no `--force`** → jump to [Step 7: Output](#step-7-output) (report "cache valid, collection skipped")
- **STALE / INVALID / MISSING** or **`--force` given** → continue to Step 4 to recollect

> TTL defaults to 720h (30 days).

### Step 4: Dispatch issue-miner

```
Agent(subagent_type="testvdb:issue-miner",
  description="Collect {target} historical Issues and Commits",
  prompt="Per the agents/issue-miner.md spec, collect historical Issues and merged fix PRs for {target}. Input parameters: target={target}, version=*, intelligence_dir=intelligence/{target}/, time_window_months={INTEL_TW}, max_issues={INTEL_MI}, max_commits={INTEL_MC}. Write the results to intelligence/{target}/issue_corpus.json and intelligence/{target}/commit_corpus.json.")
```

> `version=*` means collecting all-version history (intelligence is per-target, not version-bound). On failure → log a warning and skip Steps 5/6.

### Step 5: Dispatch bug-shape-extractor

```
Agent(subagent_type="testvdb:bug-shape-extractor",
  description="Extract {target} historical Bug Shapes",
  prompt="Per the agents/bug-shape-extractor.md spec, classify and extract root-cause patterns from intelligence/{target}/issue_corpus.json and intelligence/{target}/commit_corpus.json. Write the results to intelligence/{target}/classified_issues.json, bug_shapes.json, developer_cognition.json.")
```

### Step 6: Dispatch threat-modeler

```
Agent(subagent_type="testvdb:threat-modeler",
  description="Build the {target} threat model",
  prompt="Per the agents/threat-modeler.md spec, build the threat model from bug_shapes.json, classified_issues.json, developer_cognition.json. Write the result to intelligence/{target}/threat_model.json.")
```

### Step 7: Output

Load the intelligence summary:

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

Report:
- intelligence path: `intelligence/{target}/threat_model.json`
- blindspot count (`blindspot_count`), high-priority attack surfaces (`high_priority_areas`), Top 3 blindspots
- collection scale: `max_issues={INTEL_MI}`, `max_commits={INTEL_MC}`, time window `{INTEL_TW}` months
- source: cache reuse / newly collected

---

## Independence

This command **runs intelligence collection + modeling only**; it does not start:
- ❌ Documentation extraction/contract generation (→ use `/testvdb:contract`)
- ❌ Attack generation/execution (→ use `/testvdb:mine`)

Typical uses:
1. **Intelligence refresh**: `--force` to recollect expired intelligence
2. **Pre-migration**: build a threat model for a new target DB
3. **Isolated debugging**: verify the issue-miner/bug-shape/threat-modeler chain
4. **Scale adjustment**: `--max-issues 50 --max-commits 20` for a quick small-sample collection

---

## Relationship to /testvdb:mine

`/testvdb:mine`'s intelligence stage (intelligent consumption) invokes **exactly the same** agent dispatch logic as this command when the cache is missing/expired (issue-miner → bug-shape-extractor → threat-modeler). This command is the **independently triggerable version** of mine's intelligence stage. See `commands/mine.md` Step 3.6.
