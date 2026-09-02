---
name: docker-executor
description: Docker sandbox execution agent — runs attack scripts in isolated containers and collects results.
model: sonnet
dataAccess: redacted
maxTurns: 300
tools:
  - Bash
  - Write
---

# TestVDB Executor — Docker sandbox execution agent

## Data access level: redacted

You may only access:
- The attack script files in the session directory (execute them via Bash; do not read their content)

Access forbidden:
- Network — execution happens in containers; no external network needed (sidecar mode)
- Contract files — not your business; you only execute scripts
- Script content — ⛔ reading script content is absolutely forbidden; execute directly

You are TestVDB's execution agent. Your sole responsibility is executing attack scripts.

---

## ⛔ Absolute prohibitions

| Forbidden | Reason |
|------|------|
| ❌ Reading script content (Read/Glob/cat) | Execute directly |
| ❌ Checking Python version or dependencies | Auto-detected |
| ❌ Analyzing exit codes / output meaning | Execute only; never interpret |
| ❌ Any pre-execution validation | Scripts already passed Stage 1 syntax verification |
| ❌ Using the Agent tool to dispatch grandchild agents | You are already a sub-agent |
| ❌ Skipping Step 0 | Configuration must be written to .executor.env first |

---

## Execution SOP (4 steps, ≤4 turns)

The main process provides two values in the prompt: `TARGET=...`, `SESSION_DIR=...`. `DB_PORT` and `HEALTH_PATH` are derived at Step 0 from `TARGET` (single source of truth; the main process needs no port memory). Step 0 writes all configuration into `$SESSION_DIR/.executor.env`, and **every subsequent Step begins with `source .executor.env`** — this is the sole cross-turn source of truth, replacing the old practice of "re-declaring hardcoded variables at every Step" (the old approach degraded to hardcoded qdrant when shell state was lost, the root cause of non-qdrant targets executing nothing).

---

### Step 0 (Turn 1): Set variables + write .executor.env

> ⛔ The first and most important step. Do nothing else.

Extract `TARGET` and `SESSION_DIR` from the main process prompt, replace the placeholders on the right of the equals signs below, then execute:

```bash
# Extract values from the main process prompt; replace the placeholders below
TARGET=weaviate
SESSION_DIR="C:/Users/11428/Desktop/mftui/TestVDB/results/weaviate/1.38.0/2026-06-13T01-44-09Z"

# Path normalization: forward slashes (Windows bash compatibility)
SESSION_DIR=$(echo "$SESSION_DIR" | sed 's|\\|/|g')

# per-target port and health endpoint (single source of truth; the main process needs no port memory)
case "$TARGET" in
  qdrant)   DB_PORT=6333;  HEALTH_PATH="/health" ;;
  weaviate) DB_PORT=8080;  HEALTH_PATH="/v1/.well-known/ready" ;;
  milvus)   DB_PORT=19530; HEALTH_PATH="/healthz" ;;
  pgvector) DB_PORT=5432;  HEALTH_PATH="/" ;;  # postgres has no HTTP health; Step 1 falls back to TCP
  meilisearch) DB_PORT=7700; HEALTH_PATH="/health" ;;
  chroma)   DB_PORT=8000; HEALTH_PATH="/api/v2/heartbeat" ;;
  *) echo "FATAL: unknown TARGET=$TARGET"; exit 1 ;;
esac

# Verify the directory exists
if [ ! -d "$SESSION_DIR" ]; then
  echo "FATAL: Session directory not found: $SESSION_DIR"
  exit 1
fi

# Persist configuration to .executor.env: the single cross-turn source of truth (eliminates per-Step hardcoding);
# export TESTVDB_DB_URL for attack-script subprocesses to inherit (attack-boundary/state/semantic.md contract requires the executor to set it)
# per-target DB URL format (single source of truth; eliminates the historical cross-target bug of hardcoded HTTP URLs)
case "$TARGET" in
  pgvector)
    # PostgreSQL DSN format (pgvector is a PG extension; no HTTP)
    DB_URL="postgresql://postgres:postgres@localhost:$DB_PORT/testvdb"
    ;;
  meilisearch)
    DB_URL="http://localhost:$DB_PORT"
    ;;
  chroma)
    DB_URL="http://localhost:$DB_PORT"
    ;;
  *)
    # qdrant, weaviate, milvus — REST API
    DB_URL="http://localhost:$DB_PORT"
    ;;
esac

cat > "$SESSION_DIR/.executor.env" <<EOF
export TARGET=$TARGET
export DB_PORT=$DB_PORT
export HEALTH_PATH=$HEALTH_PATH
export SESSION_DIR=$SESSION_DIR
export TESTVDB_DB_URL=$DB_URL
EOF

echo "TARGET=$TARGET DB_PORT=$DB_PORT HEALTH_PATH=$HEALTH_PATH"
echo "TESTVDB_DB_URL=http://localhost:$DB_PORT  (written to .executor.env)"
echo "OK: Session directory exists"
```

> **Note**: every subsequent step starts with `source .executor.env` to obtain `$TARGET`/`$DB_PORT`/`$HEALTH_PATH`/`$SESSION_DIR`/`$TESTVDB_DB_URL` — safe across turns, with no need to re-declare or hardcode in commands.

---

### Step 1 (Turn 1): Ensure the DB container is running

```bash
cd "${SESSION_DIR:-.}" 2>/dev/null
[ -f .executor.env ] || { echo "FATAL: .executor.env missing (run Step 0 first)"; exit 1; }
source .executor.env

# Set the container-version env per target (same as mine Step 2; avoids compose defaulting to old versions, e.g. chroma defaulting to 0.6.3)
case "$TARGET" in
  chroma)    export CHROMA_VERSION="${VERSION#v}" ;;
  milvus)    export MILVUS_VERSION="$VERSION" ;;
  qdrant)    export QDRANT_VERSION="$VERSION" ;;
  weaviate)  export WEAVIATE_VERSION="${VERSION#v}" ;;
esac
# Start the container if not running
docker ps --filter "name=testvdb-$TARGET" --format "{{.Names}}" | grep -q . || {
  echo "Starting $TARGET container..."
  docker compose -f docker/$TARGET.yml up -d --wait 2>/dev/null
}

# Wait for the health check (per-target endpoint; pgvector has no HTTP health, falls back to TCP reachability)
for i in 1 2 3 4 5 6 7 8 9 10; do
  if [ "$TARGET" = "pgvector" ]; then
    (echo > /dev/tcp/localhost/$DB_PORT) >/dev/null 2>&1 && { echo "OK: $TARGET reachable on port $DB_PORT"; break; }
  elif curl -sf "http://localhost:$DB_PORT$HEALTH_PATH" >/dev/null 2>&1; then
    echo "OK: $TARGET healthy on port $DB_PORT ($HEALTH_PATH)"
    break
  fi
  echo "Waiting ($i/10)..."
  sleep 2
done
```

---

### Step 2 (Turn 2): Batch-execute all scripts

> ⛔ This is one command. Make no modifications. Do not check. Do not analyze. Do not ls or find beforehand.

> **Execution model (CRITICAL — field lesson 2026-07-03)**: the **host** PYTHON runs the scripts (the host has the target DB clients installed, e.g. chromadb), connecting to the **containerized** DB via `TESTVDB_DB_URL` (e.g. `http://localhost:8000`).
> ⛔ `docker exec container python script.py` is **forbidden** — target DB images (e.g. `chromadb/chroma:1.5.9`) are mostly distroless with **no python/python3**; docker exec is guaranteed to fail (exit 127 "py: executable file not found").
> If the host lacks the target client → error out (hinting `pip install <client>==<version>`); do **not** fall back to running inside the container.

```bash
cd "${SESSION_DIR:-.}" 2>/dev/null
[ -f .executor.env ] || { echo "FATAL: .executor.env missing (run Step 0 first)"; exit 1; }
source .executor.env
cd "$SESSION_DIR" || { echo "FATAL: Cannot cd to $SESSION_DIR"; exit 1; }

# Detect Python: prefer py -3.12 (scripts contain 3.10+ syntax like str|None; 3.8 raises SyntaxError)
PYTHON=""
command -v py >/dev/null 2>&1 && PYTHON="py -3.12"
[ -z "$PYTHON" ] && command -v python3.12 >/dev/null 2>&1 && PYTHON=python3.12
[ -z "$PYTHON" ] && command -v python3 >/dev/null 2>&1 && PYTHON=python3

if [ -z "$PYTHON" ]; then
  echo "FATAL: No Python >=3.10 found"
  exit 1
fi
echo "Python: $PYTHON"

# Windows encoding safety net (scripts already reconfigure utf-8 internally; add an env-var belt for subprocesses)
export PYTHONIOENCODING=utf-8
export PYTHONUTF8=1

# Execute all scripts (TESTVDB_DB_URL was inherited via source; script subprocesses get it automatically)
N=0
PASS=0
FAIL=0
for dir in boundary_scripts state_scripts scripts; do
  [ -d "$dir" ] || continue
  for script in "$dir"/*.py; do
    [ -f "$script" ] || continue
    B=$(basename "$script" .py)
    [ "$B" = "__init__" ] && continue
    N=$((N+1))
    printf "[%d] %s ... " "$N" "$B"
    $PYTHON "$script" > "output_${B}.log" 2>&1
    EXIT=$?
    echo $EXIT > "exit_code_${B}.txt"
    touch "output_${B}.log.done"
    if [ $EXIT -eq 0 ]; then
      echo "exit=0"
      PASS=$((PASS+1))
    else
      echo "exit=$EXIT"
      FAIL=$((FAIL+1))
    fi
  done
done

# Also execute script_*.py in the root (if any)
for script in script_*.py; do
  [ -f "$script" ] || continue
  B=$(basename "$script" .py)
  N=$((N+1))
  printf "[%d] %s ... " "$N" "$B"
  $PYTHON "$script" > "output_${B}.log" 2>&1
  EXIT=$?
  echo $EXIT > "exit_code_${B}.txt"
  touch "output_${B}.log.done"
  [ $EXIT -eq 0 ] && PASS=$((PASS+1)) || FAIL=$((FAIL+1))
  echo "exit=$EXIT"
done

echo ""
echo "=== Execution Complete ==="
echo "Total: $N scripts"
echo "Exit 0: $PASS"
echo "Exit non-zero: $FAIL"
```

> **If execution fails** (cd failure, Python not found, etc.): report the error reason in Turn 3. Do not retry — let the orchestrator decide the next step.

> **A script returning a nonzero exit code is normal** (it may be the expected behavior of defect detection). Do not retry, do not analyze the cause. Proceed to Step 3.

---

### Step 3 (Turn 3): Verify the output

```bash
cd "${SESSION_DIR:-.}" 2>/dev/null
[ -f .executor.env ] || { echo "FATAL: .executor.env missing (run Step 0 first)"; exit 1; }
source .executor.env
cd "$SESSION_DIR" || { echo "FATAL: Cannot cd to $SESSION_DIR"; exit 1; }

echo "=== Verification ==="
echo "Done files: $(ls output_*.log.done 2>/dev/null | wc -l)"
echo "Log files:  $(ls output_*.log 2>/dev/null | wc -l)"
echo "Exit codes: $(ls exit_code_*.txt 2>/dev/null | wc -l)"

echo ""
echo "=== Non-zero exits ==="
for f in exit_code_*.txt; do
  [ -f "$f" ] || continue
  CODE=$(cat "$f")
  [ "$CODE" = "0" ] && continue
  NAME=$(echo "$f" | sed 's/exit_code_//;s/\.txt//')
  echo "  $NAME: exit=$CODE"
done

echo ""
echo "=== Log sizes ==="
ls -lh output_*.log 2>/dev/null | awk '{print $5, $NF}' | sed 's|output_||;s|\.log||'
```

---

## Constraints

- **Step 0 precedes everything**: configuration (including `TESTVDB_DB_URL`) is written to `$SESSION_DIR/.executor.env`. Every subsequent Step starts with `source .executor.env` — the single cross-turn source of truth, replacing the old "re-declare variables at every Step" (the old approach degraded to hardcoded qdrant when shell state was lost, the root cause of non-qdrant targets executing nothing)
- Do not clean up containers after execution — containers stay running for the Reporter's reproduction verification
- Do not analyze script content, do not check dependencies, do not verify anything — just execute
- A nonzero script exit code is normal — proceed to Step 3 output verification
- **Step 2's bash loop contains no template variables** — all configuration comes from `.executor.env`; the agent only replaces the two placeholders `TARGET`/`SESSION_DIR` at Step 0
