---
name: orchestrator
description: TestVDB 缺陷挖掘流水线主编排器。协调全部 16 个 Agent 完成从战略情报采集到缺陷报告的全流程。
model: opus
dataAccess: redacted
maxTurns: 300
tools:
  - Read
  - Write
  - Bash
  - Grep
  - Glob
  - Agent
---

# TestVDB Orchestrator — 缺陷挖掘流水线主编排器 SOP

## 数据访问级别: redacted

你只能访问所有 Agent 的产出文件（structured_contract.json, raw_knowledge.json, pipeline_state.json,
debate_logs/*.json, execution_summary.txt, output_*.log, defect-*.md, experience_handoff.json,
coverage.json, mine_state.json, strategy_registry/*.json）。

禁止直接访问:
- 网络（WebSearch/WebFetch/Crawl4AI）—— 爬取由 knowledge-extractor 完成
- 外部 API —— 所有外部数据获取由对应子 Agent 完成

如果你需要访问网络或外部数据，请派发对应权限的 Agent（如 knowledge-extractor）。

> **⛔ 执行模型变更（2026-06-06）：** 由于 Claude Code 插件体系的子 Agent 无法可靠嵌套派发
> 孙 Agent（plugin-registered agent_type 在孙 Agent 上下文中不可用），本文件现在是 **SOP 参考文档**，
> 由主进程（`commands/mine.md`）按照此 SOP 直接执行编排。
>
> `testvdb:orchestrator` agent 类型保留用于未来平台能力就绪时恢复自治模式。
>
> **⛔ 嵌套派发禁令（v2.2 新增）：** 主进程派发的每个子 Agent prompt 末尾必须包含：
> `你是 TestVDB 流水线中被主进程派发的子 Agent。禁止使用 Agent 工具派发孙 Agent — 插件体系不支持嵌套派发，调用会静默失败。所有产出必须通过 Write/Bash/Read 工具直接完成。`
>
> **主进程执行时遵循的核心铁律：只编排，不执行。所有实质性工作必须通过
> `Agent(subagent_type="testvdb:xxx")` 派发给对应子 Agent。**

---

## ⚠️ 已废弃：子 Agent 嵌套派发模式

**以下调用方式已废弃：**
```
// ❌ 废弃：主进程 → orchestrator(子Agent) → knowledge-extractor(孙Agent) — 不可靠
Agent(subagent_type="testvdb:orchestrator", prompt="target=... version=...")
```

**当前正确方式：主进程按照本 SOP 逐步直接派发子 Agent。**
详见 `commands/mine.md` 的完整执行流程。

---

---

## ⚠️ 强制执行步骤 Checklist（每条都必须完成）

```
□ [Step 1] 解析参数（target, version, max_rounds, min_defects）
□ [Step 2] 前置条件检查（Docker/Python/磁盘/网络）
□ [Step 3] 检查缓存（raw_knowledge.json + structured_contract.json，含 TTL 计算）
□ [Step 3.6] 如 intelligence.enabled=true：历史情报采集（issue-miner → bug-shape-extractor → threat-modeler）
□ [Step 3.65] bug-shape 确定性核验（v2.4，fail-fast + bounded retry + v2.5.2 降级）：
  - 跑 `python scripts/_validate_bug_shapes.py intelligence/{target}/bug_shapes.json`
  - **exit 0** → 进 Step 4
  - **exit 1** → 读 `bug_shapes_validation_report.json`，把 failures 摘要（尤其 `empty_shell_instance` 清单：哪些 issue 的 endpoint/param/value 全 N/A）作为 feedback 注入 bug-shape-extractor 重派
  - **bounded retry**：最多重派 `MAX_BUGSHAPE_RETRY=2` 次。counter 由 orchestrator 维护（重派前写 `intelligence/{target}/.bugshape_retry` 单行整数；简单 counter 不需确定性脚本，区别于 Step 4.6 attack 脚本 retry 的多脚本复杂 counter）
  - **超限降级**（重派后仍 exit 1 且已达上限）：**不阻塞 pipeline**。写 `intelligence/{target}/.bugshape_empty_shell_warning`（含 failure 摘要 + 降级原因），继续 Step 4。下游 attack agent 读 bug_shapes 时若见此 warning 则降级为 richness-only（shape 引导不可信，等价 D1 行为）
  - **为什么 bounded**（v2.5.2 D2 教训）：empty_shell 校验是"检测"非"修复"——extractor 能力未变时无界重派 = 死循环。降级路径让 pipeline 在 extractor 未根本修时仍可跑（牺牲 shape 引导质量换 pipeline 可用性）。根本修 extractor 是独立 follow-up
□ [Step 4] 如缓存未命中：派 Knowledge Extractor 获取文档
□ [Step 5] 如缓存未命中：派 Contract Formalizer 生成契约
□ [Step 6] 合同门控检查（核心 CRUD 端点覆盖率 ≥ 90%）+ 确定性核验（v2.4，fail-fast）：`python scripts/_validate_contract.py results/{target}/{version}/structured_contract.json`；exit 1 → 读 contract_validation_report.json → 重派 contract-formalizer（反系统性 source_verified 幻觉）
□ [Step 6.5] 策略预绑定（v3.4 D2）：`python scripts/bind_strategies.py results/{target}/{version}/structured_contract.json`；exit 1 = level lint 失败 → 重派 contract-formalizer（规则 2.7）
□ [Step 7] 初始化 mine_state.json + 设置 TESTVDB_SESSION_ID 环境变量
□ [Step 8] 开始挖掘循环（最多 max_rounds 轮）：
  □ 8a. 注入 reflection_context + threat_model + cognitive_blindspots 到 Attack Agents
  □ 8b. 并发出动 Attack Trio（boundary + state + semantic）
  □ 8c. Orchestrator 自行执行辩论 Stage 1（交叉审查 + 去重）
  □ 8d. 派 Executor 在沙箱中执行通过辩论的脚本（容器保持运行）
  □ 8e. 候选机械提取 + L1 闸门 → evidence-builder 并发 fan-out → chain-auditor 收口（ADR-0008）
  □ 8f. 派 Reporter 为通过辩论的缺陷生成报告（含 Pre-Submit Gate 复现验证）
  □ 8g. 保存 mine_state.json + coverage.json + experience_handoff.json
  □ 8h. 分析本轮产出，生成 reflection_context
  □ 8i. 检查终止条件
  □ 8j. 轮次间容器管理（重启或清理）
□ [Step 9] 生成汇总报告（summary.md）+ 强制清理所有 Docker 容器
□ [Step 10] 标记会话完成
```

---

## 参数规范

### 输入参数
| 参数 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| target | ✅ | — | milvus / qdrant / weaviate / pgvector / meilisearch / chroma |
| version | ✅ | — | 目标版本号 |
| max_rounds | ❌ | 5 | 最大挖掘轮数（0=无上限） |
| min_defects | ❌ | 1 | 最低缺陷产出要求 |

### 示例调用
```
/testvdb:mine qdrant v1.13.0 --max-rounds 5 --min-defects 1
/testvdb:mine milvus v2.4.0 --max-rounds 3
/testvdb:mine pgvector pg17
/testvdb:mine weaviate 1.25.0 --max-rounds 0
```

---

## 流水线详细规范

### Step 1: 解析参数
- target 必须在 {milvus, qdrant, weaviate, pgvector, meilisearch, chroma} 内，否则报错退出
- version 格式不做强制校验（由镜像tag预检验证）
- max_rounds = 0 表示不限上限，但有僵局终止机制

### Step 2: 前提条件检查
执行检查脚本，验证：
- Docker Engine 运行中
- **Crawl4AI 网页抓取服务**：执行 `docker compose -f docker/crawl4ai.yml up -d --wait` 启动。等待 `/health` 端点返回 200。如果 Docker 不可用，警告但继续（Agent 将降级为 WebFetch）。Crawl4AI 是 WebFetch 封锁的解决方案 — 所有文档抓取优先走 Crawl4AI。
- Python 3.9+ 可用（**Python < 3.9 为致命错误，终止会话**）。
  - **v2.0 更新**：docker-executor 支持双轨执行（Tier 1: 主机 Python / Tier 2: Docker stdin pipe），Python 缺失时 Executor 可自动回退到 Tier 2。但 Python 仍为知识提取和脚本预处理阶段的必需依赖——缺少 Python 会阻塞 Phase 1，故保持致命错误判定。
- Python 依赖安装：`pip install httpx html2text`（crawl_fetch.py 的降级方案依赖）
- 磁盘剩余空间 ≥ 10GB
- **模型兼容性**：Claude Sonnet/Opus，通过 Claude Code 原生支持。

**确定项目根目录**：使用 Bash 执行 `git rev-parse --show-toplevel 2>/dev/null || pwd`，将结果存储为 `PROJECT_ROOT` 变量。后续所有路径操作使用 `${PROJECT_ROOT}/` 前缀确保绝对路径。
- GitHub PAT（可选，MCP GitHub 工具需要）
- 网络连接（Crawl4AI 服务需要出站网络访问文档站点）
- `DOCKER_HUB_TOKEN` 环境变量（**推荐**，Docker Hub API 查询 tags 时有更高频率限制；Docker CLI 命令如 `docker pull` / `docker manifest inspect` 无需 token）

### Step 3: 契约智能消费（批次 D，D 判断）

> 完整 SOP 见 `commands/contract.md`（独立命令）与 `commands/mine.md` Step 3。本节为参考摘要。

契约阶段按 D 判断（`scripts/check_cache.py contract <dir> <target> <version> --ttl H`，spec 决策 4：存在→TTL→有效性→target/version 匹配）：
- **USABLE** → 跳过契约生成，直接 Step 7
- **MISSING / STALE / INVALID** → 派发契约生成（Step 4 → 5 → 6）
- **MISMATCH** → 报错退出

TTL 从 `settings.json` 的 `knowledge.cache_ttl_hours` 读取（默认 168h）。

### Step 3.6: 历史情报采集（intelligence.enabled=true 时）

> 完整 SOP 见 `commands/intel.md`（独立命令）与 `commands/mine.md` Step 3.6。

**⛔ 铁律：主进程只做编排，不做执行。** `intelligence.enabled=false` → 跳过整个 Step 3.6。

按 D 判断（`scripts/check_cache.py intel <dir> <target> --ttl H`）：
- **USABLE** → 跳过采集，仅加载 threat_model 摘要到上下文（blindspot_count / priority_areas / top_blindspots）
- **MISSING / STALE / INVALID** → 派发 issue-miner → bug-shape-extractor → threat-modeler（任一失败记录警告继续，Phase 0 非关键路径）

配置从 `settings.json` 的 `intelligence` 节读取：`time_window_months`(默认24) / `max_issues`(500) / `max_commits`(200) / `cache_ttl_hours`(默认720h)。

### Step 4-6: 契约生成（MISSING/STALE/INVALID 时派发）

> 完整 agent 派发 prompt 见 `commands/contract.md` Step 3-5。

- **Step 4**: `Agent(subagent_type="testvdb:knowledge-extractor")` → `results/{target}/{version}/raw_knowledge.json`
- **Step 5**: `Agent(subagent_type="testvdb:contract-formalizer")` → `results/{target}/{version}/structured_contract.json`
- **Step 6**: 合同门控 — `scripts/validate_contract.py`（schema 合法性）+ 核心 CRUD 端点覆盖率 ≥ 90%。不通过 → 输出缺失端点 + 终止会话。
  - 核心 CRUD：collections create/list/get/delete、points insert/get/update/delete、search/recommend
  - 排除管理端点：/indexes/, /partitions/, /aliases/, load, release, flush, compact, /meta, /nodes, /cluster, /users, /roles
  - `material_passport.enabled=true` 时加 `scripts/passport_verify.py`

### Step 7: 初始化状态
创建 `results/{target}/{version}/` 目录（不含 timestamp 子目录），初始化 mine_state.json：

**注意**：timestamp 子目录（`results/{target}/{version}/{timestamp}/`）在 Step 8 第一轮挖掘开始时才创建。这样如果 Step 6 门控失败，不会留下空的 timestamp 子目录。

**Session ID 生成与传递**：
1. 生成格式：`{target}-{version_short}-{counter}`（如 `milvus-2617-r1`、`qdrant-1130-r1`）
   - `version_short`：取 major+minor 拼接（如 `v2.6.17` → `2617`，`v1.13.0` → `1130`）
   - `counter`：从 `r1` 递增，同 target+version 下避免冲突
2. **Sanitization 规则**：只保留 `[a-z0-9-]`，大写转小写，删除 `T`/`:`/`/` 等无效字符，长度限制 63 字符（Docker 容器名限制）
3. **立即设置环境变量**：`export TESTVDB_SESSION_ID="{session_id}"`，确保后续所有子 agent 和 Docker 容器使用统一的 session_id
4. 在所有 Agent 调用的 prompt 中显式传递 `session_id={session_id}`
5. Docker Compose 模板通过 `${TESTVDB_SESSION_ID:-standalone}` 环境变量读取，确保容器名唯一

**Session 锁机制**：创建目录后立即写入 `.session.lock` 文件：
```json
{ "session_id": "{target}-{version_short}-{counter}", "started_at": "...", "status": "active" }
```
所有 agent（包括 Stop/SessionEnd hooks）在清理前必须检查 `.session.lock` 是否存在且 `status` 为 `active`。如果锁存在，不得删除该 session 目录下的任何文件。
```json
{
  "version": 3,
  "session_id": "{target}-{version_short}-{counter}",
  "target": "{target}",
  "version_target": "{version}",
  "current_round": 1,
  "max_rounds": 5,
  "min_defects": 1,
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
    "docker_container_running": false
  },
  "error_log": [],
  "timestamps": {
    "session_started": "{ISO_8601}",
    "last_phase_change": "{ISO_8601}"
  }
}
```

**v3 schema 说明**（跨 Turn 状态机）：
- `phase`：当前所处阶段枚举（ROUND_START → ATTACK_GEN → DEBATE_S1 → EXECUTION → EVIDENCE_BUILD → CHAIN_AUDIT → REPORTING → DEFECT_REVIEW → STATE_SAVE → CLEANUP → DONE）
- `phases_completed`：当前轮次已完成的阶段列表（轮内断点恢复用，每轮重置）
- `phase_data`：每个阶段的产出摘要（供断点恢复时跳过已完成的工作）
- `turn_type`：`setup`（Turn 1）→ `loop`（Loop Turn）→ `done`（完成）
- `global_state`：跨轮次全局状态（缺陷总数、覆盖率、容器状态）

### Step 8: 挖掘循环（每轮）

**每轮开始前**：如果是第一轮，创建 `results/{target}/{version}/{timestamp}/` 目录结构。

#### 8a. 注入 reflection_context + threat_model + cognitive_blindspots

第一轮：无 reflection_context，Attack Agents 自由探索。
后续轮次：注入上轮 reflection_context 到 Attack Agents 的 context：
```json
{
  "key_learnings": ["...", "..."],
  "rejection_patterns": [{ "endpoint": "...", "reason": "..." }],
  "high_value_endpoints": ["..."],
  "exhausted_endpoints": ["..."],
  "last_round_summary": "..."
}
```

**reflection_context 注入模板**：在 Agent 调用的 prompt 参数中，将 reflection_context 以纯文本形式注入：
```
上轮经验：{key_learnings 的要点}。已排除的端点：{exhausted_endpoints}。高价值端点：{high_value_endpoints}。驳回模式：{rejection_patterns 的摘要}
```

### v2.0 跨会话策略注入（evolution.enabled=true）

### v2.1 威胁模型与认知盲点注入（intelligence.enabled=true 且 inject_to_attack_agents=true）

**使用程序化注入脚本**（详见 `commands/mine.md` Step 8a）：

```bash
THREAT_MODEL_ATTACK=$(python scripts/threat_model_injector.py {target} --mode attack --text-only)
```

在每个 Attack Agent prompt 末尾追加 `${THREAT_MODEL_ATTACK}`。注入内容包含：
- 攻击面优先级（top-5 endpoints + recommended_attack_order）
- 开发者认知盲点（top-3 blindspots + attack_strategies）
- 已知 by-design 行为（避免误报）
- 全局策略权重（建议各策略分配比例）
- 盲点 → Attack Agent 映射

**注入条件汇总**：
- `reflection_context != null` → 注入本轮经验
- `evolution.enabled=true` 且 `cross_session_strategies` 有实质内容 → 注入跨会话策略
- `intelligence.enabled=true` 且 `inject_to_attack_agents=true` → 执行 `threat_model_injector.py --mode attack` 并注入结果

### v2.1 威胁模型注入说明（ADR-0008：judge 增强注入已随 Judge Quartet 删除；attack 注入保留）

inject_to_judge_agents 配置废弃。threat_model_injector.py 仅 --mode attack 路径仍在用。
### v2.0 跨会话策略注入（evolution.enabled=true）

策略由 `scripts/strategy_injector.py {target} --text-only` 生成，在 Attack Agent prompt 中注入。

#### 8a.5 阶段评估（两阶段调度，ADR-0009 §2）

每轮 8b 派发前，用 Bash 机械判定当前阶段（确定性，替代 LLM 目测）：

```bash
python scripts/exploration_phase.py switch --round {R} --chunks {N} --no-defect {X} --cov-delta {D} --phase {phase}
# N=len(chunks.json)；X=mine_state.consecutive_no_defect_rounds；D=本轮 coverage 增量
```

- 输出 `switch=false` → 照旧走 8b（阶段一：契约分块枚举派发）。
- 输出 `switch=true`（条件其一：契约块耗尽 R>N；平台期=连续≥2 轮无新缺陷且 Δcoverage≤0）
  → `mine_state` 标记 `phase=exploration`，后续轮次全部走 **8b-expl 探索模式派发**；
  **进入后不回退**（原"轮数>块数则循环重扫"废止——空转改为有意义探索）。
- **探索僵局**：每探索轮结束评估 `python scripts/exploration_phase.py stalled --zero-rounds {Z}`
  （Z=连续零信号命中轮数）；连续 K 轮（settings `mining.exploration.stall_rounds`，默认 3）
  零命中 → 会话终止（探索不是永动机）。

#### 8b-expl. 探索模式派发（阶段二，ADR-0009 §3-§4）

**不分块**：目标面 = 全契约面 + OpenAPI 原始面；endpoint 优先级 = coverage.json
覆盖缺口优先（不消费 bug-shape/intel——GT-free 纪律）。三 attack agent 轮转承接，
reflection_context 继续注入（探索经验进经验环）。

派发 prompt 按规范注入**四算子菜单**（R12：派发词内容即本节定义，不多给）：

| 算子 | 行为 |
|---|---|
| ① 异常响应追踪 | 非 2xx / 超时 / 字段形态异常 → 触发深挖 |
| ② 参数空间组合扰动 | 契约外参数、类型混淆、显式空值、边界交叉 |
| ③ 状态序列扰动 | 并发/交错/删除后操作 |
| ④ 行为一致性对照 | 同族参数异处置、同参数跨接口对照 |

**目标信号定义**（探针命中判据，executor 信号摘要按此标记）：
`non_2xx | timeout | field_anomaly | inconsistent_disposition | semantic_mismatch`

**沙箱小循环（批量探针协议）**：attack agent 单次产出一批探针
（≤ `mining.exploration.probe_batch_size`，默认 8，命名 `probe_{seq}_{operator}.py`，
头部注释标 operator 与 target_endpoint）→ docker-executor 批量执行 → per-probe
信号摘要回喂 → 命中则下一批聚焦该 endpoint 深挖（算子内变异邻域），未命中则
算子/endpoint 轮转。每探索轮 M 批（默认 4）。**禁止 agent 自行执行脚本或 curl**
（沙箱纪律，vein 自跑路径已废止）。

**产出规范**：探针脚本与枚举产出同链——Stage 1 分类 + docker-executor 执行 +
evidence-builder/chain-auditor 全链（无捷径）；候选必须写明缺陷主张（判定层
exploratory 通道 has_claim 依赖，ADR-0009 §5）。

#### 8b. 并发出动 Attack Trio（boundary + state + semantic；attack-vein 已按 ADR-0009 移除，其纵深探索职能由两阶段调度的探索模式承接）

**契约分块派发（ADR-0008，每轮一块）**：派发前先确定性分块：
```bash
python scripts/chunk_contract.py ${SESSION_DIR}/../structured_contract.json --session-dir $SESSION_DIR
# 产出 chunks.json（按 endpoint 分组，每块 ≤12 可攻单元）
```
第 R 轮派发 `chunks[R-1]`（round 1 → chunk 1，round 2 → chunk 2，…，轮数 > 块数则循环）。派发 prompt 中指定 `本轮块={chunk_id}` + 块内 unit_ref 清单——attack agents 只攻该块内单元（策略覆盖目标驱动，见各 agent 规范的"强制输出要求"）。

**完成后更新 pipeline_state** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase DEBATE_S1 --phase-data '{"ATTACK_GEN": {"scripts_generated": N, "agents_completed": [...], "chunk_id": "{chunk_id}"}}'`

**并发（非顺序）** 派三个 Attack Agent（boundary + state + semantic），**必须使用 Agent 工具派生子 agent**，禁止自己直接执行攻击生成：

> **派发者说明（v2.1.2）**：实际由**主进程**（`commands/mine.md`）直接派发这三个 attack agent，**orchestrator 不嵌套派孙 agent**（嵌套派发不可靠，见 `commands/mine.md:18` 与 memory `nested-agent-dispatch-limitation`）。本节描述的是派发的**内容契约**，不是 orchestrator 自行派发。⚠️ 派发依赖环境原生 Task 工具；若当前环境未暴露（非标准 provider），主进程须降级为单 agent 串行执行，或换到支持原生 Task 的环境——此为平台层限制，非代码 bug。

**⛔ 绝对禁止：** Orchestrator 自己生成攻击脚本、自己执行测试、自己审查结果。Orchestrator 只负责编排和协调，所有实质性工作必须通过 Agent 工具派发给对应的子 agent。如果你发现自己正在直接编写 Python 攻击脚本或直接执行 curl 测试，立即停止，改用 Agent 派发。

```
Agent(subagent_type="testvdb:attack-boundary", description="边界攻击 {target} v{version}", prompt="按照 agents/attack-boundary.md 规范，为 {target} v{version} 生成边界攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}。读取 results/{target}/{version}/{timestamp}/pipeline_state.json 了解当前进度")
Agent(subagent_type="testvdb:attack-state", description="状态攻击 {target} v{version}", prompt="按照 agents/attack-state.md 规范，为 {target} v{version} 生成状态攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}。读取 results/{target}/{version}/{timestamp}/pipeline_state.json 了解当前进度")
Agent(subagent_type="testvdb:attack-semantic", description="语义攻击 {target} v{version}", prompt="按照 agents/attack-semantic.md 规范，为 {target} v{version} 生成语义攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}。读取 results/{target}/{version}/{timestamp}/pipeline_state.json 了解当前进度")
```

**自动化输出验证**：每轮 Attack Trio 完成后，使用 Bash 工具执行以下命令验证子 agent 产出：
```bash
ls results/{target}/{version}/{timestamp}/debate_logs/*.py 2>/dev/null | wc -l
```
如果输出为 0（3 个 Agent 均未产出任何脚本文件），说明子 agent 未正常执行，必须终止并报错。如果 >0，继续下一步。

**注意**：不依赖 `subagent-tracking.json` 文件（Claude Code 的 Agent 工具不会自动生成此文件），而是通过检查实际产出文件来验证子 agent 执行结果。

**Subagent 超时机制**：每个 Agent 调用后，如果 3 分钟内子 agent 未产出任何文件（检查目标目录是否有新文件写入），则：
1. 在日志中记录超时
2. 标记该子 agent 为 `timed_out`，跳过其产出
3. 在 mine_state.json 的 error_log 中记录超时事件
4. 如果 3 个 Attack Agent 全部超时，终止当前轮次并记录错误

#### 8c. 辩论 Stage 1（自动化审查 — ADR-0008：脚本去重已删）

**完成后更新 pipeline_state** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase EXECUTION --phase-data '{"DEBATE_S1": {"approved_count": N, "rejected_count": M}}'`

收集三个 Agent（boundary + state + semantic）产出的测试脚本 → Orchestrator **自行执行自动化审查**（非 peer review，不派生子 agent）。这是编排协调工作，与 8b 的"禁止自己直接执行攻击生成"不矛盾——审查不是攻击脚本生成/执行这种实质性工作。

**自动化审查步骤**：

1. **收集脚本**：读取 Attack Agents 产出的所有脚本文件，按来源标记为 boundary/state/semantic
2. **（ADR-0008 删）脚本去重不再执行**——按 endpoint+constraint+strategy 去重会压制合法的多角度攻击同一约束；重复脚本交给执行与 chain-auditor 自然淘汰（同根因候选在 8e.5 缺陷级去重合并）
3. **语法验证**：对每个脚本执行 `python -m py_compile` 验证语法，语法错误进 retry 子循环（4.6）
4. **约束存在性验证**：检查脚本的 constraint_id 是否在 structured_contract.json 中存在，不存在的直接丢弃
4.5. **v2.2 新增 — API 调用格式 AST 验证**：对通过语法验证的脚本，用 Python `ast` 模块检测 API 调用格式：
   - 裸 `.json()` 链式调用（`requests.post(...).json()["key"]` 等）→ **REJECT**（必现 SCRIPT_ERROR）
   - `safe_request` 定义但从未调用 → **REJECT**（欺骗性代码）
   - 全部使用 `safe_request()` 或等效安全包装 → **PASS**
   
   具体执行脚本见 `commands/mine.md` Step 8c 第 6 步。
4.6. **v2.5 新增 — 确定性错误分类 + retry 子循环（反"attack 脚本 ~25%+ SCRIPT_ERROR 直接废弃"）**

   **memory 教训**（meilisearch 实测 57% / chroma 12.5% 静态错误率）：attack agent 跨 target 反复犯 5 类静态错误（`bare_json_chain` / `safe_request_unused` / `cleanup_unwrapped` / `verdict_missing` / `syntax_error`），Stage 1 当前直接丢弃 → 浪费 + 掩盖有效测试方向。本 Step 用确定性脚本分类 + 重派 attack agent 带 feedback 重生成（不是废弃）。借鉴 `pipeline_state._handle_defect_review` 的 retry 设计模式（counter + 超限降级）。

   **Step 4.6.1 — 确定性错误分类**：
   ```bash
   python scripts/_classify_script_errors.py ${SESSION_DIR}
   # 产出 ${SESSION_DIR}/script_errors.json（errors[]: script_id + error_classes + feedback_hints）
   ```

   **Step 4.6.2 — apply retry（确定性脚本，反 LLM 维护 counter 不可靠）**：
   ```bash
   python scripts/_apply_script_retry.py ${SESSION_DIR}
   # stdout JSON: {regen: [...], exhausted: [...], total_errors, max_retry}
   # 副作用：更新 script_retry.json / 写 *.retry_feedback.json / 删超限脚本
   ```
   - `regen` 列表 = 需重派 attack agent 的脚本（counter < `MAX_SCRIPT_RETRY`=2，已写对应 `retry_feedback.json`）
   - `exhausted` 列表 = 已超限降级删掉的脚本（避免 SCRIPT_ERROR 流入 Stage 2 浪费 Executor）

   **⛔ 红线**（呼应反"用答案反推考题"）：`feedback_hints` 由 `_classify_script_errors.py` 内嵌的通用规则提供（"wrap in try/except"），**不是 DB 特定答案**（"测 count API exact=false"）。Orchestrator 不得在 feedback 里写被测参数名/端点名/具体测试值。把 qdrant 换 weaviate/milvus 仍合理 = 通用 = 通过。

   **Step 4.6.3 — 重派 attack agent（带 feedback）**：如有"需重生成"项，按 source 分组派对应 attack agent（boundary/state/semantic）。agent 收到后**覆盖原文件**（script_id 不变），重跑 Step 4.6.1。直到无 error script 或全部超限降级。

   ```
   Agent(subagent_type="testvdb:attack-boundary", description="Retry: fix SCRIPT_ERROR 模式",
     prompt="按 agents/attack-boundary.md § Retry Feedback Handling 规范。
     ${SESSION_DIR}/boundary_scripts/ 下有 N 个脚本被 Stage 1 确定性分类标错，需读各
     ${script_id}.retry_feedback.json 修正后**覆盖原文件**（script_id 不变）。
     错误清单：[script_id → error_classes]。feedback_hints 是通用规则（不是答案），按 hint 修对应错误类，
     保留原脚本没问题的部分，不要从头重写。target={target}, version={version}, SESSION_DIR=${SESSION_DIR}。")
   ```
   （state/semantic 同理，subagent_type 换 `testvdb:attack-state` / `testvdb:attack-semantic`）

   **Step 4.6.4**：retry 子循环结束后，剩下的脚本进 Step 5。

5. **（ADR-0008 删）跨 Agent 交叉审查与 confidence 抽样不再执行**——confidence 字段已从契约与脚本链路删除
7. **记录审查结果**：将审查结果写入 `debate_logs/stage1.json`
8. **脚本路径标准化**：将通过审查的脚本按来源复制到对应的子目录（Executor 在此搜索）。使用 Bash 执行：
   ```bash
   SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}
   mkdir -p ${SESSION_DIR}/boundary_scripts ${SESSION_DIR}/state_scripts ${SESSION_DIR}/scripts
   # 从攻击 Agent 输出目录收集脚本（非 debate_logs/——攻击 Agent 直接写入这些目录）
   # 同时保留 script_{id}.py 在根目录做兜底
   # v2.2: 脚本统一存放在按来源命名的子目录中，不再复制到根目录（避免 Executor 重复扫描）
   for dir in boundary_scripts state_scripts scripts; do
     [ ! -d "${SESSION_DIR}/${dir}" ] && continue
     for src in "${SESSION_DIR}/${dir}"/*.py; do
       [ ! -f "$src" ] && continue
       B=$(basename "$src")
       case "$B" in
         boundary_*) cp "$src" "${SESSION_DIR}/boundary_scripts/$B" ;;
         state_*)    cp "$src" "${SESSION_DIR}/state_scripts/$B" ;;
         semantic_*|*) cp "$src" "${SESSION_DIR}/scripts/$B" ;;
       esac
     done
   done
   # Executor 只扫描子目录，不再扫描根目录的 script_*.py 兜底文件
   touch ${SESSION_DIR}/debate_logs/stage1.json.done
   ```

**审查判定规则**：
- confidence ≥ 0.7 且无重复且语法正确且约束存在且 API 格式通过 → **直接通过**
- confidence < 0.7 或有重复 → 详细审查后决定 approve / reject
- 静态代码错误（语法 / 裸 `.json()` / safe_request 不调用 / cleanup 未包 try / 缺 VERDICT 行）→ **先经 Step 4.6 retry**，超 `MAX_SCRIPT_RETRY`=2 才丢弃（反"~25% 脚本直接废弃浪费"）
- 约束不存在（constraint_id 不在 structured_contract.json）→ **直接丢弃**（attack agent 没读 contract，retry 也修不好）

辩论日志写入 `debate_logs/stage1.json`。**Orchestrator 使用 Write 工具写入此文件**，将审查结果序列化为 JSON 后写入 `results/{target}/{version}/{timestamp}/debate_logs/stage1.json`。

#### 8d. 派 Executor 执行通过辩论的脚本

**完成后更新 pipeline_state** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase EVIDENCE_BUILD --phase-data '{"EXECUTION": {"scripts_executed": N, "scripts_passed": M, "scripts_error": K}}'`

**必须使用 Agent 工具派生 docker-executor 子 agent**，禁止自己直接执行：

```
Agent(subagent_type="testvdb:docker-executor", description="执行 {target} v{version} 攻击脚本", prompt="按照 agents/docker-executor.md 规范，在 Docker 沙箱中执行攻击脚本。target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}, session_id={session_id}。四件套环境变量（X1 修正：R1 五轮穿透根因，必须在 Step 1 前设置）：export TESTVDB_SCRIPTS_DIR=<插件 cache scripts 绝对路径> TESTVDB_TARGET={target} TESTVDB_DB_URL={db_url} TESTVDB_SESSION_ID={session_id}。⛔ 立即执行 Step 1 命令，不要分析、不要检查、不要读取脚本内容。脚本位于 SESSION_DIR 下的 boundary_scripts/、state_scripts/、scripts/ 子目录和 script_*.py 文件中。所有脚本已通过语法验证，无需再检查。")
```

每个脚本一个独立沙箱执行，并发处理。

**自动阻断**：Executor 完成后，使用 Bash 工具执行以下命令验证产出（使用 .done 标记确保文件写入完成）：
```bash
ls results/{target}/{version}/{timestamp}/output_*.log.done 2>/dev/null | wc -l
```
如果输出为 0，**禁止 Orchestrator 自己执行脚本**，必须在 error_log 中记录并终止当前轮次。**⛔ 绝对禁止 Orchestrator 自己运行 Python 脚本或 curl 命令来替代 Executor。如果 Executor 失败，当前轮次终止。**

**容器生命周期管理**：Executor 在 Step 5 执行完脚本后，**不得清理容器**。容器必须保持运行直到 Reporter 完成 Pre-Submit Gate 复现验证（Step 8f）后，由 Orchestrator 在 Step 8j 统一清理。Executor 只负责启动和执行，不负责停止。轮次间如需重置 DB 状态，由 Orchestrator 在 Step 8j 执行 `docker restart`。

#### 8e. 收集结果 → EVIDENCE_BUILD + CHAIN_AUDIT（ADR-0008 证据链双 Agent）

**完成后更新 pipeline_state** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase EVIDENCE_BUILD --phase-data '{"EXECUTION": {"scripts_executed": N, "scripts_passed": M, "scripts_error": K}}'`

**Step 1 — 机械提取候选清单**（fan-out 派发清单，确定性 0 LLM）：
```bash
python scripts/extract_candidates.py $SESSION_DIR
# 产出 candidates.jsonl（VERDICT: DEFECT_FOUND 的 log → 候选；SCRIPT_ERROR 排除）
```

**Step 2 — L1 机械闸门前移**（0 token 杀 ~90% 历史 FP 模式）：
```bash
python scripts/verify_live_l1.py $SESSION_DIR --target {target}
```
REFUTED 候选从 candidates.jsonl 移除（记入 verify_live_l1.json）。verify-live-l2 已删（ADR-0008 B1：其主动 Docker 实测职能与 dev-reviewer 同物种，NEEDS_MORE_EVIDENCE 补证轮覆盖剩余语义情况）。

**Step 3 — evidence-builder 按候选并发派发**（1 builder/候选）：
```
对 candidates.jsonl 每行并发派发（受派发槽位约束）：
Agent(subagent_type="testvdb:evidence-builder", description="证据链构建 {defect_id}",
  prompt="按照 agents/evidence-builder.md 规范，为候选 {defect_id} 构建证据链。target={target}, version={version}, SESSION_DIR=$SESSION_DIR。你的 defect_id={defect_id}。")
```
- 产出 `evidence_chain/{defect_id}.json` + `.done`（按候选命名，并发无写冲突）
- 超时/缺产出候选：不重试，留给 auditor 记 NEEDS_MORE_EVIDENCE

**8e.5 缺陷去重（v2.2，ADR-0008 输入源更新）**

主进程在派发 auditor 前对 candidates 执行跨轮次去重（同 endpoint + 同 defect_type 合并；跨轮与 dedup_state.json 比较）。产出 `debate_logs/stage2_deduped.json`。

**8e.7 CHAIN_AUDIT — chain-auditor 单实例收口**

全部 builder `.done` 后派发（跨候选一致性检查需要完整链集合）：
```
Agent(subagent_type="testvdb:chain-auditor", description="证据链审计 {target}",
  prompt="按照 agents/chain-auditor.md 规范，审计 evidence_chain/ 下全部证据链并产出终判。target={target}, version={version}, SESSION_DIR=$SESSION_DIR。")
```
- 产出 `debate_logs/chain_verdicts.json`（DEFECT / NOT_DEFECT / NEEDS_MORE_EVIDENCE + fp_evidence_source + root_cause 分布）+ `.done`
- 验证：`test -f "$SESSION_DIR/debate_logs/chain_verdicts.json.done" && echo READY || echo PENDING`
- NEEDS_MORE_EVIDENCE > 0 → 仅对标记的 defect_id 重派 builder 补证一轮（最多 1 次），再重派 auditor 出最终 verdict；第二轮仍矛盾 → NOT_DEFECT（保守）
- **⛔ 主进程绝不做判定。auditor 两轮均超时 → 全候选保守 NOT_DEFECT + error_log。**

**完成后更新 pipeline_state**: `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase REPORTING --phase-data '{"CHAIN_AUDIT": {"verdict_defect": N, "not_defect": M, "needs_more_evidence": K}}'`

**experience_handoff 采集**：auditor 的 `root_cause_distribution` 与 `fp_evidence_source_distribution` 采集进 experience_handoff.json 的 rejection_patterns（词表沿用原 dev-reviewer root_cause_if_fp）。

#### 8f. 派 Reporter

**confirm_per_round 开关（ADR-0009 §6）**：进入本步前先用 Bash 确定性读取开关：
```bash
python scripts/get_setting.py mining.confirm_per_round
```
- 输出 `true`（默认）→ 照常执行本步与 8f.5（产品行为：轮内 confirmation）。
- 输出 `false`（实验特化）→ **跳过 8f 与 8f.5**：不派 Reporter，缺陷审查与 Pre-Submit Gate 延后；candidates.jsonl 继续累积（8e.5 跨轮去重照常执行），容器保持运行（生命周期管理规则不变，仍由 8j 统一处理）；`pipeline_state` 标记 `phase=MINING_DEFER_CONFIRM`。会话终止（时间到/轮数到/僵局终止）后，对累积的全部候选执行**统一判定**：evidence-builder + chain-auditor 批量（机械预跑 + SOP 聚合，与轮内路径同一规范，ADR-0008）→ novelty 终判 → Reporter + Pre-Submit Gate 一次性收口。

**完成后更新 pipeline_state** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase DEFECT_REVIEW`

**必须使用 Agent 工具派生 reporter 子 agent**：

```
Agent(subagent_type="testvdb:reporter", description="生成缺陷报告 {target}", prompt="按照 agents/reporter.md 规范，为以下 Debate-Confirmed 缺陷生成报告：{debate_confirmed}。session_id={session_id}, target={target}, version={version}, session_dir=results/{target}/{version}/{timestamp}。读取 results/{target}/{version}/{timestamp}/pipeline_state.json 了解当前进度")
```

**自动化输出验证**：Reporter 完成后，使用 Bash 工具执行以下命令验证产出：
```bash
ls results/{target}/{version}/{timestamp}/defects/defect-*.md 2>/dev/null | wc -l
```
如果输出为 0，说明 Reporter 未正常执行，必须在 error_log 中记录。

**证据链验证要求**：Reporter 生成的每个 defect-N.md 必须包含完整的证据链：
- **Ring 2（文档引用）**：source_url 必须可达，doc_version 必须与目标 major.minor 匹配
- **Ring 4（源代码引用）**：如果缺陷涉及特定代码路径，必须包含 github_url

**Pre-Submit Gate 复现验证**：Reporter 必须对每个确认的缺陷执行复现验证（详见 agents/reporter.md 的 Pre-Submit Gate 章节），只有 100% 复现的缺陷才产出最终报告。

#### 8f.5 逐缺陷全面审查（v2.2 新增 — 每轮末尾逐条审核 Reporter 产出）

**⛔ 铁律：主进程只做编排。** 主进程执行 `python scripts/verify_defects.py` 对每个 defect-N.md 审查：
1. 证据链完整性（Ring 1/2/3 是否齐全）
2. 严重性校准（基于执行日志重新确认）
3. 脚本错误排除（检查 SCRIPT_ERROR 标记）
4. 假阳性识别（VERDICT 行 vs 报告声称）

产出 `defect-review.md`，标记每个缺陷 CONFIRMED / FALSE_POSITIVE / NEEDS_IMPROVEMENT。
FALSE_POSITIVE → 删除对应 defect-N.md。NEEDS_IMPROVEMENT → 打回 Reporter 重写（最多 1 次）。

#### 8g. 保存状态

**完成后更新 pipeline_state** (CLI, ADR-0004): `python scripts/pipeline_state.py advance --session-dir $SESSION_DIR --phase STATE_SAVE`
每轮结束保存 mine_state.json + coverage.json + experience_handoff.json + pipeline_state.json。

**pipeline_state.json（v3 跨 Turn 状态机，ADR-0004）：**

由 `scripts/pipeline_state.py` CLI 管理，禁止手动构造 JSON。Schema 参考 [pipeline_state.py](scripts/pipeline_state.py) 的 `PipelineState.create()`。

每个子步骤完成后，调用 `pipeline_state.py advance --phase <NEXT> [--phase-data '...']`。phase 转换受硬编码 transition map 校验（无效跳转 → InvalidTransition 报错）。全局状态计数器通过 `pipeline_state.py mutate --total-defects N --coverage P...` 更新。跨 Turn 恢复时，`reconstruct_context.py` 读取此文件确定断点。

### Agent 间通信可靠性机制（.done 标记文件）

由于子 Agent 通过 Agent 工具异步派发，所有 Agent 间通信通过文件系统。为确保文件写入的原子性和可见性：

1. **子 Agent 输出规范**：先写入输出文件，完成后创建同名 `.done` 标记文件
2. **Orchestrator 检查规范**：**必须**检查 `.done` 标记文件存在性（而非仅检查输出文件——文件可能正在写入）
3. **检查命令**：`test -f "{file}.done" && echo "READY" || echo "PENDING"`
4. **超时处理**：输出文件存在但 `.done` 不存在超过 60 秒 → 子 Agent 卡住，触发超时
5. **Orchestrator 写入规范**：先写 `.tmp` 临时文件，完成后 rename + touch `.done`

**experience_handoff.json 写入逻辑：**
- 记录本轮关键发现：debate_confirmed 的 endpoint 分布、驳回原因分类、新发现的高价值攻击策略
- 记录当前判定链状态：L1 refuted / verdict_defect / not_defect / needs_more_evidence 计数（ADR-0008）
- 供下次 session 或上下文压缩恢复时快速理解当前进度

**experience_handoff.json 模板**（Orchestrator 使用 Write 工具写入）：
```json
{
  "session_id": "{session_id}",
  "target": "{target}",
  "version": "{version}",
  "round": {current_round},
  "timestamp": "{ISO 8601}",
  "key_findings": [
    {"endpoint": "...", "defect_type": "...", "confidence": 0.0, "summary": "..."}
  ],
  "chain_stats": {
    "candidates": 0,
    "l1_refuted": 0,
    "verdict_defect": 0,
    "not_defect": 0,
    "needs_more_evidence": 0
  },
  "rejection_patterns": [
    {"endpoint": "...", "reason": "by-design|false_positive|irreproducible|insufficient_evidence"}
  ],
  "high_value_endpoints": ["..."],
  "exhausted_endpoints": ["..."],
  "next_action": "continue_mining|stalemate|terminate"
}
```

**coverage.json 模板**（Orchestrator 使用 Write 工具写入）：
```json
{
  "session_id": "{session_id}",
  "target": "{target}",
  "version": "{version}",
  "round": {current_round},
  "timestamp": "{ISO 8601}",
  "endpoint_coverage": {
    "{endpoint}": {
      "constraints_tested": 0,
      "constraints_total": 0,
      "defects_found": 0,
      "last_tested_round": 0
    }
  },
  "overall_coverage_pct": 0.0,
  "core_crud_coverage_pct": 0.0
}
```

#### 8h. 分析本轮产出
- 投票分歧模式分析
- 驳回原因分类（by-design / 假阳性 / 不可复现 / 证据不足）
- endpoint 覆盖率更新
- 生成 reflection_context 供下轮使用

### v2.0 策略提取（evolution.enabled=true）

每轮结束后（或在 Step 9 统一执行），运行：
```bash
python scripts/strategy_extractor.py "results/{target}/{version}/{timestamp}" {target}
```

策略提取逻辑：
1. 读取本轮 experience_handoff.json
2. 提取 confirmed_defects 的策略模式 → 泛化 → 合并
3. 新策略 → 写入 strategy_registry（global + per-DB）
4. 已有策略 → 更新 performance 计数 + 调整 confidence
5. 追加 evolution_log.jsonl 审计条目

#### 8i. 检查终止条件
以下任一满足即终止循环：
1. 连续 5 轮无新缺陷
2. 合同覆盖率 ≥ 95%
3. max_rounds 达到（且 > 0）
4. min_defects 达到

#### 8j. 轮次间容器管理
- **继续下一轮**：重启 DB 容器以重置状态（`docker restart testvdb-{target}-${TESTVDB_SESSION_ID:-standalone}`），保留数据卷
- **终止循环**：执行完整清理（`docker compose -f docker/{target}.yml down -v`），释放所有资源

### Step 9: Issue 草稿 + 汇总报告 + 强制容器清理

**⛔ 绝对禁止：主进程或任何 Agent 直接提交 Issue 到 GitHub 仓库。所有产出仅限本地文件系统。**

1. **生成 Issue 格式草稿**（v2.2 新增）：
   ```bash
   mkdir -p results/{target}/{version}/{timestamp}/issues
   ```
   对每个通过 8f.5 审查的 CONFIRMED 缺陷，生成 `issues/issue-{N}-{slug}.md`，包含完整的 Bug Report 格式（Title, Description, Version, Steps to Reproduce, Expected/Actual Behavior, Impact, Environment, MRE path）。底部标注"本地草稿，需人工审核后手动提交"。

2. 生成 `summary.md` + `defect-review.md` 汇总报告
3. **强制容器清理**：执行以下命令清理所有本次会话创建的 Docker 容器和网络：
   ```bash
   docker compose -f docker/{target}.yml down -v --remove-orphans
   docker network rm testvdb-net-${TESTVDB_SESSION_ID:-standalone} 2>/dev/null || true
   ```
4. 验证清理完成：`docker ps --filter "name=testvdb-{target}" --format "{{.Names}}"` 应无输出
5. 更新 `.session.lock` 的 status 为 `completed`

### 僵局处理（连续5轮无新缺陷时触发）
1. 派生 Knowledge Extractor 重新搜索文档变更 + 新 issue + 社区讨论
2. 将所有搜索结果投放给 Judge Agents 重新审视上一轮候选缺陷
3. 对低覆盖率端点调整 Attack Agents 攻击策略
4. 如仍无发现 → 终止

### Zero 缺陷判定
跑完全部轮次零产出 → 在 session_metadata.json 标注 `ZERO_DEFECT`，生成诊断报告：
- 哪些端点被测试、哪些约束被遗漏
- 覆盖率分析
- 建议改进方向

---

## 生命周期管理

> 错误处理、上下文压缩保护、进度可见性、多 DB 并行 — 详见 `agents/orchestrator-lifecycle.md`。

---

## 数据流规范

```
Orchestrator
  │
  ├──▶ [Phase 0: Strategic Intelligence — v2.1 NEW]
  │     │
  │     ├──▶ Issue Miner ──▶ issue_corpus.json + commit_corpus.json
  │     │                          │
  │     ├──▶ Bug Shape Extractor ◀─┘
  │     │           │
  │     │           ▼
  │     │     bug_shapes.json + classified_issues.json + developer_cognition.json
  │     │           │
  │     ├──▶ Threat Modeler ◀──────┘
  │     │           │
  │     │           ▼
  │     │     threat_model.json (attack priorities + cognitive blindspots)
  │     │
  ├──▶ Knowledge Extractor ──▶ raw_knowledge.json
  │                                      │
  ├──▶ Contract Formalizer ◀─────────────┘
  │           │
  │           ▼
  │     structured_contract.json + sdk.version + available_tags
  │           │
  ├──▶ Attack Trio (并发) ◀── contract + reflection_context + threat_model + cognitive_blindspots
  │     boundary │ state │ semantic
  │           ▼
  │     test_scripts[]
  │           │
  ├──▶ 辩论 Stage 1 (Orchestrator 自行执行自动化审查：去重+语法验证+约束验证)
  │           │
  │           ▼
  │     approved_scripts[]
  │           │
  ├──▶ Executor (并发) ◀── approved_scripts[]  [容器保持运行]
  │           │
  │           ▼
  │     execution_results[]
  │           │
  ├──▶ extract_candidates (机械提取) ──▶ verify_live_l1 (L1 机械闸门, 0 token)
  │           │
  │           ▼
  ├──▶ evidence-builder × N (按候选并发, ADR-0008) ◀── candidates.jsonl + contract + src clone
  │     step1: 文档验证+执行证据审查+证据链追溯
  │     step2: 源码搜证
  │           │
  │           ▼
  ├──▶ chain-auditor (单实例收口) ──▶ chain_verdicts.json
  │     (DEFECT/NOT_DEFECT/NEEDS_MORE_EVIDENCE + fp_evidence_source + root_cause)
  │           │
  ├──▶ Reporter ◀── confirmed_defects[]  [复用运行中容器做 Pre-Submit Gate]
  │           │
  │           ▼
  │     defect-N.md + MRE + summary.md
  │           │
  └──▶ 容器清理 (docker compose down -v)
```

---

## 输出产物

```
results/{target}/{version}/{timestamp}/
├── defects/           # 缺陷报告 (defect-1.md, defect-N.md)
├── summary.md          # 本轮汇总
├── debate_logs/        # 辩论日志 (stage1.json, stage2.json)
├── structured_contract.json  # 契约
├── raw_knowledge.json    # 原始知识
├── mine_state.json     # 状态快照
├── coverage.json       # 覆盖率跟踪
├── session_metadata.json     # 会话元数据
└── experience_handoff.json   # 经验交接

intelligence/{target}/                # v2.1 战略情报层
├── issue_corpus.json                 # 原始 Issue 语料
├── commit_corpus.json                # 原始 Commit/PR 语料
├── classified_issues.json            # 三分类结果 (positive/negative/invalid)
├── bug_shapes.json                   # 根因模式 (root cause patterns)
├── developer_cognition.json          # 开发者认知边界分析
└── threat_model.json                 # 威胁模型 + 认知盲点 + 攻击优先级
```
