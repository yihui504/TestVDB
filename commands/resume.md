---
description: 发现未完成的挖掘运行并续跑
allowed-tools: Read, Write, Bash, Grep, Glob, Agent
---

# /testvdb:resume

发现未完成的挖掘运行（含 Turn1 setup 中断），查询进度并续跑。

> **派发纪律**：续跑实质工作仍经 `Agent(subagent_type=...)`，禁用 `TaskCreate`（详见 `commands/mine.md`「派发工具纪律」）。

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

1. 定位 session_dir（按 id 匹配 pipeline_state.json）：
```bash
PYTHONIOENCODING=utf-8 py -3.12 -c "
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
2. 设 `.resume_target` 标记（供后续 `/mine` 兜底，防重复）：
```bash
py -3.12 -c "
import sys; sys.path.insert(0,'scripts')
import _entry_dispatch as ed
ed.write_resume_target(ed._plugin_root(), '{session_dir}', '{target}', '{version}')
"
```
3. 重建上下文：
```bash
PYTHONIOENCODING=utf-8 py -3.12 scripts/reconstruct_context.py --session-dir "{session_dir}" --format text
```
4. 按 reconstruct 输出的 `next_action`（resume_from_phase / skip_phases）执行 [commands/mine.md 的 Loop Turn: Resume Round](mine.md#loop-turn-resume-round) 续跑流程（reconstruct Phase 0 已提供断点，后续派发 Attack/Judge/Reporter 等按 mine.md SOP）。

## 约束

- resume 只做"发现 + 选择 + 设标记 + reconstruct + 转交 mine 续跑"，零新状态机。
- 续跑引擎 = 现有 mine Loop Turn（reconstruct_context Phase 0 + 断点续）。
- 不处理 `phase=DONE` 的已完成会话（DONE 续挖非目标，见 spec 非目标；想接着挖 = 新 `/mine`，experience_handoff 传经验）。
