---
description: 启动向量数据库自动化缺陷挖掘流水线
allowed-tools: Read, Write, Bash, Grep, Glob, Agent, ScheduleWakeup
---

# /testvdb:mine

启动向量数据库自动化缺陷挖掘流水线。

---

## ⚠️ 架构约束（CRITICAL — 技术原因）

**Claude Code 插件体系的技术限制：子 Agent 无法可靠地嵌套派发孙 Agent。**

这意味着：
- ✅ 主进程 → `testvdb:knowledge-extractor`（可以——主进程直接派发）
- ✅ 主进程 → `testvdb:orchestrator`（可以——但 orchestrator 内部派发孙 Agent 不可靠）
- ❌ orchestrator(子) → `testvdb:knowledge-extractor`(孙)（不可靠——agent_type 会被丢失为 "unknown"）

**因此本命令的设计：主进程直接担任编排者角色，按照 `agents/orchestrator.md` 的 SOP 逐步派发子 Agent。**
`testvdb:orchestrator` 的 agent 定义保留为 SOP 参考文档。

---

## ⛔ 核心铁律

**主进程永远只做编排，不做执行。违反任何一条流水线立即故障。**

| 禁止事项 | 正确做法 |
|---------|---------|
| ❌ 使用 WebSearch/WebFetch 爬取文档 | ✅ `Agent(subagent_type="testvdb:knowledge-extractor")` |
| ❌ 自己生成 structured_contract.json | ✅ `Agent(subagent_type="testvdb:contract-formalizer")` |
| ❌ 自己写 Python 攻击脚本 | ✅ `Agent(subagent_type="testvdb:attack-boundary/state/semantic")` |
| ❌ 自己运行 Python 脚本或 curl | ✅ `Agent(subagent_type="testvdb:docker-executor")` |
| ❌ 自己判断缺陷有效性 | ✅ `Agent(subagent_type="testvdb:judge-*")` |
| ❌ 自己生成缺陷报告 | ✅ `Agent(subagent_type="testvdb:reporter")` |

**主进程只使用这些工具做编排工作：** `Read`(读文件), `Write`(写状态文件), `Bash`(验证产出), `Grep`(搜索), `Glob`(匹配), `Agent`(派发子Agent), `ScheduleWakeup`(跨 turn 调度)。

---

## Usage

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, 或 `pgvector` |
| `<version>` | Yes | — | 目标版本号 |
| `--max-rounds N` | No | `5` | 最大挖掘轮数。`0` = 无上限 |
| `--min-defects N` | No | `1` | 最低缺陷产出要求 |

---

## 执行模型：ScheduleWakeup 跨 Turn Loop

> **📖 完整 SOP 参考**: `agents/orchestrator.md`（阶段详解、投票规则、错误处理）、`skills/pipeline/SKILL.md`（六阶段流水线规范）。本文件只保留编排调度命令，不重复 SOP 描述。

本命令采用 **ScheduleWakeup 驱动的跨 Turn 迭代模型**，每轮挖掘是一个独立的 Turn：

```
Turn 1 (FRESH_START):  Step 1-7 (setup) + Round 1 (8a→8j) + ScheduleWakeup
Turn N (RESUME):       reconstruct_context.py → Round N (8a→8j) + ScheduleWakeup
Final Turn:            Step 9-10 (汇总 + 清理)
```

**状态持久化**：`pipeline_state.json`（v3 schema）是跨 Turn 的唯一状态源。每个 phase 完成后立即更新，确保断点恢复精确到步骤。

---

## 执行入口

### 入口判断

每次 Turn 开始时，首先执行入口判断：

```bash
python -c "
import json, sys
# 检查当前目录和 session 目录
for d in ['results', 'intelligence']:
    import os
    for root, dirs, files in os.walk(d):
        if 'pipeline_state.json' in files:
            candidate = os.path.join(root, 'pipeline_state.json')
            try:
                with open(candidate, encoding='utf-8') as f:
                    ps = json.load(f)
                if ps.get('turn_type') == 'loop' and ps.get('phase') not in ('CLEANUP', 'DONE', None):
                    print('RESUME')
                    print(ps.get('phase', 'ROUND_START'))
                    print(candidate)
                    sys.exit(0)
            except (json.JSONDecodeError, OSError):
                continue
print('FRESH_START')
"
```

- **FRESH_START** → 执行 [Turn 1: Setup + First Round](#turn-1-setup--first-round)
- **RESUME {phase} {path}** → 执行 [Loop Turn: Resume Round](#loop-turn-resume-round)

---

## Turn 1: Setup + First Round

> 仅在 FRESH_START 时执行。完成所有初始化工作后进入第一轮挖掘。

### Step 1: 解析参数
- 验证 `target` ∈ {milvus, qdrant, weaviate, pgvector}
- 解析 `version`, `max_rounds`, `min_defects`
- 确定 `PROJECT_ROOT`: `git rev-parse --show-toplevel 2>/dev/null || pwd`

### Step 2: 前置条件检查
自行检查 Docker/Python/磁盘/网络：
```bash
python scripts/preflight.py
docker compose -f docker/crawl4ai.yml up -d --wait 2>/dev/null || true
```

**自动压缩检查**：
```bash
python -c "
import json, sys, os
settings_path = os.path.expanduser('~/.claude/settings.json')
try:
    with open(settings_path, encoding='utf-8') as f:
        s = json.load(f)
    if s.get('autoCompactEnabled'):
        print('[Preflight] autoCompactEnabled: OK')
    else:
        print('[Preflight] autoCompactEnabled: MISSING — 多轮流水线可能因上下文溢出而中断')
        print('[Preflight] 建议: 在 ~/.claude/settings.json 中设置 \"autoCompactEnabled\": true')
        sys.exit(0)
except FileNotFoundError:
    print('[Preflight] ~/.claude/settings.json 不存在，跳过 autoCompact 检查')
except json.JSONDecodeError:
    print('[Preflight] settings.json 格式错误，跳过 autoCompact 检查')
"
```

### Step 3: 缓存检查
检查 `results/{target}/{version}/structured_contract.json` 是否存在且未过期（TTL 见 settings.json 的 `knowledge.cache_ttl_hours`，默认 168h）。如果缓存有效 → 跳到 Step 6。

**Passport Hash 验证**（material_passport.enabled=true 时）：
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```

### Step 3.5: 跨会话策略注入准备（evolution.enabled=true 时）

读取 Strategy Registry：
```bash
python scripts/strategy_injector.py {target} --text-only
```

### Step 3.6: 历史情报采集（intelligence.enabled=true 时）

**⛔ 铁律：主进程只做编排，不做执行。**

如果 `intelligence.enabled=false`，跳过整个 Step 3.6。

**读取 intelligence 配置**：
```bash
python -c "
import json
with open('settings.json', encoding='utf-8') as f:
    c = json.load(f).get('intelligence', {})
print(f'INTEL_TW={c.get(\"time_window_months\", 24)}')
print(f'INTEL_MI={c.get(\"max_issues\", 500)}')
print(f'INTEL_MC={c.get(\"max_commits\", 200)}')
print(f'INTEL_TTL={c.get(\"cache_ttl_hours\", 720)}')
"
```

#### 3.6a: 检查情报缓存

检查 `intelligence/{target}/threat_model.json` 是否存在且未过期（TTL = `intelligence.cache_ttl_hours`，默认 720h）。如果缓存有效 → 跳到 Step 3.6e。

#### 3.6b: 派发 issue-miner
```
Agent(subagent_type="testvdb:issue-miner", description="采集 {target} 历史 Issues 和 Commits",
  prompt="按照 agents/issue-miner.md 规范，为 {target} 采集历史 Issues 和已合并修复 PR。输入参数: target={target}, version={version}, intelligence_dir=intelligence/{target}/, time_window_months={INTEL_TW}, max_issues={INTEL_MI}, max_commits={INTEL_MC}。将结果写入 intelligence/{target}/issue_corpus.json 和 intelligence/{target}/commit_corpus.json。")
```
如果失败 → 记录警告，跳过后续 3.6c/3.6d。

#### 3.6c: 派发 bug-shape-extractor
```
Agent(subagent_type="testvdb:bug-shape-extractor", description="提取 {target} 历史 Bug Shapes",
  prompt="按照 agents/bug-shape-extractor.md 规范，对 intelligence/{target}/issue_corpus.json 和 intelligence/{target}/commit_corpus.json 进行分类和根因模式提取。将结果写入 intelligence/{target}/classified_issues.json、bug_shapes.json、developer_cognition.json。")
```

#### 3.6d: 派发 threat-modeler
```
Agent(subagent_type="testvdb:threat-modeler", description="构建 {target} 威胁模型",
  prompt="按照 agents/threat-modeler.md 规范，基于 bug_shapes.json、classified_issues.json、developer_cognition.json 构建威胁模型。将结果写入 intelligence/{target}/threat_model.json。")
```

#### 3.6e: 加载情报摘要
```bash
python -c "
import json
with open('intelligence/{target}/threat_model.json', encoding='utf-8') as f:
    tm = json.load(f)
print(json.dumps({
    'blindspot_count': len(tm.get('cognitive_blindspots', {}).get('blindspots', [])),
    'high_priority_areas': [a['area'] for a in tm.get('attack_surface', {}).get('high_priority_areas', [])],
    'top_blindspots': [b['blindspot_id'] for b in tm.get('cognitive_blindspots', {}).get('blindspots', [])[:3]],
}, indent=2, ensure_ascii=False))
" 2>/dev/null || echo "THREAT_MODEL_NOT_AVAILABLE"
```

### Step 4: 派 Knowledge Extractor
```
Agent(subagent_type="testvdb:knowledge-extractor", description="提取 {target} {version} 文档知识",
  prompt="按照 agents/knowledge-extractor.md 规范，为 {target} {version} 提取 API 文档知识。将结果写入 results/{target}/{version}/raw_knowledge.md")
```
**验证：** `ls -la results/{target}/{version}/raw_knowledge.md`

### Step 5: 派 Contract Formalizer
```
Agent(subagent_type="testvdb:contract-formalizer", description="形式化 {target} v{version} API 契约",
  prompt="按照 agents/contract-formalizer.md 规范，将 results/{target}/{version}/raw_knowledge.md 转换为 structured_contract.json。将结果写入 results/{target}/{version}/structured_contract.json")
```
**验证：** `ls -la results/{target}/{version}/structured_contract.json`

### Step 6: 合同门控检查
检查 `structured_contract.json` 的核心 CRUD 端点覆盖率 ≥ 90%。不通过 → 输出缺失端点 + 终止。

**Passport Hash 验证**（material_passport.enabled=true 时）：
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```

### Step 7: 初始化状态

- 生成 `session_id`: `{target}-{version_short}-{counter}`（sanitize: `[a-z0-9-]`，≤63字符）
- 创建 `results/{target}/{version}/` 目录
- 写入 `mine_state.json` 和 `.session.lock`
- **写入 `pipeline_state.json`（v3 schema）**：

```python
# pipeline_state.json v3 — 跨 Turn 状态机
{
    "version": 3,
    "session_id": "{session_id}",
    "target": "{target}",
    "version_target": "{version}",
    "current_round": 1,
    "max_rounds": {max_rounds},
    "min_defects": {min_defects},
    "phase": "ROUND_START",
    "phase_step_index": 0,
    "turn_type": "setup",
    "project_root": "{PROJECT_ROOT}",
    "session_dir": "results/{target}/{version}",
    "timestamp_dir": "",
    "phases_completed": [],
    "phase_data": {},
    "global_state": {
        "total_defects_confirmed": 0,
        "consecutive_no_defect_rounds": 0,
        "overall_coverage_pct": 0.0,
        "docker_container_running": False
    },
    "error_log": [],
    "timestamps": {
        "session_started": "{ISO_8601}",
        "last_phase_change": "{ISO_8601}"
    }
}
```

- 设置环境变量：`export TESTVDB_SESSION_ID="{session_id}"`

### Step 8: 第一轮挖掘 (Round 1)

> **第一轮直接在 Turn 1 内执行，不跨 Turn。** 从 [执行一轮完整挖掘](#执行一轮完整挖掘) 开始。
>
> 完成后：
> - 如果满足终止条件 → 直接在当前 Turn 执行 [Final Turn: Cleanup](#final-turn-cleanup)
> - 如果继续 → 更新 `pipeline_state.json`（`turn_type` 改为 `"loop"`，`current_round` += 1，`phase` = `"ROUND_START"`，`phases_completed` = []），然后调用 ScheduleWakeup：

```
ScheduleWakeup(
  delaySeconds: 60,
  reason: "TestVDB round 2 for {target} {version}",
  prompt: "/testvdb:mine {target} {version} --max-rounds {max_rounds} --min-defects {min_defects}\n\n[LOOP CONTEXT]\nSession: {session_id}\nRound: 2/{max_rounds}\nTarget: {target} {version}\nSession dir: {session_dir}\nConfirmed defects: {total_defects}\nCoverage: {coverage_pct}%\n\n系统将自动从 pipeline_state.json 恢复并继续执行。"
)
```

---

## Loop Turn: Resume Round

> 在 ScheduleWakeup 触发时执行。从磁盘重建上下文，继续下一轮挖掘。

### Phase 0: 上下文重建

1. **运行上下文重建脚本**：
```bash
python scripts/reconstruct_context.py --session-dir "{session_dir}" --format text
```

2. **从输出中提取关键信息**：
   - 当前 phase（如果轮内压缩发生在某个 phase 中间，从该 phase 继续）
   - 已完成的 phases（跳过，不要重做）
   - 本轮关键信息（reflection_context、高价值端点等）
   - 全局进度（总缺陷数、覆盖率）

3. **检查 Docker 容器状态**：
```bash
docker ps --filter "name=testvdb-{target}" --format "{{.Names}}" 2>/dev/null
```
如果容器不在运行但 `global_state.docker_container_running` 为 true → 执行 `docker restart` 或重新启动。

### Phase 1: 执行挖掘

根据 `phases_completed` 列表，从第一个未完成的 phase 开始执行 [执行一轮完整挖掘](#执行一轮完整挖掘)。

**断点恢复规则**：
- 如果 `phases_completed` 包含 `ROUND_START` 但不含 `ATTACK_GEN` → 从 ATTACK_GEN 开始
- 如果 `phases_completed` 包含 `ATTACK_GEN` 但不含 `DEBATE_S1` → 从 DEBATE_S1 开始（脚本已生成，直接收集）
- 以此类推。每个已完成的 phase 的产出文件已持久化到磁盘，直接使用。

### Phase 2: 轮次结束

- 如果满足终止条件 → 执行 [Final Turn: Cleanup](#final-turn-cleanup)
- 如果继续 → 更新 `pipeline_state.json`（`current_round` += 1，`phase` = `"ROUND_START"`，`phases_completed` = []），然后 ScheduleWakeup 触发下一轮

---

## 执行一轮完整挖掘

> 这是 Step 8 的子步骤。Turn 1 的 Round 1 和 Loop Turn 的 Round N 都执行此流程。
> 每个子步骤完成后**必须**更新 `pipeline_state.json` 的 `phase`、`phases_completed`、`phase_data`。

每轮开始前：如果是第一轮，创建 `results/{target}/{version}/{timestamp}/` 目录结构。

### 8a. ROUND_START — 注入 reflection_context + threat_model

**更新 pipeline_state**: `phase` = `"ATTACK_GEN"`, `phases_completed` 追加 `"ROUND_START"`

第一轮：无 reflection_context。后续轮次注入上轮经验。

**reflection_context 结构**：
```json
{
  "key_learnings": ["...", "..."],
  "rejection_patterns": [{"endpoint": "...", "reason": "..."}],
  "high_value_endpoints": ["..."],
  "exhausted_endpoints": ["..."],
  "last_round_summary": "..."
}
```

**跨会话策略注入**（evolution.enabled=true）：`python scripts/strategy_injector.py {target} --text-only`

**威胁模型注入**（intelligence.enabled=true 且 inject_to_attack_agents=true）：
```bash
THREAT_MODEL_ATTACK=$(python scripts/threat_model_injector.py {target} --mode attack --text-only 2>/dev/null || echo "")
```

**Judge 增强注入**（intelligence.enabled=true 且 inject_to_judge_agents=true）：
```bash
THREAT_MODEL_JUDGE_SEVERITY=$(python scripts/threat_model_injector.py {target} --mode judge --judge-type severity --text-only 2>/dev/null || echo "")
THREAT_MODEL_JUDGE_NOVELTY=$(python scripts/threat_model_injector.py {target} --mode judge --judge-type novelty --text-only 2>/dev/null || echo "")
THREAT_MODEL_JUDGE_EVIDENCE=$(python scripts/threat_model_injector.py {target} --mode judge --judge-type evidence --text-only 2>/dev/null || echo "")
```

### 8b. ATTACK_GEN — 并发出动 Attack Trio

**⛔ 绝对禁止：主进程自己生成攻击脚本。必须通过 Agent 工具派发。**

```
Agent(subagent_type="testvdb:attack-boundary", description="边界攻击 {target} v{version}",
  prompt="按照 agents/attack-boundary.md 规范，为 {target} v{version} 生成边界攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}。{THREAT_MODEL_ATTACK}")

Agent(subagent_type="testvdb:attack-state", description="状态攻击 {target} v{version}",
  prompt="按照 agents/attack-state.md 规范...（同上格式）{THREAT_MODEL_ATTACK}")

Agent(subagent_type="testvdb:attack-semantic", description="语义攻击 {target} v{version}",
  prompt="按照 agents/attack-semantic.md 规范...（同上格式）{THREAT_MODEL_ATTACK}")
```

**Fan-Out 模式**（fan_out.enabled=true）：每个 Agent 使用 3 种 focus_profile 各派发一次（共 9 个 Agent）。详见 `agents/orchestrator.md`。

**验证产出**：
```bash
ls results/{target}/{version}/{timestamp}/debate_logs/*.py 2>/dev/null | wc -l
```

**更新 pipeline_state**: `phase` = `"DEBATE_S1"`, `phases_completed` 追加 `"ATTACK_GEN"`, `phase_data.ATTACK_GEN` = `{scripts_generated: N, agents_completed: [...]}`

### 8c. DEBATE_S1 — 辩论 Stage 1

主进程自行执行自动化审查（编排协调工作）：

1. 收集脚本 → 自动去重（endpoint + constraint_id + strategy）
2. 语法验证（`python -m py_compile`）
3. 约束存在性验证
4. 脚本错误启发式检测：`python scripts/detect_risky_scripts.py "results/{target}/{version}/{timestamp}"`
5. **API 调用格式 AST 验证**：`python scripts/validate_api_format.py "results/{target}/{version}/{timestamp}"`
6. 审查结果写入 `debate_logs/stage1.json`
7. 脚本路径标准化

**更新 pipeline_state**: `phase` = `"EXECUTION"`, `phases_completed` 追加 `"DEBATE_S1"`, `phase_data.DEBATE_S1` = `{approved_count: N, rejected_count: M}`

### 8d. EXECUTION — 派 Docker Executor + 打回修改

```
Agent(subagent_type="testvdb:docker-executor", description="执行 {target} v{version} 攻击脚本",
  prompt="按照 agents/docker-executor.md 规范，在 Docker 沙箱中执行攻击脚本。target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}, session_id={session_id}。⛔ 立即执行 Step 1 命令... \n\n你是 TestVDB 流水线中被主进程派发的子 Agent。禁止使用 Agent 工具派发孙 Agent。")
```

**验证产出**：`ls results/{target}/{version}/{timestamp}/output_*.log.done 2>/dev/null | wc -l`

**打回修改机制**（8d.5）：
```bash
python scripts/scan_script_errors.py "results/{target}/{version}/{timestamp}"
```
如有错误 → 派发对应 Attack Agent 修复（最多 2 轮）。

**更新 pipeline_state**: `phase` = `"DEBATE_S2"`, `phases_completed` 追加 `"EXECUTION"`, `phase_data.EXECUTION` = `{scripts_executed: N, scripts_passed: M, scripts_error: K}`

### 8e. DEBATE_S2 — 辩论 Stage 2 + 去重

**阶段 1：先派 judge-doc**
```
Agent(subagent_type="testvdb:judge-doc", description="文档契约验证 {target}", ...)
```

**阶段 2：确认 stage2_doc.json 存在后，并发派其他 3 个 Judge**
```
Agent(subagent_type="testvdb:judge-evidence", ..., prompt="...${THREAT_MODEL_JUDGE_EVIDENCE}")
Agent(subagent_type="testvdb:judge-novelty", ..., prompt="...${THREAT_MODEL_JUDGE_NOVELTY}")
Agent(subagent_type="testvdb:judge-severity", ..., prompt="...${THREAT_MODEL_JUDGE_SEVERITY}")
```

**Fallback 机制**：如果任一 Judge 超时，主进程生成默认评估文件。

**投票逻辑和缺陷确认规则**见 `agents/orchestrator.md` Step 8e。

**缺陷去重**（8e.5）：
```bash
python scripts/dedup_defects.py "results/{target}/{version}/{timestamp}"
```

**更新 pipeline_state**: `phase` = `"REPORTING"`, `phases_completed` 追加 `"DEBATE_S2"`, `phase_data.DEBATE_S2` = `{confirmed_defects: N, rejected_defects: M}`

### 8f. REPORTING — 派 Reporter

```
Agent(subagent_type="testvdb:reporter", description="生成缺陷报告 {target}",
  prompt="按照 agents/reporter.md 规范，为以下确认的缺陷生成报告：{confirmed_defects}。session_id={session_id}, target={target}, version={version}, session_dir=results/{target}/{version}/{timestamp}")
```
**验证：** `ls results/{target}/{version}/{timestamp}/defects/defect-*.md 2>/dev/null | wc -l`

**更新 pipeline_state**: `phase` = `"DEFECT_REVIEW"`, `phases_completed` 追加 `"REPORTING"`

### 8f.5. DEFECT_REVIEW — 逐缺陷审查

```bash
python scripts/verify_defects.py "results/{target}/{version}/{timestamp}"
```
产出 `defect-review.md`。FALSE_POSITIVE → 删除。NEEDS_IMPROVEMENT → 打回 Reporter 重写（最多 1 次）。

**更新 pipeline_state**: `phase` = `"STATE_SAVE"`, `phases_completed` 追加 `"DEFECT_REVIEW"`

### 8g-8i. STATE_SAVE — 保存状态 + 分析产出 + 终止检查

主进程自行完成：

1. **保存 mine_state.json + coverage.json + experience_handoff.json + pipeline_state.json**
2. **分析本轮产出**：投票分歧模式、驳回原因分类、endpoint 覆盖率更新、生成 reflection_context
3. **策略提取**（evolution.enabled=true）：`python scripts/strategy_extractor.py "results/{target}/{version}/{timestamp}" {target}`
4. **终止条件检查**（任一满足即终止）：
   - consecutive_no_defect_rounds >= 5
   - overall_coverage_pct >= 95
   - current_round >= max_rounds（且 max_rounds > 0）
   - total_defects_confirmed >= min_defects

**更新 pipeline_state**: `phases_completed` 追加 `"STATE_SAVE"`

### 8j. 轮次间容器管理

- **继续下一轮**：`docker restart testvdb-{target}-${TESTVDB_SESSION_ID:-standalone}`
- **终止循环**：`docker compose -f docker/{target}.yml down -v`

---

## Final Turn: Cleanup

> 终止条件满足时执行（可能在 Turn 1 或任何 Loop Turn 的末尾触发）。

### Step 9: Issue 草稿 + 汇总 + 清理

#### 9a. 生成 Issue 草稿

**⛔ 绝对禁止：直接提交 Issue 到 GitHub。所有产出仅限本地文件系统。**

```bash
mkdir -p results/{target}/{version}/{timestamp}/issues
```

对每个通过审查的缺陷，生成 `issues/issue-{N}-{slug}.md`。

#### 9a.5 Issue 审核提醒

> ⚠️ **人工审核必需**：Issue 草稿由 AI 生成，需人工审核后手动提交。

#### 9b. 生成 summary.md + defect-review.md

#### 9c. 清理

```bash
# 策略提取（evolution.enabled=true）
python scripts/strategy_extractor.py "results/{target}/{version}/{timestamp}" {target}

# 容器清理
docker compose -f docker/{target}.yml down -v --remove-orphans
docker network rm testvdb-net-${TESTVDB_SESSION_ID:-standalone} 2>/dev/null || true

# 更新状态
# 更新 .session.lock status 为 completed
```

### Step 10: 标记完成

更新 `pipeline_state.json`: `phase` = `"DONE"`, `turn_type` = `"done"`

---

## Phase 更新指令

> 每个子步骤完成后，主进程必须执行以下更新。

**更新模板**（使用 Bash 工具执行 Python 脚本）：
```bash
python -c "
import json, os
ps_path = '{session_dir}/pipeline_state.json'
with open(ps_path, encoding='utf-8') as f:
    ps = json.load(f)

# 更新当前 phase
ps['phase'] = '{NEXT_PHASE}'

# 追加已完成的 phase
completed = ps.get('phases_completed', [])
if '{COMPLETED_PHASE}' not in completed:
    completed.append('{COMPLETED_PHASE}')
ps['phases_completed'] = completed

# 更新 phase_data
ps['phase_data']['{COMPLETED_PHASE}'] = {PHASE_OUTPUT}

# 更新全局状态
ps['global_state']['total_defects_confirmed'] = {total_defects}
ps['global_state']['overall_coverage_pct'] = {coverage}
ps['global_state']['docker_container_running'] = {docker_running}
ps['global_state']['consecutive_no_defect_rounds'] = {consecutive_no_defect}

# 更新时间戳
from datetime import datetime, timezone
ps['timestamps']['last_phase_change'] = datetime.now(timezone.utc).isoformat()

with open(ps_path, 'w', encoding='utf-8') as f:
    json.dump(ps, f, indent=2, ensure_ascii=False)
print(f'[pipeline_state] phase → {NEXT_PHASE}')
"
```

---

## Termination Conditions

1. **Stalemate**: 连续 5 轮无新缺陷
2. **Coverage**: 合同覆盖率 ≥ 95%
3. **Max Rounds**: `--max-rounds` 达到（且 > 0）
4. **Min Defects**: `--min-defects` 达到

## Output

```
results/{target}/{version}/{timestamp}/
├── defects/defect-1.md
├── mre/defect-1-script.py
├── issues/issue-1-batch-atomicity.md
├── defect-review.md
├── summary.md
├── debate_logs/
│   ├── stage1.json
│   ├── stage2_aggregation.json
│   ├── stage2_deduped.json
│   ├── stage2_doc.json
│   ├── stage2_evidence.json
│   ├── stage2_novelty.json
│   └── stage2_severity.json
├── structured_contract.json
├── mine_state.json
├── pipeline_state.json     ← v3 跨 Turn 状态机
├── coverage.json
├── experience_handoff.json
└── session_metadata.json

intelligence/{target}/
├── issue_corpus.json
├── commit_corpus.json
├── classified_issues.json
├── bug_shapes.json
├── developer_cognition.json
└── threat_model.json
```

## Error Recovery

重新运行相同命令可恢复中断的会话。Loop Turn 入口自动检测 `pipeline_state.json` 中的断点并恢复。

## Multi-DB Mining

```bash
# Terminal 1
/testvdb:mine milvus v2.4.0
# Terminal 2
/testvdb:mine qdrant v1.13.0
```
