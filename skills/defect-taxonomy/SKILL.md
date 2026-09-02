---
name: defect-taxonomy
description: TestVDB four-type defect taxonomy reference. Auto-loaded when Judge or Attack agents need to determine defect types.
version: 1.0.0
---

# Defect Taxonomy Reference

## Trigger conditions

Auto-loaded when Judge agents (evidence/novelty/severity) review candidate defects. Available to Attack agents when generating tests. Not user-triggered.

## The four-type defect taxonomy

### Type 1: Illegal Success

**Definition**: input violating a documentation constraint is accepted by the database (returns 200/201 instead of 400/422).

**Examples**:
- `limit=-1` returns 200 OK instead of 400
- missing required parameter `vector` returns 200 with an empty result instead of an error
- `distance` with an unsupported metric returns 201 Created

**Check pattern**: expect 4xx → got 2xx

### Type 2: Poor Diagnostics

**Definition**: the database correctly rejects bad input (returns 4xx/5xx), but the error message is not clear enough.

**Diagnostics quality rubric** (3 points):
1. The parameter name is mentioned (1pt)
2. The correct format/range is stated (1pt)
3. An actionable fix suggestion (1pt)

**Threshold**: score < 2 = Type-2 defect

### Type 3: Runtime Failure

**Definition**: legal input causes the database to crash, return 500, or behave anomalously.

**Examples**:
- a legal search request returns 500 Internal Server Error
- a specific vector dimension crashes the container
- concurrent operations cause a deadlock that is not handled correctly

### Type 4: State/Logic Violation

**Definition**: the API responds correctly (200 OK), but the data state or semantic result is inconsistent.

**Examples**:
- INSERT 3 rows, COUNT returns 2
- DELETE collection, search still returns data
- UPDATE vector, search returns old results
- ordering inconsistent with vector distances

## Classification decision tree

```
1. Is legal input being rejected?
   ├── yes → Type 1 reverse (Illegal Rejection)
   └── no → 2

2. Is illegal input being accepted?
   ├── yes → Type 1 (Illegal Success)
   └── no → 3

3. Does legal input cause a crash/500?
   ├── yes → Type 3 (Runtime Failure)
   └── no → 4

4. Is the error message unclear?
   ├── yes → Type 2 (Poor Diagnostics)
   └── no → 5

5. Is the state/result inconsistent?
   ├── yes → Type 4 (State/Logic Violation)
   └── no → reclassify or not a defect
```

## 7-Mode AI Failure Checklist (v2.0)

The reporter's self-check mechanism run before the Pre-Submit Gate. See `scripts/ai_failure_check.py` for details.

| Mode | What is checked | Detection method | Triggered action |
|------|---------|---------|---------|
| M1 | script error misjudged as a database defect | check execution_summary.txt | informational |
| M2 | fabricated documentation citation (hallucinated URL) | curl source_url | REJECT |
| M3 | fabricated execution-result data | compare against output_*.log | REJECT |
| M4 | shortcut that skips key verification | check .done markers | HALT |
| M5 | script bug dressed up as a new finding | classification-consistency check | REWIND |
| M6 | fabricated methodology | attack-agent output consistency | REJECT |
| M7 | locked onto an early wrong hypothesis | endpoint repeatedly rejected | HALT |
