# TestVDB

English | [中文](./README_zh.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Claude Code Plugin](https://img.shields.io/badge/Claude%20Code-Plugin-purple.svg)](https://docs.anthropic.com/en/docs/claude-code)
[![Version](https://img.shields.io/badge/version-2.2.0-orange.svg)](https://github.com/yihui504/TestVDB/releases)
[![Tests](https://img.shields.io/badge/tests-55%20passed-brightgreen.svg)](tests/)

**Automated Defect Mining for Vector Databases**

TestVDB is an LLM-powered Claude Code plugin that automatically discovers compliance defects in vector databases. It reverse-engineers structured contracts from official documentation, generates targeted attack scripts through multi-agent debate, executes them in Docker sandboxes, and produces verified defect reports with full evidence chains.

Currently supports **Milvus**, **Qdrant**, **Weaviate**, and **pgvector**.

---

## What's New in v2.2.0 — Command Decoupling

The monolithic `/testvdb:mine` pipeline is now split into **three independently-triggerable, intelligently-collaborating commands**:

| Command | Stage | Output |
|---------|-------|--------|
| `/testvdb:contract <db> <version> [--force]` | Doc extraction + contract generation | `structured_contract.json` |
| `/testvdb:intel <db> [--max-issues N] [--max-commits N] [--force]` | Intelligence gathering + threat modeling | `threat_model.json` |
| `/testvdb:mine <db> <version> [--intel \| --contract] [...]` | Attack mining (intelligently consumes intel/contract cache) | defects + reports |

**Smart cache reuse (D-judgment)** — `scripts/check_cache.py` decides whether to reuse cached intel/contract via four conditions: exists → TTL-fresh → valid → target/version match. Any miss → regenerate; all hit → pure mining (skip generation, save time).

**`--intel`/`--contract` parameter control** with **C-boundary** semantics:

| Cache state | `--xxx false` behavior |
|-------------|------------------------|
| **MISSING** (no cache) | Error exit ("missing, run `/testvdb:xxx` first") |
| **STALE / INVALID** | Use existing + warning (no refresh) |
| **USABLE** | Use as-is |

This distinguishes "I have it but want the old one" from "I don't have it at all" — preventing silent mining without prerequisites.

**End-to-end verified** (CC 2.1.165): 5 agent types dispatched successfully with zero `unknown` — knowledge-extractor, contract-formalizer, issue-miner, bug-shape-extractor, threat-modeler.

[Full Changelog →](#whats-new-in-v213)

---

## What's New in v2.1.3

- **Anti-Shortcut Enforcement**: Stop-hook pipeline gate (`scripts/hooks/pipeline_gate.py`) validates three LLM shortcut symptoms at session end — (1) document analysis coverage below threshold, (2) unjustified fallback without documented reason, (3) pipeline phase not reaching DONE. Gate performs exact string matching (not fuzzy) — generic or placeholder URLs result in exit 2 interception.
- **Agent Contract Hardening**: All three attack agents now include mandatory step-by-step contracts: Read `raw_knowledge.md` → locate `## Document Sources` table → copy URLs character-by-character.
- **Gate Path Bug Fix**: `_resolve_round_dir()` correctly resolves `timestamp_dir` against `project_root` (pipeline v3 convention) with fallback to `session_dir`-relative paths.
- **Configurable Gate Thresholds**: `TESTVDB_GATE_ACTIVE_THRESHOLD` (default 600s) and `TESTVDB_DOC_COVERAGE_THRESHOLD` (default 0.6) configurable via environment variables.

---

## What's New in v2.1.2

- **Cross-Turn State Machine**: `pipeline_state.json` v3 — phase-level checkpoint recovery across context compaction.
- **ScheduleWakeup Loop**: Multi-round mining uses `ScheduleWakeup`-driven cross-turn iteration; `reconstruct_context.py` rebuilds full pipeline context from disk state at each loop turn.
- **Executor Reliability Fix**: Template variable substitution moved from embedded bash to explicit Step 0 shell assignments — zero-byte log bug eliminated.

---

## What's New in v2.1.1

- **Quality Hardening**: All attack scripts use `safe_request()` pattern — zero bare API calls.
- **AST-based API Format Validation**: `validate_api_format.py` in Stage 1 debate.
- **Target Neutrality Validation**: `validate_target_neutrality.py` ensures attack scripts don't leak DB-specific signatures (e.g. Qdrant port `6333` when target is Weaviate).
- **Reporter Split**: `reporter.md` (defect reports) split from `reporter-mre.md` (MRE scripts).

---

## Table of Contents

- [What's New](#whats-new-in-v22--command-decoupling)
- [How It Works](#how-it-works)
- [Defect Taxonomy](#defect-taxonomy)
- [Quick Start](#quick-start)
- [Installation](#installation)
- [Usage (Three Commands)](#usage-three-commands)
- [Architecture](#architecture)
- [Anti-Shortcut Pipeline Gate](#anti-shortcut-pipeline-gate)
- [Directory Structure](#directory-structure)
- [Configuration](#configuration)
- [Requirements](#requirements)
- [Evidence Chain Standard](#evidence-chain-standard)
- [License](#license)

---

## How It Works

TestVDB is a **Claude Code plugin** orchestrated by specialized agents. Since v2.2.0, the pipeline exposes **three decoupled commands** that can run independently or compose via smart cache reuse:

```
┌─────────────────┐     ┌──────────────────┐
│ /testvdb:intel  │     │ /testvdb:contract│
│ issue-miner     │     │ knowledge-       │
│ bug-shape       │     │ extractor        │
│ threat-modeler  │     │ contract-        │
│   ↓             │     │ formalizer       │
│ threat_model.json│    │   ↓              │
│ (cache, 30d)    │     │ structured_      │
└─────────────────┘     │ contract.json    │
         │              │ (cache, 7d)      │
         │   D-judgment  └──────────────────┘
         │   (check_cache.py)        │
         └──────────┬───────────────┘
                    ▼
         ┌─────────────────────┐
         │ /testvdb:mine       │  ← intelligently consumes cached
         │ attack-boundary/    │     intel + contract (skip gen if fresh)
         │ state/semantic      │
         │ docker-executor     │
         │ judge-* (4)         │
         │ reporter            │
         │   ↓                 │
         │ defects + MRE       │
         └─────────────────────┘
```

**Mining rounds** use **ScheduleWakeup-driven cross-turn iteration** — each round is an independent Turn, with `pipeline_state.json` (v3 state machine) persisting phase-level progress for exact breakpoint recovery. A **Stop-hook pipeline gate** enforces anti-shortcut quality checks at session end.

Each round injects `reflection_context` from the previous round, enabling strategy adaptation. Phase 0 intelligence (threat model + cognitive blindspots) prioritizes attack surfaces with historically high defect density.

---

## Defect Taxonomy

TestVDB classifies discovered defects into four MECE categories:

| Type | Name | Definition | Example |
|------|------|------------|--------|
| Type 1 | Illegal Success | Input violating documented constraints is accepted | `limit=-1` returns 200 OK |
| Type 2 | Poor Diagnostics | Invalid input rejected, but error message unclear | "Unknown Error" instead of "Invalid Dimension" |
| Type 3 | Runtime Failure | Valid input causes crash or 500 error | Legal search returns 500 |
| Type 4 | State/Logic Violation | API returns success, but internal state inconsistent | INSERT 3 rows, COUNT returns 2 |

```
1. Illegal input accepted?     --> Type 1
2. Valid input causes crash?   --> Type 3
3. Error message unclear?      --> Type 2
4. State/result inconsistent?  --> Type 4
5. None of the above           --> Not a defect
```

---

## Quick Start

### 1. Install Claude Code CLI

```bash
npm install -g @anthropic-ai/claude-code
```

### 2. Install TestVDB Plugin

```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@testvdb
```

### 3a. Full Mining (default — backward compatible)

```
/testvdb:mine milvus v2.6.17
/testvdb:mine qdrant v1.12.0 --max-rounds 3
```

`mine` auto-detects cache freshness (D-judgment) — if intel/contract are fresh, it skips generation and goes straight to mining.

### 3b. Stage-Independent Commands (new in v2.2.0)

```
# Generate/refresh contract only (no mining) — debug contract-formalizer
/testvdb:contract weaviate 1.38.0

# Gather intelligence only (no contract/mining) — refresh threat model
/testvdb:intel pgvector --max-issues 50 --max-commits 20

# Force regenerate contract, then mine
/testvdb:mine milvus v2.6.17 --contract true
```

---

## Installation

### Marketplace Install (Recommended)

```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@testvdb
```

The marketplace is registered as `testvdb` (same name as the plugin), so the install target is `testvdb@testvdb`. Pull updates later with `/plugin marketplace update`.

### Local Development Install

```bash
git clone https://github.com/yihui504/TestVDB.git
cd TestVDB
claude --plugin-dir .
```

> File changes take effect in the next session.

---

## Usage (Three Commands)

### `/testvdb:contract` — Doc Extraction + Contract Generation

```
/testvdb:contract <db> <version> [--force]
```

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, `pgvector` |
| `<version>` | Yes | — | Target version (e.g. `1.38.0`) |
| `--force` | No | — | Force regenerate, bypass cache |

Runs **only** knowledge-extractor → contract-formalizer → gate validation. No attack/execution/judge/reporting. Cache TTL: `knowledge.cache_ttl_hours` (default 168h / 7 days).

### `/testvdb:intel` — Intelligence Gathering + Threat Modeling

```
/testvdb:intel <db> [--max-issues N] [--max-commits N] [--force]
```

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, `pgvector` |
| `--max-issues N` | No | settings `intelligence.max_issues` (500) | Recent issues + merged PRs to crawl |
| `--max-commits N` | No | settings `intelligence.max_commits` (200) | Recent commits to crawl |
| `--force` | No | — | Force regather, bypass cache |

Runs **only** issue-miner → bug-shape-extractor → threat-modeler. Intelligence is **per-target** (not per-version). Cache TTL: `intelligence.cache_ttl_hours` (default 720h / 30 days).

### `/testvdb:mine` — Attack Mining (Smart Consumer)

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N] [--intel true|false] [--contract true|false]
```

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, `pgvector` |
| `<version>` | Yes | — | Target version |
| `--max-rounds N` | No | 5 | Maximum mining rounds. `0` = unlimited |
| `--min-defects N` | No | 1 | Minimum defects before early termination |
| `--intel true\|false` | No | `auto` | Intel stage control (see C-boundary below) |
| `--contract true\|false` | No | `auto` | Contract stage control (see C-boundary below) |

**`auto` (default)**: D-judgment via `check_cache.py` — USABLE→skip generation (pure mining); MISSING/STALE/INVALID→regenerate; MISMATCH→error.

**`true`**: Force regenerate (bypass cache).

**`false` (C-boundary)**: MISSING→error exit; STALE/INVALID→use + warning; USABLE→use as-is.

### Termination Conditions (mining)

1. **Stalemate**: 5 consecutive rounds with no new defects
2. **Coverage**: Contract coverage >= 95%
3. **Max Rounds**: `--max-rounds` reached
4. **Min Defects**: `--min-defects` reached

### Error Recovery

Re-run the same command to resume. The system auto-detects incomplete sessions via `pipeline_state.json`.

### Output Structure

```
results/{db}/{version}/{timestamp}/
  defects/defect-1.md              # Defect report
  mre/defect-1-script.py           # Minimal Reproducible Example
  summary.md                       # Session summary
  debate_logs/                     # Stage 1 + Stage 2 debate logs
  structured_contract.json         # Generated contract (with passport)
  pipeline_state.json              # v3 cross-turn state machine
  coverage.json                    # Endpoint coverage tracking
  experience_handoff.json          # Cross-round reflection context

intelligence/{target}/             # Strategic intelligence (per-DB, TTL 30d)
  threat_model.json                # Threat model + cognitive blindspots
  issue_corpus.json / bug_shapes.json / ...  # Intermediate artifacts
```

---

## Architecture

### Agent Fleet (17 core agents)

| Agent | dataAccess | Role |
|-------|-----------|------|
| **orchestrator** | redacted | Pipeline coordinator SOP (main process dispatches directly) |
| **issue-miner** | raw | Crawls historical issues and merged PRs |
| **bug-shape-extractor** | redacted | Tri-classifies issues, extracts root cause patterns |
| **threat-modeler** | redacted | Builds threat model and cognitive blindspots |
| **knowledge-extractor** | raw | Crawls official docs, extracts endpoints/constraints |
| **contract-formalizer** | redacted | Converts raw knowledge → structured JSON contract |
| **attack-boundary** | redacted | Boundary-value attack scripts (contract-driven, target-neutral) |
| **attack-state** | redacted | State-transition attack scripts |
| **attack-semantic** | redacted | Semantic/logic attack scripts |
| **docker-executor** | redacted | Batch script execution in Docker sandbox |
| **judge-doc** | raw | Document reference validator (weight regulator) |
| **judge-evidence** | verified_only | Evidence chain completeness |
| **judge-novelty** | raw | Defect novelty via GitHub search |
| **judge-severity** | verified_only | Severity assessment |
| **reporter** | verified_only | Defect report generator |
| **reporter-mre** | verified_only | Self-contained MRE script generator |
| **model-test** | redacted | Model routing verification |

> Plus helper definitions: `orchestrator-lifecycle` (lifecycle management), `dev-reviewer`, `api-template-formalizer`, `_target_api_reference` (shared contract-driven API reference).

### Skills (4)

| Skill | Purpose |
|-------|--------|
| **pipeline** | 6-phase pipeline SOP |
| **contract-schema** | JSON schema reference for contract formalization |
| **defect-taxonomy** | Four-type defect classification |
| **docker-templates** | Docker container templates per target DB |

### 2-Stage Debate Mechanism

**Stage 1 — Attack Script Peer Review**: Attack agents generate test scripts; scripts undergo automated review (dedup, AST validation, target-neutrality check, risky-pattern detection) before sandbox execution.

**Stage 2 — Judge Quartet Voting**: Four judge agents review results. `judge-doc` runs first as a weight regulator (DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH) adjusting the strictness of the other three. A defect is confirmed when evidence and severity both vote `is_defect`.

---

## Anti-Shortcut Pipeline Gate

A **Stop-hook pipeline gate** enforces three quality symptoms at session end, preventing LLM agents from silently cutting corners:

| Symptom | Check | Gate Behavior |
|---------|-------|---------------|
| ① Document Coverage | Analyzed URLs vs `raw_knowledge.md` Document Sources | < threshold → exit 2 (block) |
| ② Fallback Justification | Every `FALLBACK_TRIGGERED` needs `[FALLBACK_JUSTIFIED: reason]` | Unjustified → exit 2 (block) |
| ③ Phase Completeness | Pipeline must reach `phase=DONE` | Not DONE → exit 2 (block) |

```bash
# Configurable thresholds
export TESTVDB_GATE_ACTIVE_THRESHOLD=1200    # default 600s
export TESTVDB_DOC_COVERAGE_THRESHOLD=0.8    # default 0.6
```

---

## Directory Structure

```
TestVDB/
  .claude-plugin/plugin.json      Plugin manifest (v2.2.0)
  agents/                         17 core + helper agent definitions
  commands/                       3 commands (v2.2.0 decoupling)
    mine.md                         Smart mining (consumes intel/contract cache)
    contract.md                     Independent contract generation
    intel.md                        Independent intelligence gathering
  skills/                         4 skill definitions
  scripts/                        Infrastructure scripts (32 modules)
    check_cache.py                  v2.2.0 D-judgment (cache reuse detection)
    hooks/pipeline_gate.py          Stop-hook anti-shortcut gate
    preflight.py / reconstruct_context.py / validate_contract.py / ...
    validate_target_neutrality.py   Target-neutral attack validation (v2.1.1)
  docker/                         Docker Compose templates (5 DBs + crawl4ai)
  contracts/                      Reference contracts + settings schema
  intelligence/                   Strategic intelligence cache (per-DB, TTL 30d)
  strategy_registry/              Cross-session attack strategies
  tests/                          Test suite (55 passed, 1 skipped)
  docs/                           Specs + plans + review reports
  settings.json                   Plugin configuration (26+ parameters)
  AGENTS.md                       Agent orchestration rules
  THEORETICAL_FRAMEWORK.md        Research paper
```

---

## Configuration

### settings.json

Key configuration sections:

| Section | Key Parameters | Description |
|---------|---------------|-------------|
| `docker` | `cleanup_on_exit`, per-DB ports | Container lifecycle and port mapping |
| `pipeline` | `default_max_rounds`, `default_min_defects` | Pipeline execution limits |
| `knowledge` | `cache_enabled`, `cache_ttl_hours` | Contract caching (default 168h) |
| `intelligence` | `enabled`, `cache_ttl_hours`, `max_issues`, `max_commits`, `inject_to_*` | Strategic intelligence (default 720h TTL) |
| `evolution` | `enabled`, `strategy_registry_dir` | Cross-session strategy evolution |
| `fan_out` | `enabled`, `seeds_per_agent`, `profiles` | Fan-Out attack dispatch (9 concurrent) |
| `material_passport` | `enabled`, `hash_algorithm`, `reject_on_tamper` | Contract hash integrity |
| `ai_failure_check` | `enabled`, `halt_on`, `reject_on` | 7-mode AI failure detection |

### .mcp.json

Configures the GitHub MCP server used by the novelty judge.

---

## Requirements

| Requirement | Version | Notes |
|-------------|---------|-------|
| **LLM Model** | Claude Sonnet/Opus | Runs via Claude Code |
| Claude Code CLI | Latest | `npm install -g @anthropic-ai/claude-code` |
| Docker Engine | 20+ | Must be running before pipeline start |
| Python | 3.9+ | Used by hooks and helper scripts |
| Disk Space | 10GB+ | For Docker images and results |
| Docker Hub Token | — | **Recommended** for higher rate limits |
| Network Access | — | Must reach target doc sites |
| GitHub Token | — | Optional; enables full novelty judge |

> **Note on CC version**: Subagent dispatch requires Claude Code 2.1.165 in some proxy setups (v2.1.166+ may not inject Task/Agent tools under certain proxies). If dispatch returns `unknown`, pin CC to 2.1.165.

**Python dependencies**: `pip install httpx html2text requests` (used by hooks and helper scripts).

**Web scraping**: WebFetch is blocked by some doc sites. A local Crawl4AI Docker service (`docker/crawl4ai.yml`) is the primary fetcher (WebFetch is the fallback). Crawl4AI needs ~2GB shared memory (`shm_size`) and runs isolated with no host network access — scraping is restricted to documentation sites only.

**Security model**: All attack scripts run in resource-limited Docker containers (`--memory=1g --cpus=2`), with no privileged containers and no host network access. All tokens flow through environment variables.

---

## Evidence Chain Standard

Every confirmed defect must satisfy the **3-ring evidence chain**:

1. **Contract Reference**: The specific constraint violated, with constraint ID from the structured contract
2. **Source URL**: Direct link to the official documentation page defining the constraint
3. **Documentation Link**: (Optional) Source code reference or GitHub issue

Each defect report includes a **Minimal Reproducible Example (MRE)** — a self-contained Python script reproducible in a fresh Docker container.

---

## License

This project is licensed under the [MIT License](LICENSE).
