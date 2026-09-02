---
description: Generate/refresh documentation knowledge and the structured contract for a specified DB version
allowed-tools: Read, Write, Bash, Grep, Glob, Agent
---

# /testvdb:contract — documentation extraction + contract generation

Independently extract official documentation knowledge for a specified vector database version and formalize it into a structured contract (`structured_contract.json`). **Runs documentation+contract only**; no attack/execution/judge/reporting. Used for debugging contract-formalizer, verifying contracts (e.g. bug #3's category), and refreshing expired contracts.

---

## ⚠️ Architecture constraint (CRITICAL — for technical reasons)

**Same as `/testvdb:mine`: the main process only ever orchestrates; it never executes.**

| Forbidden | Correct approach |
|---------|---------|
| ❌ Using WebSearch/WebFetch to crawl documentation | ✅ `Agent(subagent_type="testvdb:knowledge-extractor")` |
| ❌ Generating structured_contract.json yourself | ✅ `Agent(subagent_type="testvdb:contract-formalizer")` |

The main process uses only `Read`/`Write`/`Bash`(verification)/`Grep`/`Glob`/`Agent` to orchestrate.

> **Dispatch discipline**: dispatching `testvdb:*` sub-agents uses **only `Agent(subagent_type=...)`**; ❌ `TaskCreate` is disabled (it does not recognize plugin agent_types → `Spawning agent: unknown`, tasks stay `pending` forever as ghost entries that `TaskStop` cannot delete, with no real agent behind them). `Agent` is a core built-in tool; call it directly (`ToolSearch` not finding it ≠ unavailable). See `commands/mine.md` "dispatch tool discipline".

---

## Usage

```
/testvdb:contract <db> <version> [--force]
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, `pgvector`, `meilisearch`, or `chroma` |
| `<version>` | Yes | — | Target version number (e.g. `1.38.0`) |
| `--force` | No | — | Force regeneration, ignoring the cache (rerun even if the cache is valid) |

---

## Execution steps

### Step 1: Parse arguments + preflight

- Verify `target` ∈ {milvus, qdrant, weaviate, pgvector, meilisearch, chroma}
- Parse `version`, `force`
- Determine `PROJECT_ROOT`: `git rev-parse --show-toplevel 2>/dev/null || pwd`
- Preflight: `python scripts/preflight.py`

### Step 2: Cache check (D-judgment, batch D)

Check whether `results/{target}/{version}/structured_contract.json` is reusable:

```bash
python scripts/check_cache.py contract "results/{target}/{version}" {target} {version} --ttl {knowledge.cache_ttl_hours}
```

- **USABLE** (exit 0) and **no `--force`** → jump to [Step 6: Output](#step-6-output) (report "cache valid, generation skipped")
- **STALE / INVALID / MISSING** or **`--force` given** → continue to Step 3 to regenerate

> TTL defaults to 168h (read from `knowledge.cache_ttl_hours` in `settings.json`).

### Step 3: Dispatch the Knowledge Extractor

```
Agent(subagent_type="testvdb:knowledge-extractor",
  description="Extract {target} {version} documentation knowledge",
  prompt="Per the agents/knowledge-extractor.md spec, extract API documentation knowledge for {target} {version}. Write the result to results/{target}/{version}/raw_knowledge.json (SDK/Docker info goes separately to deployment_meta.json, v3.4 §B)")
```

**Verification:** `ls -la results/{target}/{version}/raw_knowledge.json results/{target}/{version}/deployment_meta.json`

### Step 4: Dispatch the Contract Formalizer

```
Agent(subagent_type="testvdb:contract-formalizer",
  description="Formalize the {target} v{version} API contract",
  prompt="Per the agents/contract-formalizer.md spec, convert results/{target}/{version}/raw_knowledge.json into structured_contract.json (every constraint graded with level, Rule 2.7; v3.4). Write the result to results/{target}/{version}/structured_contract.json")
```

**Verification:** `ls -la results/{target}/{version}/structured_contract.json`

### Step 5: Contract gate checks

Contract validity verification (batch B's generic `validate_contract`):

```bash
python scripts/validate_contract.py "results/{target}/{version}/structured_contract.json"
```

- exit 0 (PASS, possibly with warnings) → pass
- exit 1 (FAIL, with errors) → print errors + terminate (an unqualified contract must not be used for mining)
- exit 2 (loading/usage error) → terminate

**Step 5b: Contract documentation-asset preflight (Rule P1.0, 2026-09-02)**:

```bash
python scripts/preflight_contract_docs.py "results/{target}/{version}/structured_contract.json"
```

- exit 0 → sidecar `doc_preflight.json` ready (consumed by evidence-builder layer A's first two layers), or
  skipped via `TESTVDB_OFFLINE=1`
- exit 1 → dead/mismatched documentation exists; **do not terminate** (an environmental fact; the forensic layer records it honestly), print
  counts for troubleshooting

**Passport hash verification** (when `material_passport.enabled=true`):
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```

### Step 5.5: Strategy pre-binding (v3.4 D2; deterministic main-process step — 0 LLM)
```bash
python scripts/bind_strategies.py "results/{target}/{version}/structured_contract.json"
```
- exit 1 (level lint failure: the contract lacks `level` fields) → redispatch contract-formalizer (Rule 2.7 was not executed)
- normal → every constraint carries `bound_strategies` + the top-level `_strategy_binding` summary (attack agents generate directly per the binding list)

### Step 6: Output

Report:
- contract path: `results/{target}/{version}/structured_contract.json`
- endpoint count (`len(api_endpoints)`), category distribution, data_types count
- source: cache reuse / newly generated
- gate result: PASS / FAIL (warning count)

---

## Independence

This command **runs documentation extraction + contract generation + gating only**; it does not start:
- ❌ Attack generation (attack-boundary/state/semantic)
- ❌ Docker execution (docker-executor)
- ❌ Judge debate (judge-*)
- ❌ Report generation (reporter)

Typical uses:
1. **Contract debugging**: verify contract-formalizer output in isolation (e.g. bug #3's category neutralization)
2. **Contract refresh**: `--force` to regenerate an expired contract
3. **Pre-migration verification**: generate a contract for a target DB and confirm endpoint coverage

---

## Relationship to /testvdb:mine

`/testvdb:mine`'s contract stage (intelligent consumption) invokes **exactly the same** agent dispatch logic as this command when the cache is missing/expired (knowledge-extractor → contract-formalizer → gating). This command is the **independently triggerable version** of mine's contract stage. See `commands/mine.md` Step 3.
