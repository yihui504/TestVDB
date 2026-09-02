---
name: orchestrator-lifecycle
description: Orchestrator lifecycle management — error handling, context-compaction protection, progress visibility, multi-DB parallelism.
---

# TestVDB Orchestrator — lifecycle management

> Auxiliary spec referenced by `orchestrator.md`. Defines the error-handling policy, context-compaction protection, progress visibility, and multi-DB parallelism guidance.

---

## Error handling

### Tiered policy
| Error type | Retries | Backoff | On final failure |
|---------|---------|---------|-----------|
| Docker startup | 5 | 10s increasing | **Terminate the session** |
| Script execution | 5 | 3s increasing | Skip that script |
| Documentation fetch | 5 | 5s increasing | Skip that endpoint |
| Invalid LLM output format | 5 | immediate | Degrade to a low-confidence marker |

All errors are recorded in error_log.json → aggregated into session_metadata.json at session end.

---

## Context protection — ScheduleWakeup Loop + hook safety net

### Primary mechanism: the ScheduleWakeup cross-turn loop

The pipeline uses a **ScheduleWakeup-driven cross-turn iteration model**. Each mining round is an independent turn:

1. **Turn 1 (FRESH_START)**: Steps 1-7 (setup) + Round 1 (8a→8j) → ScheduleWakeup triggers Turn 2
2. **Turn N (RESUME)**: `reconstruct_context.py` rebuilds context from disk → Round N (8a→8j) → ScheduleWakeup or terminate
3. **Final turn**: Steps 9-10 (summary + cleanup)

**Context is rebuilt at the start of every round**:
```bash
python scripts/reconstruct_context.py --session-dir "{session_dir}" --format text
```
The output includes: current phase, completed phases, this round's key information, global progress, next action.

**State-machine driven**: `pipeline_state.json` (v3 schema) is the sole cross-turn state source. It is updated immediately after each phase completes, ensuring resume precision down to the step.

**Intra-round checkpoint recovery**: if compaction triggers mid-turn (in the middle of 8a→8j):
- the `phases_completed` list records the completed phases
- `phase_data` records each phase's output summary
- the loop-turn entry automatically skips completed phases and resumes from the breakpoint

### Safety net: PreCompact / PostCompact hooks

Hooks protect the intra-round compaction scenario as a **last resort**. The loop-turn entry's `reconstruct_context.py` is the primary recovery mechanism.

#### PreCompact
`precompact_save.py` saves `pipeline_state.json` (with precise breakpoint info) to `.checkpoints/`. Behavior unchanged.

#### PostCompact
`postcompact_verify.py` reads `pipeline_state.json` (v3 schema) and outputs:
- current phase + completed phases
- precise recovery instructions (which phase to resume from, which to skip)
- if turn_type=loop, a prompt to run `reconstruct_context.py` for full context

The PostCompact output is injected as a `<system-reminder>`; the post-compaction agent can continue the current turn from it.

### Phase state machine

```
ROUND_START → ATTACK_GEN → DEBATE_S1 → EXECUTION → DEBATE_S2 → 
REPORTING → DEFECT_REVIEW → STATE_SAVE → 
  ├─ ScheduleWakeup → ROUND_START (next round)
  └─ CLEANUP → DONE
```

After each phase completes, three key fields of `pipeline_state.json` are updated:
- `phase`: the next phase name
- `phases_completed`: append the current phase
- `phase_data.{current phase}`: record the output summary

---

## Progress visibility

### stdout live logging
Print to stdout immediately at round start/end and on defect discovery:
```
[Round 1/5] Starting Test Generation...
[Round 1/5] Attack Trio: 3 agents dispatched
[Round 1/5] Debate Stage 1: 12/15 scripts passed (3 rejected)
[Round 1/5] Executor: 12 scripts running in sandboxes...
[Round 1/5] Execution complete: 6 passed, 4 failed, 2 error
[Round 1/5] Debate Stage 2: 2 defects confirmed (DataCorruption×1, StateLogicViolation×1)
[Round 1/5] DEFECT FOUND: DataCorruption in /collections/{name} (confidence=0.92)
```

### mine_state.json
Persistent state file; progress can be checked at any time.

### Monitors (independent daemon processes)
- Docker crash monitor: detects abnormal container exits, triggers recovery automatically
- Results-directory monitor: detects new defect files, triggers notifications

---

## Multi-DB parallelism guidance

This Orchestrator processes one DB at a time. To mine multiple DBs simultaneously, open multiple terminal windows and run in parallel:
```bash
# Terminal 1
/testvdb:mine milvus v2.4.0
# Terminal 2
/testvdb:mine qdrant v1.13.0
```
