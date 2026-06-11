# TestVDB

English | [中文](./README_zh.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Claude Code Plugin](https://img.shields.io/badge/Claude%20Code-Plugin-purple.svg)](https://docs.anthropic.com/en/docs/claude-code)
[![Version](https://img.shields.io/badge/version-2.1.2-orange.svg)](https://github.com/yihui504/TestVDB/releases)

**Automated Defect Mining for Vector Databases**

TestVDB is an LLM-powered Claude Code plugin that automatically discovers compliance defects in vector databases. It reverse-engineers structured contracts from official documentation, generates targeted attack scripts through multi-agent debate, executes them in Docker sandboxes, and produces verified defect reports with full evidence chains.

Currently supports **Milvus**, **Qdrant**, **Weaviate**, and **pgvector**.

---

## What's New in v2.1.2

- **Cross-Turn State Machine**: `pipeline_state.json` v3 — phase-level checkpoint recovery across context compaction. Every phase completion is immediately persisted, enabling exact breakpoint resumption without relying on model memory.
- **ScheduleWakeup Loop**: Multi-round mining now uses `ScheduleWakeup`-driven cross-turn iteration. Each round is an independent Turn, with `reconstruct_context.py` rebuilding full pipeline context from disk state files at the start of each loop turn.
- **Context Reconstruction**: New `reconstruct_context.py` reads 6 state files (pipeline_state, mine_state, coverage, experience_handoff, structured_contract, threat_model) and produces a self-contained agent context — phase, completed phases, per-phase outputs, global progress, termination conditions, and next action.
- **Executor Reliability Fix**: Template variable substitution in `docker-executor` moved from embedded bash commands to explicit Step 0 shell assignments (`$TARGET`, `$SESSION_DIR`, `$DB_PORT`). Bash variable expansion is deterministic — zero-byte log bug eliminated. Agent retains full execution control (no script-based bypass).
- **PostCompact Enhancement**: `postcompact_verify.py` now supports both v3 and legacy schemas, with precise phase-level recovery instructions and automatic fallback.
- **Agent Update**: `docker-executor.md` rewritten — 4-step SOP with explicit variable declaration per step, Windows path normalization via `sed`, real-time per-script exit code visibility.

[Full Changelog →](#whats-new-in-v211)

---

## What's New in v2.1.1

- **Quality Hardening**: All attack scripts now use `safe_request()` pattern — zero bare API calls, zero script crashes on connection/timeout errors
- **AST-based API Format Validation**: New `validate_api_format.py` in Stage 1 debate performs AST-level checking of attack scripts, rejecting bare `.json()` chains before execution
- **Reporter Split**: `reporter.md` (defect reports) split from `reporter-mre.md` (MRE scripts) — separation of concerns, smaller file sizes
- **Code Deduplication**: `_session_utils.py` shared across 7 hook/maintenance scripts, eliminating ~100 lines of duplicate `_plugin_root()` / `is_session_locked()` implementations
- **Nested Dispatch Prohibition**: Explicit prohibition of nested agent dispatch across all agent prompts — a platform limitation discovered and codified
- **Orchestrator Lifecycle Management**: Extracted to `orchestrator-lifecycle.md` (error handling strategy, PreCompact/PostCompact context protection, progress visibility, multi-DB parallelism)
- **pgvector SQL Endpoint Patterns**: Completed `strategy_extractor.py` with DDL/DML/Search/Index regex patterns for SQL-based vector databases
- **Exception Hardening**: Bare `except:` → specific exception types, `--target` missing-value validation, `.env` quote stripping
- **Agent Fleet**: 18 agents (+ `orchestrator-lifecycle`, + `reporter-mre`), 25 scripts (+ `_session_utils.py`, + `validate_api_format.py`, + `reconstruct_context.py`)

[Full Changelog →](#whats-new-in-v21)

---

## What's New in v2.1

- **Phase 0: Strategic Intelligence Layer**: Pre-mining historical defect analysis — crawls target DB GitHub repos for past issues and merged PRs, builds threat models and cognitive blindspot profiles
- **Issue Tri-Classification**: Classifies historical issues into positive (acknowledged bugs), negative (by-design / wontfix), and invalid (no response) — extracts root cause patterns from positive samples
- **Developer Cognitive Blindspot Model**: 5-category taxonomy (BS-01 through BS-05) mapping systemic developer oversights to attack strategies
- **Cross-DB Bug Shape Migration**: Marks extracted bug patterns as `cross_db_applicable` for Milvus→Qdrant→Weaviate→PGVector strategy reuse
- **3 New Agents**: `issue-miner` (raw), `bug-shape-extractor` (redacted), `threat-modeler` (redacted) — total agent fleet: 18

[Full Changelog →](#whats-new-in-v20)

---

## Table of Contents

- [What's New in v2.1.2](#whats-new-in-v212)
- [What's New in v2.1.1](#whats-new-in-v211)
- [What's New in v2.1](#whats-new-in-v21)
- [What's New in v2.0](#whats-new-in-v20)
- [How It Works](#how-it-works)
- [Defect Taxonomy](#defect-taxonomy)
- [Quick Start](#quick-start)
- [Installation](#installation)
- [Usage](#usage)
- [Architecture](#architecture)
- [Directory Structure](#directory-structure)
- [Configuration](#configuration)
- [Requirements](#requirements)
- [Evidence Chain Standard](#evidence-chain-standard)
- [Testing on Claude Code](#testing-on-claude-code)
- [License](#license)

---

## How It Works

TestVDB operates as a **Claude Code plugin** with a 7-phase pipeline orchestrated by 18 specialized agents. Multi-round mining uses **ScheduleWakeup-driven cross-turn iteration** — each round is an independent Turn, with `pipeline_state.json` (v3 state machine) persisting phase-level progress to disk for exact breakpoint recovery after context compaction.

```
Phase 0: Strategic Intelligence      -- Historical issue mining + bug shape extraction + threat modeling
Phase 1: Knowledge Extraction        -- WebSearch + WebFetch official docs
Phase 2: Contract Formalization      -- Structured JSON contract from raw docs
Phase 3: Attack Script Generation    -- 9 concurrent agents (Fan-Out) + Stage 1 debate (inc. AST validation)
Phase 4: Sandbox Execution           -- Single-command batch execution via executor_runner
Phase 5: Defect Judgment             -- 4 judge agents + Stage 2 voting debate
Phase 6: Report Generation           -- Defect reports + MRE scripts + strategy extraction
```

The pipeline runs iteratively: each round injects `reflection_context` from the previous round into attack agents, enabling strategy adaptation. Phase 0 intelligence (threat model + cognitive blindspots) is also injected to prioritize attack surfaces with historically high defect density. Stage 1 debate now includes AST-level API format validation. After each round, `pipeline_state.json` is updated and `ScheduleWakeup` triggers the next turn. `reconstruct_context.py` rebuilds full agent context from disk at the start of each loop turn. Stalemate detection (5 consecutive rounds with no new defects) triggers strategy re-evaluation.

---

## What's New in v2.0

### Fan-Out Attack Trio

Instead of 3 attack agents, v2.0 deploys **9 concurrent agents** — each attack type (boundary, state, semantic) × 3 focus profiles:
- `priority_first` — targets high-priority constraints first
- `coverage_gap` — fills gaps from `coverage.json`
- `rejection_pattern` — reverse-engineers bypass paths from prior round rejections

All 9 agents run in parallel. Results go through **3-tier deduplication** (endpoint × constraint × strategy), with independent-verification confidence bonuses.

### Cross-Session Strategy Evolution

Mining strategies are now **persistent across sessions and DB targets**:
1. `strategy_extractor.py` extracts successful attack patterns after each session
2. `strategy_registry/` stores strategies per-DB (`{db}_strategies.json`) and globally (`global_strategies.json`)
3. `strategy_injector.py` reads applicable strategies and injects them into Attack Agent prompts
4. Cross-DB migration: strategies from Milvus automatically map to Qdrant equivalents

Every evolution is audited in `evolution_log.jsonl`.

### 7-Mode AI Failure Checklist

`scripts/ai_failure_check.py` validates the pipeline output with 7 detection modes:

| Mode | Check | Action on Failure |
|------|-------|-------------------|
| M1 | Script syntax errors | Rewind |
| M2 | Source URL reachability | Reject |
| M3 | Execution result data validation | Reject |
| M4 | `.done` marker completeness (pipeline integrity) | Halt |
| M5 | Defect classification consistency | Rewind |
| M6 | Methodology fabrication detection | Reject |
| M7 | Infinite loop detection | Halt |

### Material Passport

Every `structured_contract.json` includes a `_passport` field with SHA-256 hash:
```json
{
  "_passport": {
    "schema_version": "2.0",
    "hash_algorithm": "sha256",
    "hash": "88ed0dc...",
    "endpoint_count": 68,
    "constraint_count": 39,
    "core_crud_coverage_pct": 95.0
  }
}
```

`scripts/passport_verify.py` validates contract integrity:
- Exit 0: PASS — hash matches
- Exit 1: NO_PASSPORT — legacy contract format
- Exit 2: TAMPERED — hash mismatch, pipeline rejects and re-generates

### Data Access Level

Every agent file declares a `dataAccess` level in frontmatter:
- `raw` — access to raw documentation and external network
- `redacted` — only access to specific session artifacts
- `verified_only` — only access to judge-verified data

---

## Defect Taxonomy

TestVDB classifies discovered defects into four MECE (Mutually Exclusive, Collectively Exhaustive) categories:

| Type | Name | Definition | Example |
|------|------|------------|--------|
| Type 1 | Illegal Success | Input violating documented constraints is accepted (2xx instead of 4xx) | `limit=-1` returns 200 OK |
| Type 2 | Poor Diagnostics | Invalid input correctly rejected, but error message is unclear | Returns "Unknown Error" instead of "Invalid Dimension" |
| Type 3 | Runtime Failure | Valid input causes crash, 500 error, or abnormal behavior | Legal search request returns 500 |
| Type 4 | State/Logic Violation | API returns success, but internal state is inconsistent | INSERT 3 rows, COUNT returns 2 |

Classification decision tree:

```
1. Illegal input accepted?     --> Type 1 (Illegal Success)
2. Valid input causes crash?   --> Type 3 (Runtime Failure)
3. Error message unclear?      --> Type 2 (Poor Diagnostics)
4. State/result inconsistent?  --> Type 4 (State/Logic Violation)
5. None of the above           --> Not a defect
```

---

## Quick Start

### 1. Install Claude Code CLI

```bash
npm install -g @anthropic-ai/claude-code
```

### 2. Install TestVDB Plugin

**Method A: Claude Code Marketplace (recommended)**
```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

**Method B: Local clone**
```bash
git clone https://github.com/yihui504/TestVDB.git
claude --plugin-dir TestVDB
```

### 3. Mine Defects

Use the `/testvdb:mine` command inside a Claude Code session:

```
/testvdb:mine milvus v2.6.17
/testvdb:mine qdrant v1.12.0 --max-rounds 3
/testvdb:mine weaviate 1.25.0 --min-defects 2
/testvdb:mine pgvector pg17 --max-rounds 0
```

---

## Installation

### Marketplace Install (Recommended)

TestVDB is distributed as a Claude Code plugin. Install via the marketplace:

```bash
# In any Claude Code session:
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

The plugin installs globally and persists across sessions. Use `/help` to verify — you should see `/testvdb:mine` in the command list.

### Local Development Install

```bash
git clone https://github.com/yihui504/TestVDB.git
cd TestVDB
claude --plugin-dir .
```

> **Note:** `--plugin-dir .` loads the plugin for the current session only. File changes take effect in the next session.

---

## Usage

### Command Reference

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]
```

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | -- | Target database: `milvus`, `qdrant`, `weaviate`, or `pgvector` |
| `<version>` | Yes | -- | Target version (e.g., `v2.6.17`, `v1.12.0`, `pg17`) |
| `--max-rounds N` | No | 5 | Maximum mining rounds. `0` for unlimited |
| `--min-defects N` | No | 1 | Minimum defects before early termination |

### Termination Conditions

The pipeline stops when any of the following is met:

1. **Stalemate**: 5 consecutive rounds with no new defects
2. **Coverage**: Contract coverage reaches >= 95%
3. **Max Rounds**: `--max-rounds` limit reached
4. **Min Defects**: `--min-defects` threshold reached

### Error Recovery

Re-run the same command to resume an interrupted session. The system auto-detects incomplete sessions via checkpoint files.

### Multi-DB Parallel Mining

```bash
# Terminal 1
/testvdb:mine milvus v2.6.17
# Terminal 2
/testvdb:mine qdrant v1.12.0
```

### Output Structure

Results are written to `results/{db}/{version}/{timestamp}/`:

```
results/qdrant/v1.12.0/2026-06-07T15-30-00Z/
  defects/defect-1.md              # Defect report
  mre/defect-1-script.py           # Minimal Reproducible Example
  summary.md                       # Session summary
  debate_logs/stage1.json          # Attack script peer review logs
  debate_logs/stage2.json          # Judge quartet voting logs
  structured_contract.json         # Generated contract (with _passport)
  pipeline_state.json              # v3 cross-turn state machine
  mine_state.json                  # Session state snapshot
  session_metadata.json            # Session metadata
  coverage.json                    # Endpoint coverage tracking
  experience_handoff.json          # Cross-round reflection context

intelligence/{target}/             # v2.1 Phase 0 strategic intelligence (per-DB, TTL 30d)
  issue_corpus.json                # Raw historical issue corpus
  commit_corpus.json               # Raw historical commit/PR corpus
  classified_issues.json           # Tri-classification results
  bug_shapes.json                  # Extracted root cause patterns
  developer_cognition.json         # Developer cognitive boundary analysis
  threat_model.json                # Threat model + cognitive blindspots
```

---

## Architecture

### Agent Fleet (16 agent types + 2 auxiliary specs)

> **Note**: 16 agent types are registered in `plugin.json` and dispatchable via `Agent(subagent_type="testvdb:xxx")`. 2 additional specifications (`orchestrator-lifecycle.md`, `reporter-mre.md`) serve as auxiliary references — the former defines lifecycle management rules consumed by the main process, the latter is an agent registered separately for MRE generation.

| Agent | dataAccess | Role |
|-------|-----------|------|
| **orchestrator** | redacted | Pipeline coordinator; dispatches all sub-agents |
| **orchestrator-lifecycle** | redacted | Lifecycle management: error handling, Pre/PostCompact, progress visibility (extracted from orchestrator) |
| **issue-miner** | raw | Crawls historical issues and merged PRs from target repos |
| **bug-shape-extractor** | redacted | Tri-classifies issues (positive/negative/invalid), extracts root cause patterns |
| **threat-modeler** | redacted | Builds threat model and cognitive blindspot model from historical data |
| **knowledge-extractor** | raw | Crawls official docs, extracts endpoints/parameters/constraints |
| **contract-formalizer** | redacted | Converts raw knowledge into structured JSON contract with `_passport` |
| **attack-boundary** | redacted | Generates boundary-value attack scripts |
| **attack-state** | redacted | Generates state-transition attack scripts |
| **attack-semantic** | redacted | Generates semantic/logic attack scripts |
| **docker-executor** | redacted | Batch script execution with explicit variable declaration, real-time exit code visibility |
| **judge-doc** | raw | Validates document reference accessibility and content consistency (with network verification) |
| **judge-evidence** | verified_only | Validates evidence chain completeness |
| **judge-novelty** | raw | Checks defect novelty via GitHub search |
| **judge-severity** | verified_only | Assesses defect severity |
| **reporter** | verified_only | Generates defect reports with evidence chains |
| **reporter-mre** | verified_only | Generates self-contained MRE scripts for confirmed defects |
| **model-test** | redacted | CCSwitch tier routing verification |

### Skills (4 skills)

| Skill | Purpose |
|-------|--------|
| **pipeline** | 6-phase pipeline SOP for the orchestrator |
| **contract-schema** | JSON schema reference for contract formalization |
| **defect-taxonomy** | Four-type defect classification reference |
| **docker-templates** | Docker container templates for each target DB |

### 2-Stage Debate Mechanism

**Stage 1 — Attack Script Peer Review**: Attack agents independently generate test scripts. Scripts undergo peer review voting before sandbox execution. Only scripts that pass the vote proceed.

**Stage 2 — Judge Quartet Voting**: After sandbox execution, the four judge agents independently review results. `judge-doc` runs first as a weight regulator (DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH) adjusting the strictness of the other three judges. A defect is confirmed when evidence and severity both vote `is_defect`. Novelty always votes `is_defect` with `novelty_rating` metadata, not participating in defect confirmation but affecting report priority.

### Pre-Submit Reverify Gate

Every confirmed defect is re-verified in a fresh Docker container before report generation. This eliminates false positives caused by container state leakage or transient errors.

---

## Directory Structure

```
TestVDB/
  .claude-plugin/plugin.json      Plugin manifest (name, version, commands, agents)
  .mcp.json                       MCP server config (GitHub API)
  agents/                         18 agent definitions
    orchestrator.md
    orchestrator-lifecycle.md
    issue-miner.md
    bug-shape-extractor.md
    threat-modeler.md
    knowledge-extractor.md
    contract-formalizer.md
    attack-boundary.md
    attack-state.md
    attack-semantic.md
    docker-executor.md
    judge-doc.md
    judge-evidence.md
    judge-novelty.md
    judge-severity.md
    reporter.md
    reporter-mre.md
    model-test.md
  commands/mine.md                Entry command (/testvdb:mine)
  docker/                         Docker Compose templates
    crawl4ai.yml                  Crawl4AI web scraper service
    milvus.yml                    Milvus (etcd + MinIO + standalone)
    qdrant.yml                    Qdrant standalone
    weaviate.yml                  Weaviate standalone
    pgvector.yml                  PGVector standalone
  hooks/hooks.json                Lifecycle hooks (pre-compact, post-compact, etc.)
  skills/                         4 skill definitions
    pipeline/SKILL.md
    contract-schema/SKILL.md
    defect-taxonomy/SKILL.md
    docker-templates/SKILL.md
  intelligence/                   v2.1 Strategic intelligence cache (per-DB, TTL 30d)
  contracts/                      Reference contracts & schema
    AGENTS.md
    settings_schema.json          Settings validation schema
    pgvector_contract.json        PGVector reference contract
    weaviate_contract.json        Weaviate reference contract
  scripts/                        Infrastructure scripts (25 scripts)
    passport_verify.py            Material Passport hash verification
    strategy_extractor.py         Cross-session strategy extraction
    strategy_injector.py          Cross-DB strategy injection
    ai_failure_check.py           7-mode AI failure checklist
    validate_api_format.py        AST-based API call format validation (v2.1.1)
    _session_utils.py             Shared session utilities (v2.1.1)
    preflight.py                  Session pre-flight checks
    crawl_fetch.py                Crawl4AI web scraper (primary)
    crawl_milvus.py               Milvus-specific doc crawler
    hook_runner.py                Cross-platform hook executor
    github_search.py              GitHub issue/code search
    prioritizer.py                Attack script prioritization
    verify_defects.py             Batch defect verification
    find_python.py                Python interpreter resolution
    developer_attitude.py         Developer sentiment analysis
    cleanup_stop.py               Session cleanup
    emergency_cleanup.py          Emergency container cleanup
    log_execution.py              Execution logging
    notify_check.py               Notification config validation
    postcompact_verify.py         Post-compaction state recovery
    precompact_save.py            Pre-compaction state preservation
    retry_policy.py               Retry policy reporter
    gen_weaviate_contract.py      Weaviate contract generation
    validate_weaviate_contract.py Weaviate contract validation
  docs/                           Documentation
    reviews/                      Code review reports
  settings.json                   Plugin configuration (26+ parameters)
  AGENTS.md                       Agent orchestration rules
  THEORETICAL_FRAMEWORK.md        Research paper
  LICENSE                         MIT License
```

---

## Configuration

### settings.json

Configuration parameters organized into sections:

| Section | Key Parameters | Description |
|---------|---------------|-------------|
| `docker` | `cleanup_on_exit`, `startup_timeout_seconds`, per-DB ports | Docker container lifecycle and port mapping |
| `github` | `token` | GitHub personal access token for novelty judge |
| `retry` | `max_attempts`, `*_delay_seconds` | Retry and delay policies |
| `pipeline` | `default_max_rounds`, `default_min_defects` | Pipeline execution limits |
| `results` | `base_dir`, `max_sessions` | Output directory and session management |
| `knowledge` | `cache_enabled`, `cache_ttl_hours` | Contract caching (default: 168h / 7 days) |
| `notification` | `on_severity`, `webhook_url` | Alert configuration for critical defects |
| `network` | `proxy` | HTTP proxy for network requests |
| `evolution` | `enabled`, `strategy_registry_dir`, `max_strategies_per_injection`, `min_confidence_for_injection` | Cross-session strategy evolution |
| `fan_out` | `enabled`, `seeds_per_agent`, `profiles` | Fan-Out attack dispatch (9 concurrent agents) |
| `ai_failure_check` | `enabled`, `halt_on`, `reject_on`, `rewind_on` | 7-mode AI failure detection |
| `material_passport` | `enabled`, `hash_algorithm`, `reject_on_tamper` | Contract hash integrity verification |
| `intelligence` | `enabled`, `cache_ttl_hours`, `time_window_months`, `max_issues`, `max_commits`, `inject_to_attack_agents`, `inject_to_judge_agents` | v2.1 Phase 0 strategic intelligence config |

### .mcp.json

Configures the GitHub MCP server used by the novelty judge to search for duplicate issues:

```json
{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_PERSONAL_ACCESS_TOKEN": "${GITHUB_TOKEN}"
      }
    }
  }
}
```

---

## Requirements

| Requirement | Version | Notes |
|-------------|---------|-------|
| **LLM Model** | Claude Sonnet/Opus | Runs via Claude Code |
| Claude Code CLI | Latest | `npm install -g @anthropic-ai/claude-code` |
| Docker Engine | 20+ | Must be running before pipeline start |
| Python | 3.9+ | Used by hooks and helper scripts |
| Disk Space | 10GB+ | For Docker images and results |
| Docker Hub Token | -- | **Recommended**. Set `DOCKER_HUB_TOKEN` env var for higher rate limits |
| Network Access | -- | WebFetch must reach target doc sites (milvus.io, qdrant.tech, etc.) |
| GitHub Token | -- | Optional; enables full novelty judge via GitHub API |

---

## Testing on Claude Code

### Quick Test

```bash
# Start Claude Code with the plugin loaded
cd TestVDB
claude --plugin-dir .

# Run a single-round test on Qdrant (simplest setup, single container)
/testvdb:mine qdrant v1.12.0 --max-rounds 1
```

Results will be in `results/qdrant/v1.12.0/<timestamp>/`.

### Debugging

```bash
# Enable debug mode to see plugin loading details
claude --plugin-dir . --debug

# Check loaded agents
/agents

# Check available commands
/help
```

---

## Evidence Chain Standard

Every confirmed defect must satisfy the **3-ring evidence chain**:

1. **Contract Reference**: The specific constraint violated, with constraint ID from the structured contract
2. **Source URL**: Direct link to the official documentation page that defines the constraint
3. **Documentation Link**: (Optional) Source code reference or GitHub issue for additional context

Additionally, each defect report includes a **Minimal Reproducible Example (MRE)** — a self-contained Python script that can be run in a fresh Docker container to reproduce the defect.

---

## License

This project is licensed under the [MIT License](LICENSE).
