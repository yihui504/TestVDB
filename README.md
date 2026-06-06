# TestVDB

English | [中文](./README_zh.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Claude Code Plugin](https://img.shields.io/badge/Claude%20Code-Plugin-purple.svg)](https://docs.anthropic.com/en/docs/claude-code)

**Automated Defect Mining for Vector Databases**

TestVDB is an LLM-powered tool that automatically discovers compliance defects in vector databases. It reverse-engineers structured contracts from official documentation, generates targeted attack scripts through multi-agent debate, executes them in Docker sandboxes, and produces verified defect reports with full evidence chains.

Currently supports **Milvus**, **Qdrant**, **Weaviate**, and **pgvector**.

---

## Table of Contents

- [How It Works](#how-it-works)
- [Defect Taxonomy](#defect-taxonomy)
- [Quick Start](#quick-start)
- [Usage](#usage)
- [Architecture](#architecture)
- [Directory Structure](#directory-structure)
- [Configuration](#configuration)
- [Requirements](#requirements)
- [Evidence Chain Standard](#evidence-chain-standard)
- [Rust Implementation](#rust-implementation)
- [License](#license)

---

## How It Works

TestVDB operates as a **Claude Code plugin** with a 6-phase pipeline orchestrated by 12 specialized agents:

```
Phase 1: Knowledge Extraction     -- WebSearch + WebFetch official docs
Phase 2: Contract Formalization    -- Structured JSON contract from raw docs
Phase 3: Attack Script Generation  -- 3 attack agents + Stage 1 peer review debate
Phase 4: Sandbox Execution         -- Docker-isolated script execution
Phase 5: Defect Judgment           -- 4 judge agents + Stage 2 voting debate
Phase 6: Report Generation         -- Defect reports with MRE scripts
```

The pipeline runs iteratively: each round injects a `reflection_context` from the previous round into the attack agents, enabling strategy adaptation. Stalemate detection (5 consecutive rounds with no new defects) triggers document re-search and strategy adjustment.

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

## Quick Start

### 1. Install Claude Code CLI

```bash
npm install -g @anthropic-ai/claude-code
```

### 2. Clone and Run

```bash
git clone https://github.com/yihui504/TestVDB.git
cd TestVDB
claude --plugin-dir .
```

> **Note**: `--plugin-dir .` loads the plugin for the current session only (development/testing). For permanent installation, see [Testing on Claude Code](#testing-on-claude-code).

### 3. Mine Defects

Use the `/mine` command to start the pipeline:

```
/mine milvus v2.6.17
/mine qdrant v1.12.0 --max-rounds 3
/mine weaviate 1.25.0 --min-defects 2
/mine pgvector pg17 --max-rounds 0
```

## Usage

### Command Reference

```
/mine <db> <version> [--max-rounds N] [--min-defects N]
```

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | -- | Target database: `milvus`, `qdrant`, `weaviate`, or `pgvector` |
| `<version>` | Yes | -- | Target version (e.g., `v2.6.17`, `v1.13.0`, `pg17`) |
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

### Output Structure

Results are written to `results/{db}/{version}/{timestamp}/`:

```
results/qdrant/v1.13.0/2026-06-04T15-30-00Z/
  defects/defect-1.md           # Defect report
  mre/defect-1-script.py        # Minimal Reproducible Example script
  summary.md                    # Session summary
  debate_logs/stage1.json       # Attack script peer review logs
  debate_logs/stage2.json       # Judge quartet voting logs
  structured_contract.json      # Generated contract
  session_metadata.json         # Session metadata
```

## Architecture

### 12 Agents

| Agent | Role |
|-------|------|
| **orchestrator** | Pipeline coordinator; dispatches all sub-agents |
| **knowledge-extractor** | Crawls official docs, extracts endpoints/parameters/constraints |
| **contract-formalizer** | Converts raw knowledge into structured JSON contract |
| **attack-boundary** | Generates boundary-value attack scripts |
| **attack-state** | Generates state-transition attack scripts |
| **attack-semantic** | Generates semantic/logic attack scripts |
| **docker-executor** | Manages Docker containers, executes scripts in sandbox |
| **judge-doc** | Validates document reference accessibility and content consistency |
| **judge-evidence** | Validates evidence chain completeness |
| **judge-novelty** | Checks defect novelty against known issues (GitHub) |
| **judge-severity** | Assesses defect severity |
| **reporter** | Generates defect reports with MRE scripts |

### 4 Skills

| Skill | Purpose |
|-------|--------|
| **pipeline** | 6-phase pipeline SOP for the orchestrator |
| **contract-schema** | JSON schema reference for contract formalization |
| **defect-taxonomy** | Four-type defect classification reference |
| **docker-templates** | Docker container templates for each target DB |

### 2-Stage Debate Mechanism

**Stage 1 -- Attack Script Peer Review**: The three attack agents (boundary, state, semantic) independently generate test scripts. Scripts undergo peer review voting before sandbox execution. Only scripts that pass the vote proceed.

**Stage 2 -- Judge Quartet Voting**: After sandbox execution, the four judge agents (doc, evidence, novelty, severity) independently review results. judge-doc runs first as a weight regulator, producing DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH to adjust the strictness of the other three judges. A defect is confirmed when evidence and severity both vote is_defect. Novelty always votes is_defect with novelty_rating metadata, not participating in defect confirmation but affecting report priority.

### Pre-Submit Reverify Gate

Every confirmed defect is re-verified in a fresh Docker container before report generation. This eliminates false positives caused by container state leakage or transient errors.

## Directory Structure

```
TestVDB/
  .claude-plugin/plugin.json       Plugin manifest
  .mcp.json                        MCP server config (GitHub API)
  agents/                          12 agent definitions
    orchestrator.md
    knowledge-extractor.md
    contract-formalizer.md
    attack-boundary.md
    attack-state.md
    attack-semantic.md
    docker-executor.md
    judge-evidence.md
    judge-novelty.md
    judge-severity.md
    judge-doc.md
    reporter.md
  commands/mine.md                 Entry command
  docker/                          Docker Compose templates
    crawl4ai.yml
    milvus.yml
    qdrant.yml
    weaviate.yml
    pgvector.yml
  hooks/hooks.json                  Lifecycle hooks
  skills/                          4 skill definitions
    pipeline/SKILL.md
    contract-schema/SKILL.md
    defect-taxonomy/SKILL.md
    docker-templates/SKILL.md
  contracts/                        Configuration & seed data
    milvus_contract.json           Pre-crawled Milvus documentation
    weaviate_contract.json         Pre-crawled Weaviate documentation
    pgvector_contract.json         Pre-crawled pgvector documentation
    db_strategies.json             Per-DB centralized strategy config
    settings_schema.json           Settings validation schema
  issues/                          Known defect reports
    00-summary.md
    001-*.md ... 007-*.md
    milvus_*.md
    qdrant_*.md
    weaviate_*.md
  scripts/                         Helper scripts
    crawl_fetch.py                 Crawl4AI web scraper (primary)
    hook_runner.py                 Cross-platform Python interpreter resolver
    verify_defects.py
    github_search.py
    prioritizer.py
    developer_attitude.py
    verify/                        Defect verification scripts
      verify_defect3.py
      verify_defect4.py
      verify_extra.py
      verify_extra2.py
      verify_p0b_extended.py
      verify_remaining.py
    cleanup_stop.py                Session cleanup
    emergency_cleanup.py           Emergency container cleanup
    log_execution.py               Execution logging
    notify_check.py                Notification config check
    postcompact_verify.py          Post-compact state recovery
    precompact_save.py             Pre-compact state save
    preflight.py                   Session pre-flight checks
    retry_policy.py                Retry policy reporter
  settings.json                    26 configurable parameters
  THEORETICAL_FRAMEWORK.md         Research paper
```

## Theoretical Framework

The [THEORETICAL_FRAMEWORK.md](./THEORETICAL_FRAMEWORK.md) document describes the research foundations of TestVDB, covering:

- **Natural Language Contract Reverse Engineering** — Extracting machine-executable structured contracts from official documentation
- **Human-in-the-Loop Anti-Hallucination Sandbox** — Physical isolation (Docker) + logical isolation (Four-Type Bug Taxonomy as gatekeeper)
- **Four-Type Bug Taxonomy** — The MECE (Mutually Exclusive, Collectively Exhaustive) defect classification at the core of all agents

## Configuration

### settings.json

26 configurable parameters organized into sections:

| Section | Key Parameters | Description |
|---------|---------------|-------------|
| `docker` | `cleanup_on_exit`, `startup_timeout_seconds`, per-DB ports | Docker container lifecycle and port mapping |
| `github` | `token` | GitHub personal access token for novelty judge |
| `retry` | `max_attempts`, `docker_startup_delay_seconds`, `script_execution_delay_seconds` | Retry and delay policies |
| `pipeline` | `default_max_rounds`, `default_min_defects` | Pipeline execution limits |
| `results` | `base_dir`, `max_sessions` | Output directory and session management |
| `knowledge` | `cache_enabled`, `cache_ttl_hours` | Contract caching (default: 7 days) |
| `notification` | `on_severity`, `webhook_url` | Alert configuration for critical defects |
| `network` | `proxy` | HTTP proxy for network requests |

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

## Requirements

| Requirement | Version | Notes |
|-------------|---------|-------|
| **LLM Model** | Claude Sonnet/Opus or DeepSeek | Use `claude --model sonnet` for Claude. DeepSeek models are also supported. |
| Claude Code CLI | Latest | `npm install -g @anthropic-ai/claude-code` |
| Docker Engine | 20+ | Must be running before pipeline start |
| Python | 3.9+ | Used by hooks and helper scripts |
| Disk Space | 10GB+ | For Docker images and results |
| Docker Hub Token | -- | **Required**. Set `DOCKER_HUB_TOKEN` env var. Unauthenticated requests are rate-limited. Obtain via `echo $TOKEN \| docker login --username $USER --password-stdin` |
| Network Access | -- | WebFetch must reach target doc sites (milvus.io, qdrant.tech, etc.). Corporate proxies must whitelist these domains. |
| GitHub Token | -- | Optional; enables full novelty judge via GitHub API |

## Testing on Claude Code

### Method 1: Session-only loading (recommended for development)

```bash
cd TestVDB
claude --plugin-dir .
```

This loads the plugin for the current session only. Changes to plugin files take effect in the next session.

### Method 2: Permanent local installation

```bash
# Add the plugin directory as a local marketplace
/plugin marketplace add /path/to/TestVDB

# Install the plugin
/plugin install testvdb@TestVDB

# Verify installation
/help
# You should see /testvdb:mine in the command list
```

### Method 3: From GitHub

```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

### Debugging

```bash
# Enable debug mode to see plugin loading details
claude --plugin-dir . --debug

# Check loaded agents
/agents

# Check available commands
/help
```

### Testing the Pipeline

1. Ensure Docker Engine is running: `docker info`
2. Start a session: `claude --plugin-dir .`
3. Run a quick test on Qdrant (simplest setup, single container):
   ```
   /testvdb:mine qdrant v1.13.0 --max-rounds 1
   ```
4. Check results in `results/qdrant/v1.13.0/<timestamp>/`

## Evidence Chain Standard

Every confirmed defect must satisfy the **3-ring evidence chain**:

1. **Contract Reference**: The specific constraint violated, with constraint ID from the structured contract
2. **Source URL**: Direct link to the official documentation page that defines the constraint
3. **Documentation Link**: (Optional) Source code reference or GitHub issue for additional context

Additionally, each defect report includes a **Minimal Reproducible Example (MRE)** -- a self-contained Python script that can be run in a fresh Docker container to reproduce the defect.

## Rust Implementation

The Rust implementation has been moved to the `archive/rust-impl` branch. It contains a standalone implementation written in Rust (edition 2024) that shares the same theoretical framework and defect taxonomy but operates independently of the Claude Code plugin.

To access the archived Rust code:

```bash
git fetch origin archive/rust-impl
git checkout archive/rust-impl
```

## License

This project is licensed under the [MIT License](LICENSE).
