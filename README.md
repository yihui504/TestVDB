# TestVDB

English | [中文](./README_zh.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Claude Code Plugin](https://img.shields.io/badge/Claude%20Code-Plugin-purple.svg)](https://docs.anthropic.com/en/docs/claude-code)
[![Version](https://img.shields.io/badge/version-2.1.3-orange.svg)](https://github.com/yihui504/TestVDB/releases)

**Automated Defect Mining for Vector Databases**

TestVDB is an LLM-powered Claude Code plugin that automatically discovers compliance defects in vector databases. It reverse-engineers structured contracts from official documentation, generates targeted attack scripts through multi-agent debate, executes them in Docker sandboxes, and produces verified defect reports with full evidence chains.

Currently supports **Milvus**, **Qdrant**, **Weaviate**, and **pgvector**.

---

## What's New in v2.1.3

- **Anti-Shortcut Enforcement**: Stop-hook pipeline gate (`scripts/hooks/pipeline_gate.py`) validates three LLM shortcut symptoms at session end — (1) document analysis coverage below 60% threshold, (2) unjustified fallback without documented reason, (3) pipeline phase not reaching DONE. Attack agents are contract-bound to produce `analyzed_documents_*.md` with exact URLs from `raw_knowledge.md` Document Sources, and must pair every `FALLBACK_TRIGGERED` marker with a `[FALLBACK_JUSTIFIED: reason]` marker. Gate performs exact string matching (not fuzzy) — generic or placeholder URLs result in exit 2 interception.
- **Agent Contract Hardening**: All three attack agents (`attack-boundary.md`, `attack-state.md`, `attack-semantic.md`) now include mandatory step-by-step contracts: (a) Read `raw_knowledge.md` before writing analyzed documents, (b) locate `## Document Sources` table, (c) copy URLs character-by-character from the `URL` column. Self-check rule: every URL must match a row in the Document Sources table exactly.
- **Gate Path Bug Fix**: `_resolve_round_dir()` now correctly resolves `timestamp_dir` against `project_root` (pipeline v3 convention) with fallback to `session_dir`-relative paths (legacy/test convention). Previously, path double-nesting caused all quality checks to silently skip. `_parse_analyzed_docs()` now uses recursive glob (`rglob`) to find analyzed documents in subdirectories like `debate_logs/`.
- **Configurable Gate Thresholds**: `TESTVDB_GATE_ACTIVE_THRESHOLD` (default 600s) and `TESTVDB_DOC_COVERAGE_THRESHOLD` (default 0.6) now configurable via environment variables.
- **Project Cleanup**: Removed 40+ one-time development scripts, empty JS stubs, temp HTML/JSON artifacts, and stale Docker attack scripts from source tree. Reorganized reference data into `data/`, logs into `logs/development/`, analysis pipelines into `scripts/analysis/`.

[Full Changelog →](#whats-new-in-v212)

---

## What's New in v2.1.2

- **Cross-Turn State Machine**: `pipeline_state.json` v3 — phase-level checkpoint recovery across context compaction. Every phase completion is immediately persisted, enabling exact breakpoint resumption without relying on model memory.
- **ScheduleWakeup Loop**: Multi-round mining now uses `ScheduleWakeup`-driven cross-turn iteration. Each round is an independent Turn, with `reconstruct_context.py` rebuilding full pipeline context from disk state files at the start of each loop turn.
- **Context Reconstruction**: New `reconstruct_context.py` reads 6 state files and produces a self-contained agent context — phase, completed phases, per-phase outputs, global progress, termination conditions, and next action.
- **Executor Reliability Fix**: Template variable substitution in `docker-executor` moved from embedded bash commands to explicit Step 0 shell assignments. Bash variable expansion is deterministic — zero-byte log bug eliminated.
- **Agent Update**: `docker-executor.md` rewritten — 4-step SOP with explicit variable declaration per step, Windows path normalization via `sed`, real-time per-script exit code visibility.

---

## What's New in v2.1.1

- **Quality Hardening**: All attack scripts now use `safe_request()` pattern — zero bare API calls, zero script crashes on connection/timeout errors
- **AST-based API Format Validation**: New `validate_api_format.py` in Stage 1 debate performs AST-level checking of attack scripts
- **Reporter Split**: `reporter.md` (defect reports) split from `reporter-mre.md` (MRE scripts)
- **Code Deduplication**: `_session_utils.py` shared across 7 hook/maintenance scripts
- **Nested Dispatch Prohibition**: Explicit prohibition of nested agent dispatch across all agent prompts
- **Orchestrator Lifecycle Management**: Extracted to `orchestrator-lifecycle.md`
- **Agent Fleet**: 18 agents, 25+ scripts

---

## Table of Contents

- [What's New in v2.1.3](#whats-new-in-v213)
- [How It Works](#how-it-works)
- [Defect Taxonomy](#defect-taxonomy)
- [Quick Start](#quick-start)
- [Installation](#installation)
- [Usage](#usage)
- [Architecture](#architecture)
- [Anti-Shortcut Pipeline Gate](#anti-shortcut-pipeline-gate)
- [Directory Structure](#directory-structure)
- [Configuration](#configuration)
- [Requirements](#requirements)
- [Evidence Chain Standard](#evidence-chain-standard)
- [License](#license)

---

## How It Works

TestVDB operates as a **Claude Code plugin** with a 7-phase pipeline orchestrated by 18 specialized agents. Multi-round mining uses **ScheduleWakeup-driven cross-turn iteration** — each round is an independent Turn, with `pipeline_state.json` (v3 state machine) persisting phase-level progress to disk for exact breakpoint recovery after context compaction. A **Stop-hook pipeline gate** enforces three anti-shortcut quality checks at session end.

```
Phase 0: Strategic Intelligence      -- Historical issue mining + bug shape extraction + threat modeling
Phase 1: Knowledge Extraction        -- WebSearch + WebFetch official docs
Phase 2: Contract Formalization      -- Structured JSON contract from raw docs
Phase 3: Attack Script Generation    -- 9 concurrent agents (Fan-Out) + Stage 1 debate (inc. AST validation)
Phase 4: Sandbox Execution           -- Single-command batch execution via docker-executor
Phase 5: Defect Judgment             -- 4 judge agents + Stage 2 voting debate
Phase 6: Report Generation           -- Defect reports + MRE scripts + strategy extraction

Stop Hook: Pipeline Gate             -- Quality enforcement (doc coverage, fallback justification, phase completeness)
```

Each round injects `reflection_context` from the previous round into attack agents, enabling strategy adaptation. Phase 0 intelligence (threat model + cognitive blindspots) prioritizes attack surfaces with historically high defect density. After each round, `pipeline_state.json` is updated and `ScheduleWakeup` triggers the next turn. Stalemate detection (5 consecutive rounds with no new defects) triggers strategy re-evaluation.

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
/testvdb:mine weaviate 1.38.0 --min-defects 2
/testvdb:mine pgvector pg17 --max-rounds 0
```

---

## Installation

### Marketplace Install (Recommended)

```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

### Local Development Install

```bash
git clone https://github.com/yihui504/TestVDB.git
cd TestVDB
claude --plugin-dir .
```

> **Note:** File changes take effect in the next session.

---

## Usage

### Command Reference

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]
```

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | -- | `milvus`, `qdrant`, `weaviate`, or `pgvector` |
| `<version>` | Yes | -- | Target version (e.g., `v2.6.17`, `v1.12.0`, `pg17`) |
| `--max-rounds N` | No | 5 | Maximum mining rounds. `0` for unlimited |
| `--min-defects N` | No | 1 | Minimum defects before early termination |

### Termination Conditions

1. **Stalemate**: 5 consecutive rounds with no new defects
2. **Coverage**: Contract coverage >= 95%
3. **Max Rounds**: `--max-rounds` limit reached
4. **Min Defects**: `--min-defects` threshold reached

### Error Recovery

Re-run the same command to resume an interrupted session. The system auto-detects incomplete sessions via `pipeline_state.json`.

### Multi-DB Parallel Mining

```bash
# Terminal 1
/testvdb:mine milvus v2.6.17
# Terminal 2
/testvdb:mine qdrant v1.12.0
```

### Output Structure

```
results/{db}/{version}/{timestamp}/
  defects/defect-1.md              # Defect report
  mre/defect-1-script.py           # Minimal Reproducible Example
  summary.md                       # Session summary
  debate_logs/stage1.json          # Attack script peer review logs
  debate_logs/stage2.json          # Judge quartet voting logs
  debate_logs/analyzed_documents_*.md  # Per-agent document analysis manifests
  structured_contract.json         # Generated contract (with _passport)
  pipeline_state.json              # v3 cross-turn state machine
  mine_state.json                  # Session state snapshot
  coverage.json                    # Endpoint coverage tracking
  experience_handoff.json          # Cross-round reflection context

intelligence/{target}/             # Phase 0 strategic intelligence (per-DB, TTL 30d)
  issue_corpus.json                # Raw historical issue corpus
  commit_corpus.json               # Raw historical commit/PR corpus
  classified_issues.json           # Tri-classification results
  bug_shapes.json                  # Extracted root cause patterns
  developer_cognition.json         # Developer cognitive boundary analysis
  threat_model.json                # Threat model + cognitive blindspots
```

---

## Architecture

### Agent Fleet (18 agent types)

| Agent | dataAccess | Role |
|-------|-----------|------|
| **orchestrator** | redacted | Pipeline coordinator; dispatches all sub-agents |
| **orchestrator-lifecycle** | redacted | Lifecycle management: error handling, Pre/PostCompact, progress visibility |
| **issue-miner** | raw | Crawls historical issues and merged PRs from target repos |
| **bug-shape-extractor** | redacted | Tri-classifies issues, extracts root cause patterns |
| **threat-modeler** | redacted | Builds threat model and cognitive blindspot model |
| **knowledge-extractor** | raw | Crawls official docs, extracts endpoints/parameters/constraints |
| **contract-formalizer** | redacted | Converts raw knowledge into structured JSON contract |
| **attack-boundary** | redacted | Generates boundary-value attack scripts (with anti-shortcut contract) |
| **attack-state** | redacted | Generates state-transition attack scripts (with anti-shortcut contract) |
| **attack-semantic** | redacted | Generates semantic/logic attack scripts (with anti-shortcut contract) |
| **docker-executor** | redacted | Batch script execution in Docker sandbox |
| **judge-doc** | raw | Validates document reference accessibility and content consistency |
| **judge-evidence** | verified_only | Validates evidence chain completeness |
| **judge-novelty** | raw | Checks defect novelty via GitHub search |
| **judge-severity** | verified_only | Assesses defect severity |
| **reporter** | verified_only | Generates defect reports with evidence chains |
| **reporter-mre** | verified_only | Generates self-contained MRE scripts for confirmed defects |
| **model-test** | redacted | Model routing verification |

### Skills (4 skills)

| Skill | Purpose |
|-------|--------|
| **pipeline** | 6-phase pipeline SOP for the orchestrator |
| **contract-schema** | JSON schema reference for contract formalization |
| **defect-taxonomy** | Four-type defect classification reference |
| **docker-templates** | Docker container templates for each target DB |

### 2-Stage Debate Mechanism

**Stage 1 — Attack Script Peer Review**: Attack agents independently generate test scripts. Scripts undergo peer review voting before sandbox execution. Only scripts that pass the vote proceed.

**Stage 2 — Judge Quartet Voting**: After sandbox execution, the four judge agents independently review results. `judge-doc` runs first as a weight regulator (DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH) adjusting the strictness of the other three judges. A defect is confirmed when evidence and severity both vote `is_defect`.

---

## Anti-Shortcut Pipeline Gate

TestVDB v2.1.3 introduces a **Stop-hook pipeline gate** that enforces three quality symptoms at session end, preventing LLM agents from silently cutting corners:

### Three Symptoms

| Symptom | Check | Gate Behavior |
|---------|-------|---------------|
| ① Document Coverage | Ratio of analyzed document URLs to `raw_knowledge.md` Document Sources | < 60% → exit 2 (block) |
| ② Fallback Justification | Every `FALLBACK_TRIGGERED` must have a `[FALLBACK_JUSTIFIED: reason]` marker | Unjustified → exit 2 (block) |
| ③ Phase Completeness | Pipeline must reach `phase=DONE` before session end | Not DONE → exit 2 (block) |

### Agent Contract Requirements

Each attack agent (`attack-boundary`, `attack-state`, `attack-semantic`) must:

1. **Read** `raw_knowledge.md` before writing analyzed documents
2. **Locate** the `## Document Sources` table
3. **Copy** URLs character-by-character from the `URL` column — gate performs exact string matching, not fuzzy
4. **Write** `analyzed_documents_{type}.md` with the exact document source URLs
5. **Self-check**: every URL must match a row in the Document Sources table exactly

### Configuration

```bash
# Gate active threshold (default 600s)
export TESTVDB_GATE_ACTIVE_THRESHOLD=1200

# Document coverage threshold (default 0.6 = 60%)
export TESTVDB_DOC_COVERAGE_THRESHOLD=0.8
```

### Hook Registration

The gate is registered as a Stop hook in `.claude/settings.local.json`:
```json
{
  "hooks": {
    "Stop": [{
      "matcher": "",
      "hooks": [{
        "type": "command",
        "command": "python scripts/hooks/pipeline_gate.py"
      }]
    }]
  }
}
```

---

## Directory Structure

```
TestVDB/
  .claude-plugin/plugin.json      Plugin manifest (name, version, commands, agents)
  .claude/settings.local.json     Stop-hook pipeline gate registration
  .mcp.json                       MCP server config (GitHub API)
  agents/                         21 agent definitions
    orchestrator.md                Main orchestrator SOP
    orchestrator-lifecycle.md      Lifecycle management rules
    issue-miner.md                 Historical issue crawler
    bug-shape-extractor.md         Issue tri-classification
    threat-modeler.md              Threat model builder
    knowledge-extractor.md         Documentation crawler
    contract-formalizer.md         Contract generation
    attack-boundary.md             Boundary-value attacks (with anti-shortcut contract)
    attack-state.md                State-transition attacks (with anti-shortcut contract)
    attack-semantic.md             Semantic/logic attacks (with anti-shortcut contract)
    docker-executor.md             Sandbox script executor
    judge-doc.md                   Document reference validator
    judge-evidence.md              Evidence chain validator
    judge-novelty.md               Defect novelty checker
    judge-severity.md              Severity assessor
    reporter.md                    Defect report generator
    reporter-mre.md                MRE script generator
    model-test.md                  Model routing verification
    _target_api_reference.md       Contract-driven API reference (shared)
    api-template-formalizer.md     API template formalizer
    dev-reviewer.md                Dev review agent
  commands/mine.md                 Entry command (/testvdb:mine)
  docker/                          Docker Compose templates
    crawl4ai.yml                   Crawl4AI web scraper service
    milvus.yml                     Milvus (etcd + MinIO + standalone)
    qdrant.yml                     Qdrant standalone
    weaviate.yml                   Weaviate standalone
    pgvector.yml                   PGVector standalone
  skills/                          4 skill definitions
    pipeline/SKILL.md
    contract-schema/SKILL.md
    defect-taxonomy/SKILL.md
    docker-templates/SKILL.md
  intelligence/                    Strategic intelligence cache (per-DB, TTL 30d)
  contracts/                       Reference contracts & schema
    settings_schema.json           Settings validation schema
    pgvector_contract.json         PGVector reference contract
    weaviate_contract.json         Weaviate reference contract
  scripts/                         Infrastructure scripts
    hooks/
      pipeline_gate.py             Stop-hook anti-shortcut gate (v2.1.3)
      _test_pipeline_gate.py       8-case gate unit tests
      _test_stop_hook.py           Stop hook integration tests
    preflight.py                   Session pre-flight checks
    reconstruct_context.py         Cross-turn context reconstruction
    strategy_extractor.py          Cross-session strategy extraction
    strategy_injector.py           Cross-DB strategy injection
    threat_model_injector.py       Threat model prompt injection
    passport_verify.py             Material Passport hash verification
    validate_api_format.py         AST-based API call format validation
    validate_weaviate_contract.py  Weaviate contract validation
    detect_risky_scripts.py        Risky script detection (Stage 1 debate)
    scan_script_errors.py          Script error scanner (rework trigger)
    dedup_defects.py               Cross-round defect deduplication
    verify_defects.py              Batch defect verification
    prioritize.py                  Attack script prioritization
    developer_attitude.py          Developer sentiment analysis
    crawl_fetch.py                 Crawl4AI web scraper (primary)
    crawl_milvus.py                Milvus-specific doc crawler
    github_search.py               GitHub issue/code search
    find_python.py                 Python interpreter resolution
    hook_runner.py                 Cross-platform hook executor
    retry_policy.py                Retry policy reporter
    _session_utils.py              Shared session utilities
    analysis/                      Reference analysis pipelines
      milvus_bug_shape_pipeline.py
      milvus_full_pipeline.py
    dev_review_repro.py            Dev review reproduction
    validate_threat_model.py       Threat model validation
  data/                            Reference data
    weaviate_openapi_schema.json   Weaviate OpenAPI schema
    experience_handoff.json        Experience handoff template
  logs/development/                Development run logs (archived)
  strategy_registry/               Cross-session attack strategies
  docs/                            Documentation
    reviews/                       Code review reports
    acceptance-checklist-v2.1.1.md
  tests/                           Test suite
  settings.json                    Plugin configuration (26+ parameters)
  AGENTS.md                        Agent orchestration rules
  THEORETICAL_FRAMEWORK.md         Research paper
  LICENSE                          MIT License
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

Configures the GitHub MCP server used by the novelty judge:

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

## Evidence Chain Standard

Every confirmed defect must satisfy the **3-ring evidence chain**:

1. **Contract Reference**: The specific constraint violated, with constraint ID from the structured contract
2. **Source URL**: Direct link to the official documentation page that defines the constraint
3. **Documentation Link**: (Optional) Source code reference or GitHub issue for additional context

Additionally, each defect report includes a **Minimal Reproducible Example (MRE)** — a self-contained Python script that can be run in a fresh Docker container to reproduce the defect.

---

## License

This project is licensed under the [MIT License](LICENSE).
