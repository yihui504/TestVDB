---
description: 启动向量数据库自动化缺陷挖掘流水线
allowed-tools: Read, Write, Bash, Grep, Glob, Task, WebSearch, WebFetch
---

# /testvdb:mine

启动向量数据库自动化缺陷挖掘流水线。

## Usage

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | Target database: `milvus`, `qdrant`, `weaviate`, or `pgvector` |
| `<version>` | Yes | — | Target version (e.g., `v1.13.0`, `v2.4.0`, `pg17`, `1.25.0`) |
| `--max-rounds N` | No | `5` | Maximum mining rounds. Set to `0` for unlimited |
| `--min-defects N` | No | `1` | Minimum defects before early termination |

## Examples

```bash
/testvdb:mine qdrant v1.13.0
/testvdb:mine milvus v2.4.0 --max-rounds 3
/testvdb:mine pgvector pg17 --max-rounds 0
/testvdb:mine weaviate 1.25.0 --min-defects 2
```

## Execution Flow

1. Parse parameters and validate
2. Pre-flight checks (Docker / Python / Disk / Network / Crawl4AI)
3. Check cache (raw_knowledge.md + structured_contract.json)
4. If cache miss: Knowledge Extraction → Contract Formalization
5. Contract Gate Check (core CRUD endpoint coverage ≥ 90%)
6. Initialize mine_state.json
7. Mining loop (up to max_rounds)
8. Generate summary.md
9. Cleanup and mark session complete

## Output

Results written to `results/{db}/{version}/{timestamp}/`:

```
results/qdrant/v1.13.0/2026-06-04T15-30-00Z/
├── defects/defect-1.md
├── mre/defect-1-script.py
├── summary.md
├── debate_logs/stage1.json
├── debate_logs/stage2.json
├── structured_contract.json
└── session_metadata.json
```

## Termination Conditions

1. **Stalemate**: 5 consecutive rounds with no new defects
2. **Coverage**: Contract coverage reaches ≥ 95%
3. **Max Rounds**: `--max-rounds` reached
4. **Min Defects**: `--min-defects` reached

## Multi-DB Mining

Open multiple terminal windows for parallel mining:

```bash
# Terminal 1
/testvdb:mine milvus v2.4.0

# Terminal 2
/testvdb:mine qdrant v1.13.0
```

## Error Recovery

Re-run the same command to resume an interrupted session. The system auto-detects incomplete sessions.

## Prerequisites

- Docker Engine running
- Crawl4AI Docker service (auto-started if `docker/crawl4ai.yml` exists)
- Python 3.9+ with `httpx` and `html2text` (auto-installed if missing)
- Disk space ≥ 10GB
