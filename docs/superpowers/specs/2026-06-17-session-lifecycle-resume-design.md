# TestVDB 运行生命周期管理 — 发现 / 查进度 / 续跑

**日期**: 2026-06-17
**状态**: 设计已批准，待实现
**主题**: session 发现、进度查询、中断恢复的统一入口（含 `/testvdb:resume` 命令）

## 背景

TestVDB 跨 turn 挖掘流水线依赖 `pipeline_state.json`（v3 schema）持久化状态。现有"发现 / 查进度 / 恢复"能力分散在三个工具：

- `scripts/session_index.py` — 列所有 session（含 `--running` / `--target` / `--json`，按 mtime 降序）
- `scripts/reconstruct_context.py` — 重建单 session 上下文（phase / round / defects / coverage / 断点续跑指令），支持 `--session-dir` 省略时 auto-discover
- Stop hook（`scripts/hooks/pipeline_gate.py`）+ mine 入口判断 — 自动续 `turn_type=loop` 的中断

## 问题（实证）

入口判断（`commands/mine.md:108-126`）存在五个缺口：

1. **Turn1 setup 中断不可恢复**：硬编码 `turn_type == 'loop'`（mine.md:119）。Turn1（setup turn）在 Round1 某阶段崩了，`turn_type` 仍为 `setup`，永不被 RESUME。
   - 实证：`results/weaviate/1.38.0/pipeline_state.json` = `EXECUTION/setup/r1/def=0`、`results/qdrant/v1.18.2/pipeline_state.json` = `ROUND_START/setup/r1/def=0`，均卡住且无自动恢复。
2. **扫描不按 target 过滤**：按 mtime 降序取第一个 `pipeline_state.json`，不校验 target/version。`/mine weaviate` 可能 RESUME 到 qdrant 的中断运行（若其 mtime 最新）—— 续错 target。
3. **无精确指定续跑入口**：用户无法指定"续这个 session"，只能靠入口判断续最新。
4. **state 冗余**：version 根目录 + timestamp 子目录各一份 `pipeline_state.json`，扫描可能命中错那份。
5. **散装工具无统一入口**：session_index / reconstruct 能力都在，但无 `/resume` 命令串成"发现 → 查 → 续"流程；用户得手敲 CLI。

## 目标

- Turn1 setup 中断可恢复
- 多中断运行时精确续指定 session（不续错 target）
- 统一 `/testvdb:resume` 命令：发现未完成 → 选 → 续（无参列选 + 带 id 直接续）
- 文档明确三件套（session_index / reconstruct / resume）关系

## 非目标（YAGNI，明确排除）

- **`/sessions` 命令族**：`session_index.py` CLI 已够，包成 slash 命令是 speculative。用户想看跑一行 `py scripts/session_index.py`。否决条件：当发现自己频繁手敲 session_index 觉得烦时再包。
- **DONE 续挖**：已完成会话（`phase=DONE`）上继续挖。理由：weaviate 25-defect 已饱和，边际收益低；`experience_handoff.json` 已把经验传新 session。想接着挖 = 新 `/mine`（继承经验）。否决条件：若某 session coverage 远未饱和且明确要接着那批缺陷挖，再单议。

## 设计

### 1. mine 入口判断修复（`commands/mine.md:108-126`）

- **放宽 turn_type**：`== 'loop'` → `in ('loop', 'setup')` 且 `phase ∉ {CLEANUP, DONE, None}`
- **按 target/version 过滤**：扫描兜底只匹配 `/mine <db> <ver>` 指定的 target/version（防续错 target）
- **认 `TESTVDB_RESUME_DIR` 优先**：resume 命令设此 env → 入口判断优先 RESUME 该 session_dir，跳过扫描（精确续指定）
- **同 target/version 有未完成运行时提示（不阻断）**：`/mine` 检测到已有未完成 → 输出"检测到未完成 `{session_id}`（phase=X），建议 `/resume {session_id}`；如确认新建会话则继续"。防静默重复新建。

### 2. 新增 `/testvdb:resume` 命令（薄壳，零新状态机）

`commands/resume.md`：

- **无参 `/resume`**：
  1. 调 `session_index.py`（过滤 `phase ∉ DONE`，按 target 分组）列出未完成运行
  2. 提示用户在对话里指定要续的 `session_id`
  3. 用户回复后 → 定位 session_dir → 设 `TESTVDB_RESUME_DIR` → 进 mine 的 RESUME 路径（`reconstruct_context.py` Phase 0 + 断点续）
- **`/resume <session_id>`**：
  1. 定位 session_dir（session_index 按 id 匹配）
  2. 设 `TESTVDB_RESUME_DIR` → 进 mine RESUME

续跑引擎 = 现有 mine Loop Turn，零新数据结构。resume 只做"发现 + 选择 + 设 env + 转交 mine 续跑"。

### 3. 文档 + state 清理

- `README.md` / `AGENTS.md`：写明 **session_index（发现）→ reconstruct（查进度）→ resume（续跑）** 三件套关系与典型工作流
- state 冗余：确认 `results/{target}/{version}/pipeline_state.json`（version 根目录）是历史残留 → 删除；入口判断只扫 timestamp 级目录（防命中错那份）

## 已知实现风险（实现时细化，不阻塞设计）

- **Turn1 setup 中断的断点精确度**：Turn1 = Step1-7（setup）+ Round1（8a-8j）混在一个 turn。若在 Step1-7（SETUP phase）中断，`reconstruct_context._get_next_phase` 可能从 ROUND_START 重启、重做 setup。实现时验证 reconstruct 对 SETUP phase 的处理，必要时小调。
- **resume 转交续跑的机制**：`TESTVDB_RESUME_DIR` env 在 slash 命令间是否传播需验证。若不可靠，改用标记文件（如 `results/.resume_target`，含 session_dir），入口判断读文件优先 —— 文件比 env 跨命令更稳。实现时择优。
- **resume 是独立 slash 命令，无法"调用"另一个 slash 命令**：resume.md 需内嵌 mine 的 RESUME 分支逻辑（reconstruct + Loop Turn 续跑指令），或引导用户接着跑 `/mine`（入口判断读标记）。实现时定。

## 验证

- 入口判断修复：构造 `turn_type=setup` + 各 phase 的 `pipeline_state.json`，验证 RESUME 命中正确 session（含 target 过滤、不续错）
- resume 命令：无参列出未完成、带 id 续指定、续错 target 防护
- state 清理后：version 根目录无残留 `pipeline_state.json`，入口判断只扫 timestamp 级
- 回归：现有 `turn_type=loop` 中断恢复不受影响

## 不变的核心约束

- 主进程只编排，续跑实质工作仍经 `Agent(subagent_type=...)` 派发（见 `commands/mine.md` 派发工具纪律）
- `pipeline_state.json`（v3）仍是跨 turn 唯一状态源
