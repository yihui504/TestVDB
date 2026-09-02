---
description: Discover unfinished mining runs and resume them
allowed-tools: Read, Write, Bash, Grep, Glob, Agent
---

# /testvdb:resume

Discover unfinished mining runs (including Turn-1 setup interruptions), query progress, and resume.

> **Dispatch discipline**: substantive resumed work still goes through `Agent(subagent_type=...)`; `TaskCreate` is disabled (see `commands/mine.md` "dispatch tool discipline").

## Usage

```
/testvdb:resume                  # list unfinished runs; pick one in conversation
/testvdb:resume <session_id>     # resume the specified session directly
```

## Execution flow

### Form A: no argument — list unfinished, pick one

1. List unfinished runs:
```bash
py -3.12 scripts/session_index.py --incomplete
```
2. If none → output "no unfinished runs", end.
3. If any → output the list + prompt the user: "reply `/testvdb:resume <session_id>` to resume one".

### Form B: with session_id — resume the specified session

1. Locate session_dir (reuses `_entry_dispatch.find_by_session_id`, avoiding duplicate globbing):
```bash
py -3.12 -c "
import sys; sys.path.insert(0,'scripts')
import _entry_dispatch as ed
print(ed.find_by_session_id(ed._plugin_root(), '{session_id}') or 'NOT_FOUND')
"
```
2. Set the `.resume_target` marker (for later `/mine` fallback; prevents duplicates):
```bash
py -3.12 -c "
import sys; sys.path.insert(0,'scripts')
import _entry_dispatch as ed
ed.write_resume_target(ed._plugin_root(), '{session_dir}', '{target}', '{version}')
"
```
3. Rebuild context:
```bash
PYTHONIOENCODING=utf-8 py -3.12 scripts/reconstruct_context.py --session-dir "{session_dir}" --format text
```
4. Per the reconstruct output's `next_action` (resume_from_phase / skip_phases), execute the resume flow of [commands/mine.md's Loop Turn: Resume Round](mine.md#loop-turn-resume-round): reconstruct Phase 0 has already provided the breakpoint; the main process executes that round's next_action (dispatching Attack/Judge/Reporter etc. per the mine.md SOP), and **actively ends the current turn when done** — the `pipeline_gate.py` Stop hook detects `phase != DONE` → `exit 2` → the harness automatically opens a new turn to continue subsequent rounds (identical to the normal mine loop turn; no manual driving needed).

## Constraints

- resume only does "discover + select + set marker + reconstruct + hand off to mine"; zero new state machines.
- The resume engine = the existing mine loop turn (reconstruct_context Phase 0 + breakpoint resume).
- Sessions with `phase=DONE` are not handled (resuming DONE sessions is a non-goal, see spec non-goals; to keep mining = a new `/mine`, with experience passed via experience_handoff).
