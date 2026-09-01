---
description: 启动向量数据库自动化缺陷挖掘流水线
allowed-tools: Read, Write, Bash, Grep, Glob, Agent
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
| ❌ 自己判断缺陷有效性 | ✅ `Agent(subagent_type="testvdb:chain-auditor")`（ADR-0008） |
| ❌ 自己生成缺陷报告 | ✅ `Agent(subagent_type="testvdb:reporter")` |

**主进程只使用这些工具做编排工作：** `Read`(读文件), `Write`(写状态文件), `Bash`(验证产出), `Grep`(搜索), `Glob`(匹配), `Agent`(派发子Agent)。跨 turn 由 Stop hook（`pipeline_gate.py`）驱动，主进程无需调度工具。

> **⚠️ 派发工具纪律（CRITICAL — 避免重犯历史错误）**：派发 `testvdb:*` 子 Agent **只能用 `Agent(subagent_type="testvdb:xxx", ...)`**。
> - ❌ **禁止用 `TaskCreate`**：它不识别 plugin agent_type，派发记录为 `Spawning agent: unknown (inherit)`，任务永久 `pending`（幽灵条目，`TaskStop` 也删不掉），**背后无真实 agent 执行**。
> - ✅ `Agent(subagent_type=...)` 是**核心内置工具**——无需 `ToolSearch` 加载（ToolSearch 只索引 deferred 工具清单，**搜不到 ≠ 不可用**），直接调用即可。
> - ✅ 非 v2.1.166 regression 环境下 plugin subagent 真实可用（2026-06-17 实测：reporter-mre 派发成功，weaviate 3 confirmed defect 产出 `mre/*.done`）。
> - `TaskCreate`/`TaskList` 等仅用于 OMC 任务追踪，**不派 plugin agent**。探针派发能力也只用 `Agent(subagent_type=...)`，别用 TaskCreate 探针。

---

## Usage

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, `pgvector`, `meilisearch`, 或 `chroma` |
| `<version>` | Yes | — | 目标版本号 |
| `--max-rounds N` | No | `5` | 最大挖掘轮数。`0` = 无上限 |
| `--min-defects N` | No | `1` | 最低缺陷产出要求 |
| `--intel true\|false` | No | `auto` | 情报阶段控制。`true`=强制重新采集；`false`=禁用采集（C 边界：无情报→报错，有但过期→用+警告）；不传=`auto`（缓存有效→复用，否则采集） |
| `--contract true\|false` | No | `auto` | 契约阶段控制。`true`=强制重新生成；`false`=禁用生成（C 边界：无契约→报错，有但过期→用+警告）；不传=`auto`（缓存有效→复用，否则生成） |
| `--new` | No | — | 强制新建会话，忽略未完成运行的自动 RESUME（旧中断废弃时用） |

---

## 执行模型：Stop hook 驱动的跨 Turn Loop

> **📖 完整 SOP 参考**: `agents/orchestrator.md`（阶段详解、投票规则、错误处理）、`skills/pipeline/SKILL.md`（六阶段流水线规范）。本文件只保留编排调度命令，不重复 SOP 描述。

本命令采用 **Stop hook 驱动的跨 Turn 迭代模型**（`scripts/hooks/pipeline_gate.py` 接入 `.claude/settings.local.json` 的 Stop hook，参考 ralph "boulder never stops"）。每轮挖掘是一个独立的 Turn：

```
Turn 1 (FRESH_START):  Step 1-7 (setup) + Round 1 (8a→8j) + 更新 state → 主动结束 turn
                          ↓ Stop hook: phase != DONE → exit 2 → harness 强制新 turn
Turn N (RESUME):       reconstruct_context.py → Round N (8a→8j) + 更新 state → 主动结束 turn
                          ↓ （同上）
Final Turn:            终止条件满足 → phase=DONE → Stop hook 放行(exit 0) → Step 9-10
```

**机制**：主进程每轮结束更新 `pipeline_state.json`（`phase=ROUND_START`, `current_round+1`）后**主动结束 turn**。harness 的 Stop hook 调用 `pipeline_gate.py`：
- `phase != DONE` → `exit 2`（强制 Claude 新 turn 继续）
- `phase == DONE` + quality gate 通过 → `exit 0`（允许停止）

**状态持久化**：`pipeline_state.json`（v3 schema）是跨 Turn 的唯一状态源。每个 phase 完成后立即更新，确保断点恢复精确到步骤。

> ⚠️ **autoCompact 必需**：跨 turn 循环依赖 `~/.claude/settings.json` 的 `autoCompactEnabled: true`（每个 turn 间压缩 context）。preflight 会检查并警告。

---

## 执行入口

### 入口判断

每次 Turn 开始时，首先执行入口判断。**主进程：若 `/mine` 命令行含 `--new`，执行下面的脚本前先 `export TESTVDB_FORCE_NEW=1`**（force_new 强制 FRESH_START，并清残留 `.resume_target` 标记）。

```bash
python -c "
import sys, os, json
# 锁定插件根（与 Step 1 同逻辑，防 cwd 漂移）
root = os.environ.get('TESTVDB_PLUGIN_ROOT', '')
if not (root and os.path.isdir(root)):
    cur = os.getcwd()
    for _ in range(7):
        if os.path.isfile(os.path.join(cur, 'commands', 'mine.md')):
            root = cur; break
        parent = os.path.dirname(cur)
        if parent == cur: break
        cur = parent
if not root:
    print(json.dumps({'decision':'FRESH_START','reason':'no plugin root'}, ensure_ascii=False)); sys.exit(0)
os.chdir(root)
sys.path.insert(0, os.path.join(root, 'scripts'))
import _entry_dispatch as ed
# 入口判断：扫描所有未完成（loop+setup）续最新；--new 强制新建；resume 命令的 .resume_target 标记优先
result = ed.dispatch('', '', force_new=os.environ.get('TESTVDB_FORCE_NEW') == '1')
print(json.dumps(result, ensure_ascii=False))
"
```

读取 dispatch 结果 JSON：
- `decision=RESUME`（含 `session_dir`/`phase`/`target`/`version`）→ 从 `session_dir` 执行 [Loop Turn: Resume Round](#loop-turn-resume-round)
  - 若 `target`/`version` 与本次 `/mine <db> <version>` 请求不符（Turn 1 场景）→ 输出提示："最新未完成是 `{target}/{version}`，与请求 `{db}/{version}` 不符；续它还是新建？建议 `/testvdb:resume {session_id}` 或加 `--new`"
- `decision=FRESH_START` → 执行 [Turn 1: Setup + First Round](#turn-1-setup--first-round)
  - 若 `incomplete` 非空（同 target/version 有未完成）→ 输出提示："检测到未完成 `{session_id}`（`{phase}`），建议 `/testvdb:resume {session_id}`；已新建会话（如需续旧的请用 resume）"

> **`--new`**：主进程解析到 `--new` 时，入口判断前 `export TESTVDB_FORCE_NEW=1`（force_new=True 强制 FRESH_START，仍返回 `incomplete` 供知情）。
> **派发纪律**：续跑实质工作仍经 `Agent(subagent_type=...)`，禁用 `TaskCreate`（见本文件「派发工具纪律」）。

---

## Turn 1: Setup + First Round

> 仅在 FRESH_START 时执行。完成所有初始化工作后进入第一轮挖掘。

### Step 1: 解析参数
- 验证 `target` ∈ {milvus, qdrant, weaviate, pgvector, meilisearch, chroma}
- 解析 `version`, `max_rounds`, `min_defects`
- **version 规范化**：统一为 `vX.Y.Z`（用户传 `1.38.0` 或 `v1.38.0` 都归一为 `v1.38.0`），用于 session_id 和 `results/{target}/{version}/` 目录名——历史曾出现 `2.6.17`/`1.38.0`(不带v) 与 `v1.18.2`(带v) 混用，导致脚本按一种格式找不到另一种格式的产出：
```bash
version="${version#v}"   # 去掉可能的前缀 v
version="v${version}"    # 统一加回 v 前缀
```
- 确定 `PROJECT_ROOT`（**禁止用 `git rev-parse --show-toplevel`**：用户主目录 `~/` 本身是 git 仓库，在父目录启动 claude 时它会漂移到 `~/`，导致 `results/` 写错根——这是历史目录错位的根因）：
```bash
# 校验式锁定：必须是含 commands/mine.md 的 testvdb 插件根
PROJECT_ROOT="${TESTVDB_PLUGIN_ROOT:-}"
if [ -z "$PROJECT_ROOT" ] || [ ! -f "$PROJECT_ROOT/commands/mine.md" ]; then
  cur="$PWD"
  for _ in 1 2 3 4 5 6; do
    [ -f "$cur/commands/mine.md" ] && PROJECT_ROOT="$cur" && break
    cur="$(dirname "$cur")"
  done
fi
if [ -z "$PROJECT_ROOT" ] || [ ! -f "$PROJECT_ROOT/commands/mine.md" ]; then
  echo "FATAL: 找不到 testvdb 插件根（含 commands/mine.md）。请在 TestVDB 目录启动，或设 TESTVDB_PLUGIN_ROOT。"; exit 1
fi
cd "$PROJECT_ROOT"          # 锁定 cwd，后续所有相对路径 results/... 一律相对此根
export TESTVDB_PLUGIN_ROOT="$PROJECT_ROOT"   # 供 hook 脚本（_session_utils/pipeline_gate）读取
echo "[TestVDB] PROJECT_ROOT=$PROJECT_ROOT"
```

### Step 2: 前置条件检查
自行检查 Docker/Python/磁盘/网络：
```bash
python scripts/preflight.py
# 按 target 设容器版本 env（避免 compose 默认旧版本，如 chroma 0.6.3 导致 mine 1.5.9 时 server 版本不匹配 scripts API）
# image tag 格式 per-target：chroma/weaviate 不带 v（1.5.9），milvus/qdrant 带 v（v2.4.0）
case "$TARGET" in
  chroma)    export CHROMA_VERSION="${VERSION#v}" ;;
  milvus)    export MILVUS_VERSION="$VERSION" ;;
  qdrant)    export QDRANT_VERSION="$VERSION" ;;
  weaviate)  export WEAVIATE_VERSION="${VERSION#v}" ;;
esac
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
        print('[Preflight] autoCompactEnabled: MISSING — 多轮流水线会因上下文溢出而中断(这是 compact 后从头开始的根因之一)')
        print('[Preflight] 修复: 在 ~/.claude/settings.json 设置 "autoCompactEnabled": true')
        if os.environ.get('TESTVDB_ALLOW_NO_AUTOCOMPACT') == '1':
            print('[Preflight] TESTVDB_ALLOW_NO_AUTOCOMPACT=1 → 继续运行(风险自负,单轮可用)')
        else:
            print('[Preflight] 中止。设 TESTVDB_ALLOW_NO_AUTOCOMPACT=1 可强制继续。')
            sys.exit(1)
except FileNotFoundError:
    print('[Preflight] ~/.claude/settings.json 不存在，跳过 autoCompact 检查')
except json.JSONDecodeError:
    print('[Preflight] settings.json 格式错误，跳过 autoCompact 检查')
"
```

### Step 3: 契约智能消费（批次 D，D 判断）

按 `--contract` 参数决定契约阶段行为（spec 决策 4：存在→TTL→有效性→target/version 匹配）。逻辑与 `/testvdb:contract` 命令相同。

**智能判断（不传 `--contract`，默认 auto）**：
```bash
python scripts/check_cache.py contract "results/{target}/{version}" {target} {version} --ttl {knowledge.cache_ttl_hours}
```
- **USABLE**（exit 0）→ 跳过契约生成，直接 [Step 7](#step-7-初始化状态)（纯挖掘）
- **MISSING / STALE / INVALID** → 派发契约生成（Step 4 → Step 5 → Step 6）
- **MISMATCH**（target/version 不匹配）→ 报错退出

**`--contract true`**：跳过 check_cache，强制派发 Step 4 → Step 5 → Step 6（重新生成）

**`--contract false`（C 边界）**：
- **MISSING** → 报错退出（"契约缺失，--contract false 跳过生成；请先 `/testvdb:contract {target} {version}`"）
- **STALE / INVALID** → 用现有契约 + 警告（"契约可能过期/无效，--contract false 跳过刷新"），继续 Step 7
- **USABLE** → 正常使用

**Passport Hash 验证**（`material_passport.enabled=true` 且契约阶段执行了 Step 4-5 时）：
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

#### 3.6a: 情报智能消费（批次 D，D 判断）

按 `--intel` 参数决定情报阶段行为（spec 决策 4：存在→TTL→有效性）。逻辑与 `/testvdb:intel` 命令相同。

**智能判断（不传 `--intel`，默认 auto）**：
```bash
python scripts/check_cache.py intel "intelligence/{target}" {target} --ttl {INTEL_TTL}
```
- **USABLE**（exit 0）→ 跳过情报采集，直接 [3.6e](#36e-加载情报摘要)（纯挖掘）
- **MISSING / STALE / INVALID** → 派发情报采集（3.6b → 3.6c → 3.6d）
- **MISMATCH** → 报错退出

**`--intel true`**：跳过 check_cache，强制派发 3.6b → 3.6c → 3.6d（重新采集）

**`--intel false`（C 边界）**：
- **MISSING** → 报错退出（"情报缺失，--intel false 跳过采集；请先 `/testvdb:intel {target}`"）
- **STALE / INVALID** → 用现有情报 + 警告（"情报可能过期/无效，--intel false 跳过刷新"），继续 3.6e
- **USABLE** → 正常使用

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

### Step 4: 派 Knowledge Extractor（Task 4a：失败时复用+标记）

> **P3-20 glm proxy 模式（env 标志提前触发）**：preflight `check_glm_proxy` 检测 `TESTVDB_PROXY=glm` env 标志后，pipeline 启动时即知 glm proxy 环境 → knowledge-extractor 直接走 Task 4a fallback（省去 Stop hook 重试 N 次才降级）。标准代理环境仍按"agent 失败后"路径触发。glm proxy 环境用户在 SessionStart 前设 `TESTVDB_PROXY=glm`。

```
# 先检查是否有旧版本 knowledge 可复用
OLD_VERSION=$(find results/{target} -maxdepth 2 -name "raw_knowledge.json" -printf "%T@ %p\n" 2>/dev/null | sort -rn | head -1 | cut -d" " -f2- | sed 's|/raw_knowledge.json||')

if [ -n "$OLD_VERSION" ] && [ -f "$OLD_VERSION/raw_knowledge.json" ]; then
  OLD_VER=$(basename "$OLD_VERSION" | sed 's/^v//')
  echo "[Knowledge Extractor] 降级：复用旧版本 v${OLD_VER} knowledge（glm proxy 下 agent 频繁 HTTP 400）"
  # Task 4a: 复用旧版本 + 强制标记
  cp "$OLD_VERSION/raw_knowledge.json" "results/{target}/{version}/raw_knowledge.json"
  [ -f "$OLD_VERSION/deployment_meta.json" ] && cp "$OLD_VERSION/deployment_meta.json" "results/{target}/{version}/deployment_meta.json"
  # 标记 KNOWLEDGE_DEGRADED（后续写入 mine_state.json）
  export KNOWLEDGE_DEGRADED="true"
  export OLD_KNOWLEDGE_VERSION="$OLD_VER"
else
  # 无旧版本可复用，正常派发
  Agent(subagent_type="testvdb:knowledge-extractor", description="提取 {target} {version} 文档知识",
    prompt="按照 agents/knowledge-extractor.md 规范，为 {target} {version} 提取 API 文档知识。将结果写入 results/{target}/{version}/raw_knowledge.json（SDK/Docker 信息单独写 deployment_meta.json，v3.4 §B）")

  # 派发后检查是否成功（检查 raw_knowledge.json 是否被创建/更新）
  if [ ! -f "results/{target}/{version}/raw_knowledge.json" ] || [ ! -s "results/{target}/{version}/raw_knowledge.json" ]; then
    echo "[Knowledge Extractor] 失败：无法提取 knowledge，且无旧版本可复用"
    exit 1
  fi
fi
```

**验证：** `ls -la results/{target}/{version}/raw_knowledge.json results/{target}/{version}/deployment_meta.json`

**Step 4.5: OpenAPI spec 预取 + 机械覆盖率核对（根因修复 2026-08-20）**

pilot qdrant v1.18.2 实测：extractor 只抓 12 端点页却自报 `doc_coverage_pct: 100% (70/70)`——Step 6b 的"spec 不存在则跳过"是无条件逃逸舱门（qdrant spec 从未被 fetch），且 LLM 自报覆盖率无校验。修复为主进程两步确定性动作：

```bash
# (a) 派 extractor 前预取 spec（qdrant/weaviate 有规则；其他 target 退出码 3 = 无规则不阻塞）
python scripts/fetch_openapi_spec.py {target} {version}

# (b) extractor 返回后机械核对：spec paths 做分母算覆盖率，覆写 LLM 自报数字，
#     报告落盘 results/{target}/{version}/doc_coverage_report.json
python scripts/validate_doc_coverage.py {target} {version}
```

- doc_coverage_report.json 的 `missing_endpoints` 是 extractor 补爬的确定性依据（喂给 extractor 的 retry prompt 或人工审查）
- spec 缺失（fetch 退出码 1/3）→ 跳过核对并记录警告，不阻塞（保持原 Step 6b 语义，但"未 fetch"从静默变显式）

**Task 4a：如果复用旧版本，在 Step 7 写入 mine_state.json 时标记 `KNOWLEDGE_DEGRADED`**：
```python
if os.environ.get("KNOWLEDGE_DEGRADED") == "true":
    mine_state["knowledge_degraded"] = {
        "reused_from": os.environ.get("OLD_KNOWLEDGE_VERSION"),
        "reason": "knowledge-extractor agent failed (glm proxy HTTP 400)",
        "error_log": "Agent 派发失败或超时，复用旧版本 knowledge 契约可能过时"
    }
```

### Step 5: 派 Contract Formalizer
```
Agent(subagent_type="testvdb:contract-formalizer", description="形式化 {target} v{version} API 契约",
  prompt="按照 agents/contract-formalizer.md 规范，将 results/{target}/{version}/raw_knowledge.json 转换为 structured_contract.json（每条约束标 level 分级，规则 2.7；v3.4）。将结果写入 results/{target}/{version}/structured_contract.json")
```
**验证：** `ls -la results/{target}/{version}/structured_contract.json`

**Step 5.5: spec 参数机械回填（机制化 2026-08-21，rerun pilot 手工 patch 的替代）**

rerun pilot 实测：formalizer 对 raw_knowledge.md 的 "Spec-derived Endpoints" 骨架条目
（Step 4.5 机械补全的端点）消费不可靠——65 端点 0 parameters。与其让 LLM 学会读
骨架，formalizer 之后主进程确定性回填（同 Step 4.5 哲学——0 token，不编造，字段标
`source: openapi (mechanical backfill)`）：

```bash
py -3 scripts/enrich_contract_from_spec.py results/{target}/{version} --fill-missing-fields
# 退出 0 = 回填完成或 spec 不可用（先跑过 Step 4.5 fetch 才有 spec）；含 passport 重签
```

### Step 6: 合同门控检查
检查 `structured_contract.json` 的核心 CRUD 端点覆盖率 ≥ 90%。不通过 → 输出缺失端点 + 终止。

**Passport Hash 验证**（material_passport.enabled=true 时）：
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```

**Step 6.2: 契约文档资产预检（规则 P1.0，2026-09-02 — evidence-builder A 层前两层消费）**：
```bash
python scripts/preflight_contract_docs.py "results/{target}/{version}/structured_contract.json"
```
产 sidecar `doc_preflight.json`（契约文件本体零改动；0-LLM，URL 去重后 8 并发，秒级）。
- exit 0 = PASS 或 skipped（`TESTVDB_OFFLINE=1` 跳过，builder 走回退 WebFetch）
- exit 1 = 存在 dead/mismatched 文档 → **记录 pipeline_state 不中断**（文档死亡是
  环境事实，evidence-builder 如实取证；不重派发、不硬 block）

**Step 6.5: 策略预绑定（v3.4 D2，确定性主进程步骤——0 LLM）**：
```bash
python scripts/bind_strategies.py "results/{target}/{version}/structured_contract.json"
```
- exit 1（level lint 失败：契约缺 `level` 字段）→ 重派 contract-formalizer（规则 2.7 未执行）
- 正常 → 契约各约束带 `bound_strategies` + 顶层 `_strategy_binding` 汇总；attack agents 按绑定清单直接生成（不再自行匹配策略）

### Step 7: 初始化状态

- 生成 `session_id`: `{target}-{version_short}-{counter}`（sanitize: `[a-z0-9-]`，≤63字符）
- **生成 TIMESTAMP（单一权威入口）**：后续所有 `{timestamp}` 一律引用此变量，禁止 ad-hoc 生成——历史曾出现 ISO `2026-06-06T14-26-53Z` / 紧凑T `20260611T013818` / 紧凑- `20260614-173709` 三种格式混用，根因即无统一入口。格式定为 `YYYY-MM-DDTHH-MM-SSZ`（ISO 风格、冒号→破折号、NTFS 安全、字典序可排序）：
```bash
TIMESTAMP="$(python -c "from datetime import datetime,timezone;print(datetime.now(timezone.utc).strftime('%Y-%m-%dT%H-%M-%SZ'))")"
SESSION_DIR="results/${target}/${version}/${TIMESTAMP}"
mkdir -p "$SESSION_DIR"
export TESTVDB_TIMESTAMP="$TIMESTAMP" TESTVDB_SESSION_DIR="$SESSION_DIR"
echo "[TestVDB] TIMESTAMP=$TIMESTAMP SESSION_DIR=$SESSION_DIR"
```
- 创建 session 目录 `$SESSION_DIR`（即 `results/{target}/{version}/{TIMESTAMP}/`）
- 写入 `mine_state.json` 和 `.session.lock`
- **通过 CLI 初始化 `pipeline_state.json`（v3 schema, ADR-0004）**：

```bash
python scripts/pipeline_state.py init \
    --target "{target}" \
    --version "{version}" \
    --session-dir "$SESSION_DIR" \
    --max-rounds {max_rounds} \
    --min-defects {min_defects} \
    --project-root "{PROJECT_ROOT}"
```

> 等价于以下 v3 schema JSON（供参考，无需手写）：

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
    "session_dir": "results/{target}/{version}/{TIMESTAMP}",
    "timestamp_dir": "{TIMESTAMP}",
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
> - 如果继续 → 更新 `pipeline_state.json`：先 `advance --phase ROUND_START`（含 STATE_SAVE → ROUND_START transition），再 `mutate --current-round {N+1}`，然后**主动结束当前 turn**（不调用任何调度工具）。

Stop hook（`scripts/hooks/pipeline_gate.py`）检测 `phase != DONE` → `exit 2` → harness 自动开启新 turn。新 turn 的入口判断识别 `turn_type=loop` 进入 [Loop Turn: Resume Round](#loop-turn-resume-round)。

> **不使用 ScheduleWakeup**——它在非 `/loop` runtime 环境（如 glm proxy）gate 关闭不可用。Stop hook `exit 2` 是可靠的跨 turn 驱动力（参考 ralph "boulder never stops"）。

---

## Loop Turn: Resume Round

> 在 Stop hook `exit 2` 强制的新 turn 执行（主进程结束 turn 后，pipeline_gate 检测 `phase != DONE` → `exit 2` → harness 重启 turn）。从磁盘重建上下文，继续下一轮挖掘。

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
- 如果继续 → 更新 `pipeline_state.json`（`current_round` += 1，`phase` = `"ROUND_START"`，`phases_completed` = []），然后**结束 turn**。Stop hook 触发下一轮（同 Step 8 末尾机制）。

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

（ADR-0008：Judge 增强注入已随 Judge Quartet 删除；threat_model 仅 --mode attack 注入仍在用。）

### 8b. ATTACK_GEN — 契约分块 + 并发出动 Attack Trio + Explorer

**契约分块（ADR-0008，每轮一块）**：
```bash
python scripts/chunk_contract.py results/{target}/{version}/structured_contract.json --session-dir ${PROJECT_ROOT}/results/{target}/{version}/{timestamp}
```
第 R 轮派发 `chunks[R-1]`。派发 prompt 附 `本轮块={chunk_id}` + 块内 unit_ref 清单。

**⛔ 绝对禁止：主进程自己生成攻击脚本。必须通过 Agent 工具派发。**

```
Agent(subagent_type="testvdb:attack-boundary", description="边界攻击 {target} v{version}",
  prompt="按照 agents/attack-boundary.md 规范，为 {target} v{version} 生成边界攻击脚本。contract=${PROJECT_ROOT}/results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}, reflection_context={reflection_context}, 本轮块={chunk_id}（块内 unit_ref 清单见 chunks.json，只攻该块内单元）。{THREAT_MODEL_ATTACK}")

Agent(subagent_type="testvdb:attack-state", description="状态攻击 {target} v{version}",
  prompt="按照 agents/attack-state.md 规范...（同上格式）{THREAT_MODEL_ATTACK}")

Agent(subagent_type="testvdb:attack-semantic", description="语义攻击 {target} v{version}",
  prompt="按照 agents/attack-semantic.md 规范...（同上格式）{THREAT_MODEL_ATTACK}")
```

> **attack-vein 已按 ADR-0009 移除**（v2.5 → 已废弃）：其 condition-driven 纵深挖掘职能由两阶段调度的探索模式（ADR-0009 §2-§4）承接，纵深探索产出统一走沙箱小循环（不再有 agent 自跑 curl 的例外路径）。

**验证产出**：
```bash
# 老三套的脚本产出
ls results/{target}/{version}/{timestamp}/debate_logs/*.py 2>/dev/null | wc -l
```

**更新 pipeline_state**: `phase` = `"DEBATE_S1"`, `phases_completed` 追加 `"ATTACK_GEN"`, `phase_data.ATTACK_GEN` = `{scripts_generated: N, agents_completed: [...]}`

### 8c. DEBATE_S1 — 辩论 Stage 1（静态审查 + D3b 预验证）

> **gate 版本**: v3（R1-R3，7 检查，`TESTVDB_PREVERIFY=NONE` 可回放）/ **v4（R4+，+D3b 预验证 4 类：oracle_missing / oracle_degenerate / transport_probe_wrong / oracle_shape_conflict / request_required_missing，preverify_version=D3b-R4.0）**

主进程自行执行自动化审查（编排协调工作）：

1. 收集脚本（ADR-0008：脚本去重已删——重复攻击交给执行与 chain-auditor 自然淘汰，缺陷级去重在 8e.5）
2. 语法验证（`python -m py_compile`）
3. 约束存在性验证
4. 脚本错误启发式检测：`python scripts/detect_risky_scripts.py "results/{target}/{version}/{timestamp}"`
5. **API 调用格式 AST 验证**：`python scripts/validate_api_format.py "results/{target}/{version}/{timestamp}"`
6. **Target 中立验证**：`python scripts/validate_target_neutrality.py "results/{target}/{version}/{timestamp}"`
   含与当前 target 不符的 DB 签名（如 target=weaviate 但脚本命中 :6333）的脚本 → 打回 Attack Agent 修改（同 8d.5 打回机制）。
7. **D3b 预验证 A/B 类**（v3.4，2026-08-26；oracle 行存在性 + transport 存活探针三分法）：
   ```bash
   python scripts/_classify_script_errors.py "results/{target}/{version}/{timestamp}"
   ```
8. **D3b 预验证 C/D 类**（spec 对照：断言×响应形状相容矩阵 + 请求体×嵌套必填树；含 meta.oracle 单写者派生与 WARN 边车）：
   ```bash
   python scripts/_preverify_spec_shape.py "results/{target}/{version}/{timestamp}" --db {target} --version {version}
   ```
9. **severity 感知的 retry 派发**（WARN 不消耗预算；REJECT 按 8d.5 工单模式打回）：
   ```bash
   python scripts/_apply_script_retry.py "results/{target}/{version}/{timestamp}"
   ```
10. 审查结果写入 `debate_logs/stage1.json`
11. 脚本路径标准化

**更新 pipeline_state**: `phase` = `"EXECUTION"`, `phases_completed` 追加 `"DEBATE_S1"`, `phase_data.DEBATE_S1` = `{approved_count: N, rejected_count: M}`

### 8d. EXECUTION — 派 Docker Executor + 打回修改

```
Agent(subagent_type="testvdb:docker-executor", description="执行 {target} v{version} 攻击脚本",
  prompt="按照 agents/docker-executor.md 规范，在 Docker 沙箱中执行攻击脚本。target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}, session_id={session_id}。⛔ 立即执行 Step 1 命令... \n\n你是 TestVDB 流水线中被主进程派发的子 Agent。禁止使用 Agent 工具派发孙 Agent。")
```

**验证产出**：`ls results/{target}/{version}/{timestamp}/output_*.log.done 2>/dev/null | wc -l`

**打回修改机制**（8d.5，ADR-0008 已接线 v2.5 确定性 retry 子循环）：
```bash
# 执行前静态检查（AST 分类 5 类错误）+ counter/feedback/超限降级，全确定性：
python scripts/_classify_script_errors.py "results/{target}/{version}/{timestamp}"
python scripts/_apply_script_retry.py "results/{target}/{version}/{timestamp}"
```
如有 `regen` 输出 → 按 source 分组派对应 Attack Agent（读 `${script_id}.retry_feedback.json`
修后覆盖原文件，详见 `agents/orchestrator.md` §4.6）。执行后日志扫描兜底（运行期错误）：
```bash
python scripts/scan_script_errors.py "results/{target}/{version}/{timestamp}"
```

**更新 pipeline_state**: `phase` = `"EVIDENCE_BUILD"`, `phases_completed` 追加 `"EXECUTION"`, `phase_data.EXECUTION` = `{scripts_executed: N, scripts_passed: M, scripts_error: K}`

### 8e. EVIDENCE_BUILD — 候选提取 + L1 机械闸门 + evidence-builder 并发 fan-out（ADR-0008）

**Step 1 — 机械提取候选清单**（builder fan-out 的派发清单，确定性 0 LLM）：
```bash
python scripts/extract_candidates.py "results/{target}/{version}/{timestamp}"
# 产出 candidates.jsonl（VERDICT: DEFECT_FOUND 的 log → 候选；SCRIPT_ERROR 排除）
```

**Step 2 — L1 机械闸门前移**（0 token 杀 ~90% 历史 FP 模式，REFUTED 不进 fan-out）：
```bash
python scripts/verify_live_l1.py "results/{target}/{version}/{timestamp}" --target {target}
```
REFUTED 候选从 candidates.jsonl 移除（记入 verify_live_l1.json，供实验统计）。

**Step 2.5 — 证据链引文预检（auditor 前置，R2 教训机制化 2026-08-26）**：
```bash
python scripts/verify_chain_quotes.py "results/{target}/{version}/{timestamp}"
```
builder 全部 `.done` 后、auditor 派发前执行。mismatch 的链按 R2 工单模式打回对应 builder 重引原文子串（1 分钟修复 << NME 补证轮 3 builder+1 auditor 成本；R2 实测 3 条括注引文落 NME）。

**Step 3 — evidence-builder 按候选并发派发**（1 builder/候选，任务重减压提效）：
```
对 candidates.jsonl 每行，并发派发（受派发槽位约束，超出排队）：
Agent(subagent_type="testvdb:evidence-builder", description="证据链构建 {defect_id}",
  prompt="按照 agents/evidence-builder.md 规范，为候选 {defect_id} 构建证据链。target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}。你的 defect_id={defect_id}。")
```
- 产出 `evidence_chain/{defect_id}.json` + `.done`（按候选命名，并发无写冲突）
- 超时（60s/候选）或缺产出的候选：不重试进 fan-out，留给 auditor 记 NEEDS_MORE_EVIDENCE
- **NEEDS_MORE_EVIDENCE 补证轮 + 打回工单**（auditor 判定后，2026-08-18 扩展）：
  仅对 auditor 标记的 defect_id 重派 builder。重派 prompt **必须携带 auditor 的
  rework_order 工单**（type/claim/chain_covered/drift_point/targeted_instruction），
  builder 按工单针对性重做（不是重跑全部）。
  **打回上限 3 轮**（rework_state 文件按 defect_id 计数，模式同缺陷级 retry counter）：
  第 3 轮后仍 mismatch → auditor 保守判 NOT_DEFECT。
  工单三种 type：PHENOMENON_MISMATCH（取证漂移→重读 log 全文围绕 claim 重建）/
  EVIDENCE_GAP（链不全→针对性补节）/ SUSPECTED_HALLUCINATION（引文对不上→重核原文行）

**更新 pipeline_state**: `phase` = `"CHAIN_AUDIT"`, `phases_completed` 追加 `"EVIDENCE_BUILD"`, `phase_data.EVIDENCE_BUILD` = `{candidates: N, l1_refuted: M, builders_done: K}`

### 8e.7. CHAIN_AUDIT — chain-auditor 单实例收口

**全部 builder `.done` 收口后**派发（跨候选一致性检查需要完整链集合）：
```
Agent(subagent_type="testvdb:chain-auditor", description="证据链审计 {target}",
  prompt="按照 agents/chain-auditor.md 规范，审计 evidence_chain/ 下全部证据链并产出终判（四视角 A/B/C/D，视角 D 消费 intelligence/{target}/developer_cognition.json）。target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}。")
```
- 产出 `debate_logs/chain_verdicts.json`（DEFECT / NOT_DEFECT / NEEDS_MORE_EVIDENCE +
  fp_evidence_source + root_cause 分布）+ `.done`
- 验证：`test -f "results/{target}/{version}/{timestamp}/debate_logs/chain_verdicts.json.done" && echo READY || echo PENDING`
- NEEDS_MORE_EVIDENCE > 0 → 回 8e Step 3 补证轮（最多 1 次），重派 auditor 出最终 verdict
- **⛔ 主进程绝不做判定。auditor 超时 → 全部候选保守按 NEEDS_MORE_EVIDENCE 记录并重派一轮，
  第二轮仍超时 → NOT_DEFECT（保守）+ error_log 记录。**

**跨轮去重**（8e.5，保留；输入改为 verdict 列表）：
```bash
python scripts/dedup_defects.py "results/{target}/{version}/{timestamp}"
```

**更新 pipeline_state**: `phase` = `"REPORTING"`, `phases_completed` 追加 `"CHAIN_AUDIT"`, `phase_data.CHAIN_AUDIT` = `{verdict_defect: N, not_defect: M, needs_more_evidence: K}`

### 8f. REPORTING — 派 Reporter

**confirm_per_round 开关（ADR-0009 §6）**：进入本步前执行 `python scripts/get_setting.py mining.confirm_per_round`。
- `true`（默认）→ 照常执行 8f/8f.5。
- `false`（实验特化）→ **跳过 8f 与 8f.5**：candidates.jsonl 继续累积（8e.5 跨轮去重照常），`pipeline_state` 标记 `phase=MINING_DEFER_CONFIRM`；会话终止后对全部累积候选执行统一判定（evidence-builder + chain-auditor 批量 + novelty 终判 + Reporter 一次性收口，同 ADR-0008 规范）。

```
Agent(subagent_type="testvdb:reporter", description="生成缺陷报告 {target}",
  prompt="按照 agents/reporter.md 规范，为以下 Debate-Confirmed 缺陷生成报告：{debate_confirmed}。session_id={session_id}, target={target}, version={version}, session_dir=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}")
```
**验证：** `ls results/{target}/{version}/{timestamp}/defects/defect-*.md 2>/dev/null | wc -l`

**⛔ summary.md 主进程实测门（2026-08-21 机制级修复——agent 自验证多次复现虚报）**：

reporter 返回后，主进程**必须实测**（不采信回复文本）：
```bash
test -s "results/{target}/{version}/{timestamp}/summary.md" && echo SUMMARY-OK || echo SUMMARY-MISSING
```
- `SUMMARY-MISSING` → 重派 reporter 一次（派发词附"上轮 summary.md 未落盘，本次仅补写 summary.md + 结尾 Bash test -s 实测"）；仍缺失 → 主进程代写并在 summary 顶部标注 `⚠️ reporter-generated=false (main-process fallback)`，error_log 记录
- 该门在 9b（生成 summary.md）之前执行也适用：Final Turn 路径同样实测

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
   - total_defects_confirmed >= min_defects（且 min_defects > 0；`--min-defects 0` = 无下限，不触发）

**更新 pipeline_state**: `phases_completed` 追加 `"STATE_SAVE"`

### 8j. 轮次间容器管理

- **继续下一轮**：`docker restart testvdb-{target}-${TESTVDB_SESSION_ID:-standalone}`
- **终止循环**：`docker compose -f docker/{target}.yml down -v`

---

## Final Turn: Cleanup

> 终止条件满足时执行（可能在 Turn 1 或任何 Loop Turn 的末尾触发）。

### Step 9: Issue 草稿 + 汇总 + 清理

#### 9a. 运行 Novelty Gate（ADR-0008：后置终判 + NON_NOVEL 归档不删）

**在生成 Issue 草稿前，必须对全部 chain-verdict DEFECT candidate 运行 Novelty Gate（输入源：chain_verdicts.json，ADR-0008 已适配）。Gate 产出 Gate-Endorsed（endorsement=true）才是真正可提交的缺陷（ADR-0001）。**

```bash
python scripts/novelty_gate.py --session-dir results/{target}/{version}/{timestamp}
```

**Exit code 处理**：
- `0`（有 NOVEL 背书）→ 继续 9b，仅对 `endorsement=true` 的缺陷生成 Issue 草稿
- `1`（全拒绝）→ 跳过 Issue 生成，直接生成汇总
- `2`（有 UNVERIFIED）→ 跳过 Issue 生成，记录警告

**读取 Gate 结果**：
```bash
cat results/{target}/{version}/{timestamp}/debate_logs/novelty_gate.json | python -c "
import json, sys
data = json.load(sys.stdin)
endorsed = [d for d, r in data.items() if r.get('endorsement')]
print(json.dumps({'endorsed_defects': endorsed}, ensure_ascii=False))
"
```

**NON_NOVEL 归档（ADR-0008，替代旧"拒绝即丢弃"）**：Gate 终判后一次性执行——被归档
candidate 的 defect-N.md 移入 `archived/`，并在 `archived/manifest.json` 记录
`related_issue_numbers`（gate_evidence_url 提取）。这是 RQ1"发现已被报告 bug"列的数据源。
轮内不做归档（REPORTING→Step 9 窗口期 defect-N.md 保留原位）。

#### 9b. 生成 Issue 草稿（仅背书的 NOVEL，candidate 级）

**⛔ 绝对禁止：直接提交 Issue 到 GitHub。所有产出仅限本地文件系统。**

```bash
mkdir -p results/{target}/{version}/{timestamp}/issues
```

**粒度映射规则（ADR-0002）**：Novelty Gate 按 candidate/script 级判定（一个 defect 聚合可含多个 candidate，如 defect-2 含 7 个参数）。映射如下：
- **Issue 草稿**：按 **candidate 级**生成，仅 `endorsement=true` 的 candidate → `issues/issue-{param-slug}-novel.md`。reject 的 candidate **不**生成 issue 草稿，**移入 `archived/`（ADR-0008，见 9a 归档段）**。
- **归档清单**：reject 的 candidate 记入 `archived/manifest.json`（candidate + param + grade + evidence_url + related_issue_numbers）。`judge_discrepancy=true` 的 candidate 须标注（门控推翻了初判的 NOVEL——这是门控核心价值）。
- **Defect 聚合报告**（`defects/defect-N.md`）仍生成，但头部必须标注门控汇总：含 N candidate，M endorse / (N-M) reject，避免聚合叙事掩盖 candidate 级门控决策。

#### 9b.5 Issue 审核提醒

> ⚠️ **人工审核必需**：Issue 草稿由 AI 生成，需人工审核后手动提交。

#### 9a.6 生成 MRE 脚本（派 reporter-mre）

主进程为通过审查的 Debate-Confirmed 缺陷派发 reporter-mre，生成自包含 MRE 脚本（每个缺陷一个不依赖 TestVDB 代码的独立 Python 脚本）。reporter 专注 defect-N.md 报告，MRE 脚本由 reporter-mre 独立生成（v2.1.1 Reporter 拆分）。

```
Agent(subagent_type="testvdb:reporter-mre", description="生成 MRE 脚本 {target}",
  prompt="按照 agents/reporter-mre.md 规范，为以下 Debate-Confirmed 缺陷生成自包含 MRE 脚本：{debate_confirmed}。session_id={session_id}, target={target}, version={version}, session_dir=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}")
```

**验证：** `ls results/{target}/{version}/{timestamp}/mre/defect-*-script.py.done 2>/dev/null | wc -l`（应 ≥1；reporter-mre 完成每个脚本后 `touch .done` 并通过 `py_compile`）

> reporter 的 Pre-Submit Gate 复现验证用 curl 回退（reporter.md 已支持）——MRE 脚本由本步骤的 reporter-mre 独立生成，供外部一键复现。

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

> 每个子步骤完成后，主进程必须执行以下更新。使用 `pipeline_state.py` CLI (ADR-0004)。

### advance — 阶段推进

```bash
python scripts/pipeline_state.py advance \
    --session-dir "{session_dir}" \
    --phase "{NEXT_PHASE}" \
    --phase-data '{"{COMPLETED_PHASE}": {PHASE_OUTPUT}}'
```

### mutate — 更新全局状态计数器

```bash
python scripts/pipeline_state.py mutate \
    --session-dir "{session_dir}" \
    --total-defects {total_defects} \
    --coverage {coverage} \
    --docker-running {docker_running} \
    --consecutive-no-defect {consecutive_no_defect}
```

### status — 查询当前状态

```bash
python scripts/pipeline_state.py status --session-dir "{session_dir}"
```

> 等价于原始的手动 JSON 编辑，但提供 seam 校验（无效 phase 转换 → InvalidTransition 报错）。

---

## Termination Conditions

1. **Stalemate**: 连续 5 轮无新缺陷
2. **Coverage**: 合同覆盖率 ≥ 95%
3. **Max Rounds**: `--max-rounds` 达到（且 > 0）
4. **Min Defects**: `--min-defects` 达到（`--min-defects 0` = 无下限，不触发）

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
