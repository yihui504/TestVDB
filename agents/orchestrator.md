---
name: orchestrator
description: TestVDB 缺陷挖掘流水线主编排器。协调全部 16 个 Agent 完成从战略情报采集到缺陷报告的全流程。
model: opus
dataAccess: redacted
maxTurns: 120
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

你只能访问所有 Agent 的产出文件（structured_contract.json, raw_knowledge.md, pipeline_state.json,
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
□ [Step 3] 检查缓存（raw_knowledge.md + structured_contract.json，含 TTL 计算）
□ [Step 3.6] 如 intelligence.enabled=true：历史情报采集（issue-miner → bug-shape-extractor → threat-modeler）
□ [Step 4] 如缓存未命中：派 Knowledge Extractor 获取文档
□ [Step 5] 如缓存未命中：派 Contract Formalizer 生成契约
□ [Step 6] 合同门控检查（核心 CRUD 端点覆盖率 ≥ 90%）
□ [Step 7] 初始化 mine_state.json + 设置 TESTVDB_SESSION_ID 环境变量
□ [Step 8] 开始挖掘循环（最多 max_rounds 轮）：
  □ 8a. 注入 reflection_context + threat_model + cognitive_blindspots 到 Attack Agents
  □ 8b. 并发出动 Attack Trio（boundary + state + semantic）
  □ 8c. Orchestrator 自行执行辩论 Stage 1（交叉审查 + 去重）
  □ 8d. 派 Executor 在沙箱中执行通过辩论的脚本（容器保持运行）
  □ 8e. 收集执行结果 → 辩论 Stage 2（Judge Quartet 分两阶段，注入 judge_enhancements）
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
| target | ✅ | — | milvus / qdrant / weaviate / pgvector |
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
- target 必须在 {milvus, qdrant, weaviate, pgvector} 内，否则报错退出
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

### Step 3: 缓存检查
检查路径 `results/{target}/{version}/structured_contract.json`：
- 存在且 `cache_ttl_hours` 未过期 → 跳过 Step 4-5
- 否则执行完整知识提取流程

**TTL 过期计算**：从 `settings.json` 的 `knowledge.cache_ttl_hours` 读取 TTL（默认 168 小时 = 7 天）。读取 `structured_contract.json` 中的 `cached_at`（ISO 8601 时间戳），计算 `当前时间 - cached_at > cache_ttl_hours`。如果 `cached_at` 字段缺失，视为缓存无效。

### Step 3.6: 历史情报采集（v2.1 新增，intelligence.enabled=true 时）

**⛔ 铁律：主进程只做编排，不做执行。** 本步骤的所有实质性工作通过 `Agent(subagent_type="testvdb:xxx")` 派发。

如果 `intelligence.enabled=false`，跳过整个 Step 3.6。

**主进程在派发以下 Agent 前，先从 settings.json 读取 intelligence 配置并提取为模板变量：**
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

检查 `intelligence/{target}/threat_model.json` 是否存在且未过期（TTL = `intelligence.cache_ttl_hours`，默认 720h）。

如果缓存有效 → 跳到 Step 3.6e（仅加载 threat_model 到上下文）。

#### 3.6b: 派发 issue-miner（⛔ 禁止自己爬取 GitHub）

```
Agent(
  subagent_type="testvdb:issue-miner",
  description="采集 {target} 历史 Issues 和 Commits",
  prompt="按照 agents/issue-miner.md 规范...target={target}, version={version}, intelligence_dir=intelligence/{target}/, time_window_months={INTEL_TW}, max_issues={INTEL_MI}, max_commits={INTEL_MC}。"
)
```

**如果失败** → 记录警告到 error_log，跳过 3.6c/3.6d，继续 Step 4（Phase 0 非关键路径）。

#### 3.6c: 派发 bug-shape-extractor

```
Agent(subagent_type="testvdb:bug-shape-extractor", ...)
```

失败 → 记录警告，继续 Step 4。

#### 3.6d: 派发 threat-modeler

```
Agent(subagent_type="testvdb:threat-modeler", ...)
```

失败 → 记录警告，继续 Step 4。

#### 3.6e: 加载情报摘要到上下文

从 threat_model.json 提取关键字段（blindspot_count、priority_areas、top_blindspots）供后续步骤注入。

### Step 4: 派 Knowledge Extractor
使用 Agent 工具派 knowledge-extractor agent。所有子 Agent 通过对应的 `testvdb:` 命名类型派发，Agent 定义（frontmatter 中的 tools/maxTurns/model）由插件系统自动加载。

```
Agent(
  subagent_type="testvdb:knowledge-extractor",
  description="提取 {target} {version} 文档知识",
  prompt="按照 agents/knowledge-extractor.md 规范，为 {target} {version} 提取 API 文档知识，产出 raw_knowledge.md。输入参数: target={target}, version={version}, session_dir=results/{target}/{version}。将结果写入 results/{target}/{version}/raw_knowledge.md"
)
```

确保产出 raw_knowledge.md 后继续。使用 Bash 执行 `ls -la results/{target}/{version}/raw_knowledge.md` 验证文件存在。

### Step 5: 派 Contract Formalizer
使用 Agent 工具派 contract-formalizer agent：

```
Agent(
  subagent_type="testvdb:contract-formalizer",
  description="形式化 {target} v{version} API 契约",
  prompt="按照 agents/contract-formalizer.md 规范，将 results/{target}/{version}/raw_knowledge.md 转换为 structured_contract.json。输入参数: target={target}, version={version}, session_dir=results/{target}/{version}。将结果写入 results/{target}/{version}/structured_contract.json"
)
```

确保产出 structured_contract.json 后继续。使用 Bash 执行 `ls -la results/{target}/{version}/structured_contract.json` 验证文件存在。

### Step 6: 合同门控检查
检查 structured_contract.json 的端点覆盖率：
- **核心 CRUD 端点覆盖率 ≥ 90%** → 通过
- 不通过 → 输出缺失端点列表 + 清理 `results/{target}/{version}/` 下的 mine_state.json（如果已创建）+ 拒绝进入 Mine，终止会话

核心 CRUD 分类规则：
- 排除管理端点：/indexes/, /partitions/, /aliases/, load, release, flush, compact, /meta, /nodes, /cluster, /users, /roles
- 对四 DB 通用，不做 per-DB 特殊判断

**覆盖率计算方式**：`核心 CRUD 端点覆盖率 = api_endpoints 中属于核心 CRUD 的端点数 / 文档中已知的核心 CRUD 端点总数`。核心 CRUD 端点包括：collections 的 create/list/get/delete、points 的 insert/get/update/delete、search 的 search/recommend。

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
  "session_id": "{target}-{version_short}-{counter}",
  "pipeline_state": "mining",
  "phase": "round_1",
  "target": "{target}",
  "version": "{version}",
  "current_round": 1,
  "max_rounds": 5,
  "min_defects": 1,
  "progress": { "scripts_generated": 0, "scripts_executed": 0, "defects_confirmed": 0 },
  "defects": [],
  "contract": {},
  "reflection_context": null,
  "docker_state": "not_started",
  "error_log": [],
  "timestamps": { "started_at": "..." }
}
```

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

在跨会话策略之后，追加从 Threat Model 提取的攻击优先级和认知盲点：

```
## 威胁模型与认知盲点注入（v2.1 Strategic Intelligence）

### 攻击面优先级
以下区域在当前 DB 的历史中具有最高缺陷密度，应优先攻击：
{从 threat_model.json 的 attack_priority_map 提取的 top-5 endpoints 及其推荐攻击策略}

### 开发者认知盲点
以下盲点是开发者在该代码库中系统性遗漏的模式：
{从 threat_model.json 的 cognitive_blindspots 提取的 top-3 blindspots}

### 已知 by-design 行为（避免误报）
{从 threat_model.json 的 defect_criteria.by_design_behaviors 提取}

### 全局策略权重
基于历史缺陷分布，建议各攻击策略权重：
- boundary_attacks: {weight}
- type_confusion_attacks: {weight}
- state_consistency_attacks: {weight}
- semantic_contract_attacks: {weight}
```

**注入条件汇总**：
- `reflection_context != null` → 注入本轮经验
- `evolution.enabled=true` 且 `cross_session_strategies` 有实质内容 → 注入跨会话策略
- `intelligence.enabled=true` 且 `inject_to_attack_agents=true` 且 `threat_model.json` 存在 → 注入威胁模型与认知盲点

### v2.1 Judge Agent 增强注入（intelligence.enabled=true 且 inject_to_judge_agents=true）

在派发 Judge Agent 之前（Step 8e），将威胁模型的 `judge_enhancements` 部分注入到对应 Judge 的 prompt：

- **judge-severity**：注入 `severity_calibration` 规则
- **judge-novelty**：注入 `novelty_context`（最近修复的模式、已知进行中的 issue）
- **judge-evidence**：注入 `submission_success_probability`（基于开发者历史态度预测提交成功率）

在 reflection_context 之后，追加从 Strategy Registry 读取的策略：
```
## 跨会话策略注入

以下策略来自之前成功挖掘的经验（跨 DB 迁移）：

{cross_session_strategies 的输出}

使用这些策略作为初始 seed。对于标记了 applicable_dbs 包含当前 DB 的策略，
应用 migration_rules 中的 DB 特定适配规则。
```

策略由 `scripts/strategy_injector.py {target} --text-only` 生成。

#### 8b. 并发出动 Attack Trio
**并发（非顺序）** 派三个 Attack Agent，**必须使用 Agent 工具派生子 agent**，禁止自己直接执行攻击生成：

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

### v2.0 Fan-Out 模式（fan_out.enabled=true）

当 Fan-Out 启用时，每个 Attack Agent 使用 3 种 focus_profile 各派发一次：

| Profile | 策略 | Agent prompt 差异 |
|---------|------|-------------------|
| `priority_first` | 从 contract 中 severity 最高的约束开始 | 无额外指令（默认行为） |
| `coverage_gap` | 从 coverage.json 中覆盖率最低的端点开始 | 注入 uncovered_endpoints 列表 |
| `rejection_pattern` | 从上轮 false positive 反向推导新攻击 | 注入 rejection_patterns，"绕过已知驳回模式" |

9 组脚本 → 统一汇聚 → Stage 1 去重 + 交叉审查

**去重规则（3 级）：**
1. 按 (endpoint, constraint_id, strategy) 三级去重
2. 相同三元组 → 保留 confidence 最高的版本
3. 跨 profile 重复检测 → 不同 seed 独立生成相同脚本 → confidence +0.1

**首轮建议：** 先用 `seeds_per_agent=2` 测试，确认去重逻辑正确后再增加到 3。

#### 8c. 辩论 Stage 1（自动化审查 + 去重 + 交叉审查）

收集三个 Agent 产出的测试脚本 → Orchestrator **自行执行自动化审查**（非 peer review，不派生子 agent）。这是编排协调工作，与 8b 的"禁止自己直接执行攻击生成"不矛盾——审查不是攻击脚本生成/执行这种实质性工作。

**自动化审查步骤**：

1. **收集脚本**：读取三个 Attack Agent 产出的所有脚本文件，按来源标记为 boundary/state/semantic
2. **自动去重**：按 `endpoint + constraint_id + strategy` 组合去重，只保留 confidence 最高的脚本。高 confidence（≥0.7）且无重复的脚本直接通过
3. **语法验证**：对每个脚本执行 `python -m py_compile` 验证语法，语法错误直接丢弃
4. **约束存在性验证**：检查脚本的 constraint_id 是否在 structured_contract.json 中存在，不存在的直接丢弃
5. **跨 Agent 交叉审查**：对跨 Agent 重复的脚本（相同 endpoint+constraint 被多个 Attack Agent 独立生成），比较各 Agent 的实现：
   - 各 Agent 使用不同测试值/策略 → 选择覆盖最广的版本
   - 各 Agent 使用相同测试值 → 保留 confidence 最高的版本
   - 交叉验证通过的脚本 confidence 提升 0.1
6. **抽样审查**：只对 confidence < 0.7 或跨 Agent 重复的脚本做详细审查（评估预期是否合理、攻击策略是否匹配）
7. **记录审查结果**：将审查结果写入 `debate_logs/stage1.json`
8. **脚本路径标准化**：将通过审查的脚本按来源复制到对应的子目录（Executor 在此搜索）。使用 Bash 执行：
   ```bash
   SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}
   mkdir -p ${SESSION_DIR}/boundary_scripts ${SESSION_DIR}/state_scripts ${SESSION_DIR}/scripts
   # 从攻击 Agent 输出目录收集脚本（非 debate_logs/——攻击 Agent 直接写入这些目录）
   # 同时保留 script_{id}.py 在根目录做兜底
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
   touch ${SESSION_DIR}/debate_logs/stage1.json.done
   ```

**审查判定规则**：
- confidence ≥ 0.7 且无重复且语法正确且约束存在 → **直接通过**
- confidence < 0.7 或有重复 → 详细审查后决定 approve / reject
- 语法错误或约束不存在 → **直接丢弃**

辩论日志写入 `debate_logs/stage1.json`。**Orchestrator 使用 Write 工具写入此文件**，将审查结果序列化为 JSON 后写入 `results/{target}/{version}/{timestamp}/debate_logs/stage1.json`。

#### 8d. 派 Executor 执行通过辩论的脚本
**必须使用 Agent 工具派生 docker-executor 子 agent**，禁止自己直接执行：

```
Agent(subagent_type="testvdb:docker-executor", description="执行 {target} v{version} 攻击脚本", prompt="按照 agents/docker-executor.md 规范，在 Docker 沙箱中执行攻击脚本。target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}, session_id={session_id}。⛔ 立即执行 Step 1 命令，不要分析、不要检查、不要读取脚本内容。脚本位于 SESSION_DIR 下的 boundary_scripts/、state_scripts/、scripts/ 子目录和 script_*.py 文件中。所有脚本已通过语法验证，无需再检查。")
```

每个脚本一个独立沙箱执行，并发处理。

**自动阻断**：Executor 完成后，使用 Bash 工具执行以下命令验证产出（使用 .done 标记确保文件写入完成）：
```bash
ls results/{target}/{version}/{timestamp}/output_*.log.done 2>/dev/null | wc -l
```
如果输出为 0，**禁止 Orchestrator 自己执行脚本**，必须在 error_log 中记录并终止当前轮次。**⛔ 绝对禁止 Orchestrator 自己运行 Python 脚本或 curl 命令来替代 Executor。如果 Executor 失败，当前轮次终止。**

**容器生命周期管理**：Executor 在 Step 5 执行完脚本后，**不得清理容器**。容器必须保持运行直到 Reporter 完成 Pre-Submit Gate 复现验证（Step 8f）后，由 Orchestrator 在 Step 8j 统一清理。Executor 只负责启动和执行，不负责停止。轮次间如需重置 DB 状态，由 Orchestrator 在 Step 8j 执行 `docker restart`。

#### 8e. 收集结果 → 辩论 Stage 2
将执行结果分发给 Judge Quartet（**4 个 Judge，分两阶段派发**）：

**阶段 1：先派 judge-doc（文档契约验证）**
```
Agent(subagent_type="testvdb:judge-doc", description="文档契约验证 {target}", prompt="按照 agents/judge-doc.md 规范，验证以下候选缺陷的文档引用有效性：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}。读取 results/{target}/{version}/{timestamp}/pipeline_state.json 了解当前进度")
```

**自动化输出验证**：等待 judge-doc 完成后，使用 Bash 工具执行以下命令验证产出（检查 .done 标记确保写入完成）：
```bash
test -f "results/{target}/{version}/{timestamp}/debate_logs/stage2_doc.json.done" && echo "READY" || echo "PENDING"
```
如果输出为 PENDING（含超时 60s 仍 PENDING），说明 judge-doc 未正常执行，必须在 error_log 中记录并终止当前轮次。

**阶段 2：确认 stage2_doc.json 存在后，并发派其他 3 个 Judge**
```
Agent(subagent_type="testvdb:judge-evidence", description="证据审查 {target}", prompt="按照 agents/judge-evidence.md 规范，审查以下执行结果的证据可信度：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}。读取 results/{target}/{version}/{timestamp}/pipeline_state.json 了解当前进度")
Agent(subagent_type="testvdb:judge-novelty", description="新颖性审查 {target}", prompt="按照 agents/judge-novelty.md 规范，审查以下候选缺陷的新颖性：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}。读取 results/{target}/{version}/{timestamp}/pipeline_state.json 了解当前进度")
Agent(subagent_type="testvdb:judge-severity", description="严重性评估 {target}", prompt="按照 agents/judge-severity.md 规范，评估以下候选缺陷的严重程度：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}。读取 results/{target}/{version}/{timestamp}/pipeline_state.json 了解当前进度")
```

**自动阻断**：4 个 Judge 全部完成后，使用 Bash 工具执行以下命令验证产出（所有文件必须都有 .done 标记）：
```bash
echo "doc: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_doc.json.done && echo 1 || echo 0)"
echo "evidence: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_evidence.json.done && echo 1 || echo 0)"
echo "novelty: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_novelty.json.done && echo 1 || echo 0)"
echo "severity: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_severity.json.done && echo 1 || echo 0)"
```
如果任一 Judge 计数为 0，**禁止 Orchestrator 自己做 Judge 判断**，必须在 error_log 中记录缺失的 Judge 名称。**⛔ 绝对禁止 Orchestrator 自己执行 WebSearch 或代码审查来替代 Judge。如果 Judge 失败，缺失的 Judge 投 not_defect（保守策略）。**

**交叉审查规则（防止自评偏见）：**
- 每个 Judge Agent 独立审查全部执行结果，不得参考其他 Judge 的投票
- Judge Quartet 四票独立投票，无作者-审查者角色冲突
- judge-doc 的 doc_verification_result 作为权重调节器，影响其他 3 个 Judge 的审查严格度

**投票逻辑（加权 AND，非简单多数票）：**

evidence 和 severity 按 is_defect/not_defect 投票，novelty 永远投 is_defect 但附加 novelty_rating 元数据，doc 作为权重调节器：
1. **文档门控**（judge-doc）：产出 DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH，调节其他 Judge 审查严格度
2. **证据门控**（judge-evidence）：证据等级 D → 自动 not_defect，无需继续
3. **严重性门控**（judge-severity）：severity = trivial → not_defect
4. **新颖性标记**（judge-novelty）：永远投 is_defect，仅标记 `new` / `new_similar` / `already_reported`，不影响缺陷确认

**缺陷确认规则（按优先级判定）：**
1. evidence=not_defect → **丢弃**（证据不足，记录驳回原因，不检查 severity）
2. severity=trivial → **丢弃**（影响过小不值得报告，记录驳回原因）
   - **重要**：severity 降级逻辑（如 DOC_PARTIAL → 自动降级）可能在 judge-severity 内部将 Low 降为 trivial，此降级不代表缺陷不存在，仅影响是否值得单独报告。降级被丢弃的缺陷记录到 `downgraded_defects` 数组，供 reflection_context 参考
3. evidence=is_defect AND severity∈{Critical,High,Medium,Low} → **确认缺陷**
4. novelty_rating 附加到缺陷元数据，不影响确认状态，但：
   - `new` / `new_similar` → 正常优先级
   - `already_reported` / `known_wontfix` → 降级为 P3 优先级，但仍生成报告（标注关联 issue）
5. doc_verification_result 附加到缺陷元数据：
   - DOC_VERIFIED → 正常格式
   - DOC_PARTIAL → 标注文档引用为 PARTIAL，严重性自动降一级（但仅影响 severity 输出，不影响 evidence 判定）
   - DOC_MISMATCH → 标注文档引用不匹配，严重性自动降两级，但**不阻塞缺陷确认**（只要 evidence 确认即可）

辩论日志写入 `debate_logs/stage2.json`（含 stage2_doc.json 的文档验证结果）。

#### 8f. 派 Reporter
**必须使用 Agent 工具派生 reporter 子 agent**：

```
Agent(subagent_type="testvdb:reporter", description="生成缺陷报告 {target}", prompt="按照 agents/reporter.md 规范，为以下确认的缺陷生成报告：{confirmed_defects}。session_id={session_id}, target={target}, version={version}, session_dir=results/{target}/{version}/{timestamp}。读取 results/{target}/{version}/{timestamp}/pipeline_state.json 了解当前进度")
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

#### 8g. 保存状态
每轮结束保存 mine_state.json + coverage.json + experience_handoff.json + pipeline_state.json。

**pipeline_state.json（Agent 间协调状态文件）：**
```json
{
  "current_round": 1,
  "phase": "attack_generation|debate_s1|execution|debate_s2|reporting",
  "attack_trio_done": false,
  "debate_s1_done": false,
  "execution_done": false,
  "debate_s2_done": false,
  "judge_doc_done": false,
  "reporting_done": false,
  "confirmed_defects_count": 0,
  "scripts_generated": 0,
  "scripts_executed": 0,
  "next_agent": "attack_trio",
  "agent_markers": {}
}
```
每个子 agent 完成后，Orchestrator 更新 pipeline_state.json 中的对应字段。后续 agent 可读取此文件了解当前进度。

### Agent 间通信可靠性机制（.done 标记文件）

由于子 Agent 通过 Agent 工具异步派发，所有 Agent 间通信通过文件系统。为确保文件写入的原子性和可见性：

1. **子 Agent 输出规范**：先写入输出文件，完成后创建同名 `.done` 标记文件
2. **Orchestrator 检查规范**：**必须**检查 `.done` 标记文件存在性（而非仅检查输出文件——文件可能正在写入）
3. **检查命令**：`test -f "{file}.done" && echo "READY" || echo "PENDING"`
4. **超时处理**：输出文件存在但 `.done` 不存在超过 60 秒 → 子 Agent 卡住，触发超时
5. **Orchestrator 写入规范**：先写 `.tmp` 临时文件，完成后 rename + touch `.done`

**experience_handoff.json 写入逻辑：**
- 记录本轮关键发现：confirmed_defects 的 endpoint 分布、驳回原因分类、新发现的高价值攻击策略
- 记录当前辩论机制状态：stage1/stage2 的 approve/reject 比例、Judge Quartet 一致率
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
  "debate_stats": {
    "stage1_approved": 0,
    "stage1_rejected": 0,
    "stage2_confirmed": 0,
    "stage2_rejected": 0,
    "judge_agreement_rate": 0.0
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

### Step 9: 汇总报告 + 强制容器清理
1. 生成 `summary.md` 汇总报告
2. **强制容器清理**：执行以下命令清理所有本次会话创建的 Docker 容器和网络：
   ```bash
   docker compose -f docker/{target}.yml down -v --remove-orphans
   docker network rm testvdb-net-${TESTVDB_SESSION_ID:-standalone} 2>/dev/null || true
   ```
3. 验证清理完成：`docker ps --filter "name=testvdb-{target}" --format "{{.Names}}"` 应无输出
4. 更新 `.session.lock` 的 status 为 `completed`

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

## 错误处理

### 分级策略
| 错误类型 | 重试次数 | 退避策略 | 失败后行为 |
|---------|---------|---------|-----------|
| Docker 启动 | 5 | 10s 递增 | **终止会话** |
| 脚本执行 | 5 | 3s 递增 | 跳过该脚本 |
| 文档抓取 | 5 | 5s 递增 | 跳过该端点 |
| LLM 格式不合法 | 5 | 即时 | 降级为低置信度标记 |

所有错误记录到 error_log.json → session 结束汇总到 session_metadata.json。

---

## PreCompact / PostCompact 上下文保护

### PreCompact
当 Claude Code 发出 PreCompact 信号时（上下文即将压缩）：
1. 保存 mine_state.json + coverage.json + 当前轮次辩论日志到磁盘
2. 标记 pipeline_state 为当前阶段
3. 记录 next_action 指向下一步

### PostCompact
上下文压缩后恢复时：
1. 从磁盘重新读取 mine_state.json + coverage.json + 辩论日志
2. 通过 reflection_context 恢复关键发现和下一轮策略
3. 从 pipeline_state.next_action 继续执行

---

## 进度可见性

### stdout 实时日志
每轮开始/结束、缺陷发现时即时输出到 stdout：
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
持久化状态文件，随时查看进度。

### Monitors（独立守护进程）
- Docker 崩溃监控：检测容器异常退出，自动触发恢复
- 结果目录监控：检测新缺陷文件生成，触发通知

---

## 多DB并行建议

本 Orchestrator 每次只处理一个 DB。如需同时挖掘多个 DB，用户应开多个终端窗口并行执行：
```bash
# Terminal 1
/testvdb:mine milvus v2.4.0
# Terminal 2
/testvdb:mine qdrant v1.13.0
```

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
  │     │     threat_model.json (attack priorities + cognitive blindspots + judge enhancements)
  │     │
  ├──▶ Knowledge Extractor ──▶ raw_knowledge.md
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
  ├──▶ Judge Quartet (分两阶段) ◀── execution_results[] + threat_model(judge_enhancements)
  │     Phase 1: judge-doc (文档契约验证)
  │     Phase 2: evidence │ novelty │ severity (读取 doc 结果后执行)
  │           │
  │           ▼
  │     confirmed_defects[] + debate_log_stage2.json + stage2_doc.json
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
├── raw_knowledge.md    # 原始知识
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
