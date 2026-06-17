# Session Lifecycle / Resume Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让中断的挖掘运行（含 Turn1 setup 中断）可被发现、查询进度、精确续跑，新增 `/testvdb:resume` 命令。

**Architecture:** 把 `commands/mine.md` 内联的入口判断逻辑抽成可测模块 `scripts/_entry_dispatch.py`，修复三个 bug（只认 loop / 不按 target 过滤 / 无精确入口）并加 `.resume_target` 标记机制；`session_index.py` 加 `--incomplete`；新增薄壳 `commands/resume.md` 复用 reconstruct + mine 续跑引擎。

**Tech Stack:** Python 3.12（`py -3.12`），pytest，无新依赖。

**Spec:** `docs/superpowers/specs/2026-06-17-session-lifecycle-resume-design.md`

---

## Task 1: 入口判断模块 `scripts/_entry_dispatch.py`（TDD）

**Files:**
- Create: `scripts/_entry_dispatch.py`
- Test: `tests/test_entry_dispatch.py`

将 mine.md:92-127 内联 python 抽成可测函数。修复：① 认 setup 中断 ② 按 target/version 过滤 ③ 读 `.resume_target` 标记优先 ④ `find_incomplete` 供列选/提示。

- [ ] **Step 1: 写失败测试（setup 中断 RESUME + target 过滤 + loop 不回归）**

`tests/test_entry_dispatch.py`:
```python
import json, os, sys, pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts"))
import _entry_dispatch as ed


def _make_session(root, target, version, turn_type, phase, sid="s1", completed=False):
    """在 timestamp 级目录造一个 pipeline_state.json。"""
    sd = os.path.join(root, "results", target, version, "20260617T100000")
    os.makedirs(sd, exist_ok=True)
    ps = {
        "session_id": sid, "target": target, "version_target": version,
        "turn_type": turn_type, "phase": phase, "current_round": 1, "max_rounds": 5,
        "phases_completed": [], "global_state": {"total_defects_confirmed": 0},
    }
    with open(os.path.join(sd, "pipeline_state.json"), "w", encoding="utf-8") as f:
        json.dump(ps, f)
    return sd


def test_setup_interruption_is_resumable(tmp_path, monkeypatch):
    """Bug ①: turn_type=setup 中断应被 RESUME（旧行为只认 loop）。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "weaviate", "v1.38.0", "setup", "EXECUTION")
    d = ed.dispatch("weaviate", "v1.38.0")
    assert d["decision"] == "RESUME"
    assert "EXECUTION" in (d.get("phase", "") + d.get("reason", ""))


def test_target_filter_prevents_wrong_resume(tmp_path, monkeypatch):
    """Bug ②: /mine weaviate 不能 RESUME 到 qdrant 中断（即使 mtime 更新）。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "qdrant", "v1.18.2", "loop", "ATTACK_GEN", sid="q1")
    d = ed.dispatch("weaviate", "v1.38.0")
    assert d["decision"] == "FRESH_START"


def test_loop_resume_not_regressed(tmp_path, monkeypatch):
    """回归: turn_type=loop 中断仍被 RESUME。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "weaviate", "v1.38.0", "loop", "ROUND_START")
    assert ed.dispatch("weaviate", "v1.38.0")["decision"] == "RESUME"


def test_done_phase_skipped(tmp_path, monkeypatch):
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "weaviate", "v1.38.0", "done", "DONE")
    assert ed.dispatch("weaviate", "v1.38.0")["decision"] == "FRESH_START"
```

- [ ] **Step 2: 跑测试验证失败**

Run: `py -3.12 -m pytest tests/test_entry_dispatch.py -v`
Expected: 4 FAIL（`ModuleNotFoundError: _entry_dispatch`）

- [ ] **Step 3: 实现 `scripts/_entry_dispatch.py`**

```python
#!/usr/bin/env python3
"""TestVDB mine 入口判断 — 决定 FRESH_START 还是 RESUME。

从 commands/mine.md 抽出以便测试。修复历史 bug:
  ① 只认 turn_type=loop → 现也认 setup（Turn1 setup turn 中断）
  ② 扫描不按 target/version 过滤 → 现按 /mine 参数过滤（防续错 target）
  ③ 无精确续指定入口 → .resume_target 标记优先（resume 命令设）
"""
from __future__ import annotations
import glob, json, os

DONE_PHASES = {"CLEANUP", "DONE", None}
RESUMABLE_TURN_TYPES = {"loop", "setup"}


def _plugin_root() -> str:
    root = os.environ.get("TESTVDB_PLUGIN_ROOT", "")
    if root and os.path.isfile(os.path.join(root, "commands", "mine.md")):
        return root
    cur = os.getcwd()
    for _ in range(7):
        if os.path.isfile(os.path.join(cur, "commands", "mine.md")):
            return cur
        parent = os.path.dirname(cur)
        if parent == cur:
            break
        cur = parent
    return ""


def _read_json(path: str):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (json.JSONDecodeError, OSError):
        return None


def _resume_target_path(root: str) -> str:
    return os.path.join(root, "results", ".resume_target")


def read_resume_target(root: str):
    """读 .resume_target 标记（resume 命令设）。返回 session_dir 或 None。"""
    data = _read_json(_resume_target_path(root))
    if not data or not data.get("session_dir"):
        return None
    sd = data["session_dir"]
    return sd if os.path.isdir(sd) else None


def consume_resume_target(root: str) -> None:
    """RESUME 后删标记（一次性）。"""
    try:
        os.remove(_resume_target_path(root))
    except OSError:
        pass


def write_resume_target(root: str, session_dir: str, target: str, version: str) -> None:
    """resume 命令调用：写下次要 /mine 续的 session。"""
    os.makedirs(os.path.dirname(_resume_target_path(root)), exist_ok=True)
    with open(_resume_target_path(root), "w", encoding="utf-8") as f:
        json.dump({"session_dir": session_dir, "target": target, "version": version}, f)


def scan_resumable(root: str, target: str, version: str):
    """扫描 results/ 找匹配 target/version 的可恢复中断，按 mtime 降序。

    只扫 timestamp 级目录（**跳过 version 根目录残留的 pipeline_state.json**，Bug ④）。
    """
    matches = []
    for p in glob.glob(os.path.join(root, "results", "**", "pipeline_state.json"), recursive=True):
        rel = os.path.relpath(p, root)
        if rel.count(os.sep) < 4:  # 跳过 version 根目录残留（3 层），只认 timestamp 级（4 层）
            continue
        ps = _read_json(p)
        if not ps:
            continue
        if ps.get("target") != target or ps.get("version_target") != version:
            continue
        if ps.get("turn_type") not in RESUMABLE_TURN_TYPES:
            continue
        if ps.get("phase") in DONE_PHASES:
            continue
        try:
            mtime = os.path.getmtime(p)
        except OSError:
            continue
        matches.append((mtime, os.path.dirname(p), ps))
    matches.sort(key=lambda x: x[0], reverse=True)
    return matches


def find_incomplete(root: str, target: str | None = None, version: str | None = None):
    """列出所有未完成 session（phase∉DONE），供提示/resume 列选。"""
    out = []
    for p in glob.glob(os.path.join(root, "results", "**", "pipeline_state.json"), recursive=True):
        ps = _read_json(p)
        if not ps or ps.get("phase") in DONE_PHASES:
            continue
        if target and ps.get("target") != target:
            continue
        if version and ps.get("version_target") != version:
            continue
        out.append({
            "session_id": ps.get("session_id", "?"),
            "target": ps.get("target", "?"),
            "version": ps.get("version_target", "?"),
            "phase": ps.get("phase", "?"),
            "turn_type": ps.get("turn_type", "?"),
            "session_dir": os.path.dirname(p),
        })
    return out


def dispatch(target: str, version: str, force_new: bool = False) -> dict:
    """主入口判断。

    返回 {decision: FRESH_START|RESUME, session_dir?, phase?, reason, incomplete?}
    - force_new=True: 强制新建（--new），仍列出未完成供知情
    """
    root = _plugin_root()
    if not root:
        return {"decision": "FRESH_START", "reason": "no plugin root", "incomplete": []}

    incomplete = find_incomplete(root, target, version)
    same_target_incomplete = [i for i in incomplete if i["target"] == target and i["version"] == version]

    if force_new:
        return {
            "decision": "FRESH_START", "reason": "force_new (--new)",
            "incomplete": same_target_incomplete,
        }

    # 1. .resume_target 标记优先（resume 命令设，精确续指定）
    rt = read_resume_target(root)
    if rt:
        consume_resume_target(root)
        ps = _read_json(os.path.join(rt, "pipeline_state.json")) or {}
        return {
            "decision": "RESUME", "session_dir": rt,
            "phase": ps.get("phase", "ROUND_START"),
            "reason": f"resume_target 标记 → {rt}",
        }

    # 2. 扫描匹配 target/version 的中断（认 loop+setup，Bug ①②）
    matches = scan_resumable(root, target, version)
    if matches:
        sd, ps = matches[0][1], matches[0][2]
        return {
            "decision": "RESUME", "session_dir": sd,
            "phase": ps.get("phase", "ROUND_START"),
            "reason": f"扫描命中 {ps.get('turn_type')}/{ps.get('phase')}",
            "incomplete": same_target_incomplete,
        }

    return {"decision": "FRESH_START", "reason": "无可恢复中断", "incomplete": same_target_incomplete}
```

- [ ] **Step 4: 跑测试验证通过**

Run: `py -3.12 -m pytest tests/test_entry_dispatch.py -v`
Expected: 4 PASS

- [ ] **Step 5: Commit**

```bash
git add scripts/_entry_dispatch.py tests/test_entry_dispatch.py
git commit -m "feat(entry): 入口判断抽模块 + 修 setup 中断/续错 target bug"
```

---

## Task 2: mine.md 接入 dispatch + `--new` + 提示

**Files:**
- Modify: `commands/mine.md:43-59`（Parameters 加 `--new`）、`commands/mine.md:92-127`（入口判断内联脚本改调 dispatch）

- [ ] **Step 1: Parameters 表加 `--new`**

在 `commands/mine.md` Parameters 表（约 50-58 行）末尾加一行：
```markdown
| `--new` | No | — | 强制新建会话，忽略未完成运行的自动 RESUME（旧中断废弃时用） |
```

- [ ] **Step 2: 入口判断内联脚本替换为调 dispatch**

将 `commands/mine.md:92-127` 整个 `python -c "..."` 代码块替换为：
```bash
python -c "
import sys, os
sys.path.insert(0, os.path.join(os.environ.get('TESTVDB_PLUGIN_ROOT', os.getcwd()), 'scripts'))
import _entry_dispatch as ed
d = ed.dispatch('{target}', '{version}', force_new={'true' if '--new' in ARGV else 'false'})
import json
print(json.dumps(d, ensure_ascii=False))
"
```
（`{target}`/`{version}` 是 Step 1 解析的参数；`ARGV` 是命令行参数列表。主进程读 JSON 输出，按 `decision` 走 FRESH_START 或 RESUME 分支。）

- [ ] **Step 3: 主进程分支逻辑（mine.md 文字说明）**

在入口判断代码块后补充编排指令：
```markdown
读取 dispatch 结果：
- `decision=RESUME` → 从 `session_dir` 执行 [Loop Turn: Resume Round](#loop-turn-resume-round)
  - 若 `incomplete` 非空 → 输出提示："检测到未完成 {session_id}（{phase}），已自动 RESUME；如需新建会话请加 `--new`"
- `decision=FRESH_START` → 执行 [Turn 1: Setup + First Round](#turn-1-setup--first-round)
  - 若 `incomplete` 非空 → 输出提示："检测到未完成 {session_id}（{phase}），建议 `/resume {session_id}`；已按指示新建（--new）"
```

- [ ] **Step 4: 端到端冒烟验证**

构造 setup 中断 session，验证 dispatch 输出：
```bash
py -3.12 -c "import sys; sys.path.insert(0,'scripts'); import _entry_dispatch as ed; print(ed.dispatch('weaviate','v1.38.0'))"
```
Expected: `decision` 为 RESUME 且 session_dir 指向 weaviate 中断（验证真数据命中）。

- [ ] **Step 5: Commit**

```bash
git add commands/mine.md
git commit -m "feat(mine): 入口判断接入 _entry_dispatch + --new + 未完成提示"
```

---

## Task 3: session_index.py 加 `--incomplete`

**Files:**
- Modify: `scripts/session_index.py:75-108`（main 加 flag）
- Test: 复用 `find_incomplete`（Task 1 已测），加薄验证

- [ ] **Step 1: 写失败测试（--incomplete 过滤 DONE）**

`tests/test_entry_dispatch.py` 末尾追加：
```python
def test_find_incomplete_excludes_done(tmp_path, monkeypatch):
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "weaviate", "v1.38.0", "loop", "ATTACK_GEN", sid="running")
    _make_session(tmp_path, "weaviate", "v1.38.0", "done", "DONE", sid="finished")
    inc = ed.find_incomplete(str(tmp_path))
    ids = [i["session_id"] for i in inc]
    assert "running" in ids and "finished" not in ids
```

- [ ] **Step 2: 跑测试**

Run: `py -3.12 -m pytest tests/test_entry_dispatch.py::test_find_incomplete_excludes_done -v`
Expected: PASS（find_incomplete Task 1 已实现）

- [ ] **Step 3: session_index.py main 接入 `--incomplete`**

`scripts/session_index.py` main 中（约 76-87 行）加 flag 与过滤：
```python
    ap.add_argument("--incomplete", action="store_true", help="only phase ∉ DONE (未完成，供 resume 列选)")
    args = ap.parse_args()
    ...
    if args.incomplete:
        sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
        from _entry_dispatch import find_incomplete, _plugin_root
        inc = find_incomplete(_plugin_root(), target=args.target)
        rows = [r for r in rows if r["session_id"] in {i["session_id"] for i in inc}]
```

- [ ] **Step 4: 跑 + 验证**

```bash
py -3.12 scripts/session_index.py --incomplete
```
Expected: 只列出 phase 非 DONE 的 session。

- [ ] **Step 5: Commit**

```bash
git add scripts/session_index.py tests/test_entry_dispatch.py
git commit -m "feat(session_index): --incomplete flag 过滤未完成 session"
```

---

## Task 4: 新增 `/testvdb:resume` 命令

**Files:**
- Create: `commands/resume.md`

- [ ] **Step 1: 写 `commands/resume.md`**

```markdown
---
description: 发现未完成的挖掘运行并续跑
allowed-tools: Read, Write, Bash, Grep, Glob, Agent
---

# /testvdb:resume

发现未完成的挖掘运行（含 Turn1 setup 中断），查询进度并续跑。

> **派发纪律**：续跑实质工作仍经 `Agent(subagent_type=...)`，禁用 `TaskCreate`（详见 `commands/mine.md` 派发工具纪律）。

## Usage

```
/testvdb:resume                  # 列出未完成运行，对话里选一个续
/testvdb:resume <session_id>     # 直接续指定 session
```

## 执行流程

### 形态 A: 无参 — 列出未完成，选一个续

1. 列出未完成运行：
```bash
py -3.12 scripts/session_index.py --incomplete
```
2. 若无未完成 → 输出"无未完成运行"，结束。
3. 若有 → 输出列表 + 提示用户："回复 `/testvdb:resume <session_id>` 续跑其中一个"。

### 形态 B: 带 session_id — 续指定

1. 定位 session_dir（从 session_index 按 id 匹配，或扫描 pipeline_state.json）：
```bash
py -3.12 -c "
import sys, os, glob, json
sys.path.insert(0, 'scripts')
import _entry_dispatch as ed
root = ed._plugin_root()
for p in glob.glob(os.path.join(root,'results','**','pipeline_state.json'), recursive=True):
    d = json.load(open(p, encoding='utf-8'))
    if d.get('session_id') == '{session_id}':
        print(os.path.dirname(p)); break
"
```
2. 设 `.resume_target` 标记（供 /mine 兜底，防重复）：
```bash
py -3.12 -c "
import sys; sys.path.insert(0,'scripts')
import _entry_dispatch as ed
ed.write_resume_target(ed._plugin_root(), '{session_dir}', '{target}', '{version}')
"
```
3. 重建上下文：
```bash
py -3.12 scripts/reconstruct_context.py --session-dir "{session_dir}" --format text
```
4. 按 reconstruct 输出的 `next_action`（resume_from_phase / skip_phases）执行 [commands/mine.md 的 Loop Turn: Resume Round](mine.md#loop-turn-resume-round) 续跑流程。

## 约束

- resume 只做"发现 + 选择 + 设标记 + reconstruct + 转交 mine 续跑"，零新状态机。
- 续跑引擎 = 现有 mine Loop Turn（reconstruct_context Phase 0 + 断点续）。
- 不处理 `phase=DONE` 的已完成会话（DONE 续挖非目标，见 spec 非目标）。
```

- [ ] **Step 2: 验证命令文件可被识别**

```bash
ls commands/resume.md && head -5 commands/resume.md
```
Expected: 文件存在，frontmatter 含 description。

- [ ] **Step 3: Commit**

```bash
git add commands/resume.md
git commit -m "feat(resume): 新增 /testvdb:resume 命令（发现+续跑未完成运行）"
```

---

## Task 5: state 清理 + 文档

**Files:**
- Modify: `scripts/_entry_dispatch.py:scan_resumable`（Task 1 已跳过 version 根目录，确认）
- Cleanup: 删 `results/{target}/{version}/pipeline_state.json`（version 根目录残留）
- Modify: `README.md`、`AGENTS.md`

- [ ] **Step 1: 确认 version 根目录 pipeline_state.json 是残留**

```bash
for f in results/*/v*/pipeline_state.json results/*/*/pipeline_state.json; do
  [ -f "$f" ] && echo "$f: parent=$(basename $(dirname $f))"
done
```
Expected: version 根目录（parent 是 `v1.38.0` 这类，非 timestamp）的那份是残留。对照 timestamp 子目录的确认是权威版。

- [ ] **Step 2: 删除 version 根目录残留**

```bash
# 只删 version 根目录（basename 匹配 vX.Y.Z）的 pipeline_state.json
find results -maxdepth 3 -name pipeline_state.json | while read f; do
  parent=$(basename "$(dirname "$f")")
  if [[ "$parent" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "删残留: $f"; rm -f "$f"
  fi
done
```

- [ ] **Step 3: README 加三件套说明**

`README.md` 运行恢复相关段落（约 271 行 "Re-run the same command to resume" 附近）补充：
```markdown
## Session Lifecycle: 发现 / 查进度 / 续跑

- **列所有运行**: `py -3.12 scripts/session_index.py`（`--incomplete` 只看未完成，`--target X` 过滤）
- **查单运行进度**: `py -3.12 scripts/reconstruct_context.py --session-dir <path>`（phase/round/defects/coverage/下一步）
- **续跑未完成**: `/testvdb:resume`（列选）或 `/testvdb:resume <session_id>`（直接续）；也可 `/mine <db> <ver>` 自动 RESUME 中断（含 Turn1 setup 中断），`--new` 强制新建
```

- [ ] **Step 4: AGENTS.md 同步（若已有恢复章节则更新）**

在 AGENTS.md 适当位置加与 README 一致的三件套指针。

- [ ] **Step 5: 跑全套测试回归**

```bash
py -3.12 -m pytest tests/ -q
```
Expected: 全 PASS（含 Task 1 新增 test_entry_dispatch.py），无回归。

- [ ] **Step 6: Commit**

```bash
git add README.md AGENTS.md scripts/_entry_dispatch.py
git commit -m "docs+chore: 三件套文档 + 删 version 根目录 state 残留"
```

---

## Self-Review

**Spec coverage:**
- 设计1（入口判断修 4 点）→ Task 1（setup+target+标记）+ Task 2（--new+提示）。✓
- 设计2（resume 命令）→ Task 4。✓
- 设计3（文档+state 清理）→ Task 5。✓
- 非目标（/sessions、DONE 续挖）→ 明确不实现。✓

**Placeholder scan:** Step 2 of Task 2 用了 `{target}`/`{version}`/`ARGV` 占位 — 这些是 mine.md Step 1 已解析变量的引用（mine.md 现有风格），非 plan placeholder。其余步骤含完整代码。

**Type consistency:** `dispatch()` 返回 dict 的 key（decision/session_dir/phase/reason/incomplete）在 Task 1-2-4 引用一致。`find_incomplete`/`scan_resumable`/`read_resume_target`/`write_resume_target`/`consume_resume_target` 命名跨 task 一致。

**已知风险确认:** Task 1 `scan_resumable` 用 depth 检查（`rel.count(os.sep) < 4`）跳过 version 根目录残留；Task 5 Step 1-2 显式确认残留性质并清理，双保险。
```
