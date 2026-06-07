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
| ❌ 自己判断缺陷有效性 | ✅ `Agent(subagent_type="testvdb:judge-*")` |
| ❌ 自己生成缺陷报告 | ✅ `Agent(subagent_type="testvdb:reporter")` |

**主进程只使用这些工具做编排工作：** `Read`(读文件), `Write`(写状态文件), `Bash`(验证产出), `Grep`(搜索), `Glob`(匹配), `Agent`(派发子Agent)。

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

## 执行流程（主进程 = 编排者）

详细 SOP 文档见 `agents/orchestrator.md`。以下是主进程必须执行的步骤：

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

### Step 3: 缓存检查
检查 `results/{target}/{version}/structured_contract.json` 是否存在且未过期（TTL 见 settings.json 的 `knowledge.cache_ttl_hours`，默认 168h）。如果缓存有效 → 跳到 Step 6。

### Step 4: 派 Knowledge Extractor（⛔ 禁止自己爬取文档）
```
Agent(
  subagent_type="testvdb:knowledge-extractor",
  description="提取 {target} {version} 文档知识",
  prompt="按照 agents/knowledge-extractor.md 规范，为 {target} {version} 提取 API 文档知识，产出 raw_knowledge.md。输入参数: target={target}, version={version}。将结果写入 results/{target}/{version}/raw_knowledge.md"
)
```
**等待完成后验证：** `ls -la results/{target}/{version}/raw_knowledge.md`

### Step 5: 派 Contract Formalizer（⛔ 禁止自己生成契约）
```
Agent(
  subagent_type="testvdb:contract-formalizer",
  description="形式化 {target} v{version} API 契约",
  prompt="按照 agents/contract-formalizer.md 规范，将 results/{target}/{version}/raw_knowledge.md 转换为 structured_contract.json。输入参数: target={target}, version={version}。将结果写入 results/{target}/{version}/structured_contract.json"
)
```
**等待完成后验证：** `ls -la results/{target}/{version}/structured_contract.json`

### Step 6: 合同门控检查
检查 `structured_contract.json` 的核心 CRUD 端点覆盖率 ≥ 90%。不通过 → 输出缺失端点 + 终止。

### Step 7: 初始化状态
- 生成 `session_id`: `{target}-{version_short}-{counter}`（sanitize: `[a-z0-9-]`，≤63字符）
- 创建 `results/{target}/{version}/` 目录
- 写入 `mine_state.json` 和 `.session.lock`

### Step 8: 挖掘循环（每轮）

每轮执行以下子步骤。timestamp 子目录在第一轮开始时创建。

#### 8a. 注入 reflection_context
第一轮：无。后续轮次：从上轮 `experience_handoff.json` 读取。

#### 8b. 并发出动 Attack Trio（⛔ 禁止自己写脚本）

**同时派发 3 个 Agent（并发，非顺序）：**
```
Agent(subagent_type="testvdb:attack-boundary", description="边界攻击 {target} v{version}",
  prompt="按照 agents/attack-boundary.md 规范，为 {target} v{version} 生成边界攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-state", description="状态攻击 {target} v{version}",
  prompt="按照 agents/attack-state.md 规范，为 {target} v{version} 生成状态攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-semantic", description="语义攻击 {target} v{version}",
  prompt="按照 agents/attack-semantic.md 规范，为 {target} v{version} 生成语义攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")
```

**等待全部完成后验证：**
```bash
find results/{target}/{version}/{timestamp} -name "*.py" -type f ! -path "*/mre/*" ! -name "_stage1*" ! -name "script_*" 2>/dev/null | wc -l
```
为 0 则报错终止。不为 0 则 collect 所有脚本进入 Stage 1 审查。

#### 8c. 辩论 Stage 1（主进程自行审查——这是编排工作，可自己做）

1. 收集三个 Agent 产出的脚本，按来源标记 boundary/state/semantic
2. 自动去重（endpoint + constraint_id + strategy）
3. 语法验证（`python -m py_compile`）
4. 约束存在性验证（constraint_id 在 contract 中存在）
5. 审查结果写入 `debate_logs/stage1.json`
6. 将通过审查的脚本复制到标准路径

#### 8d. 派 Docker Executor（⛔ 禁止自己运行脚本）
```
Agent(
  subagent_type="testvdb:docker-executor",
  description="执行 {target} v{version} 攻击脚本",
  prompt="按照 agents/docker-executor.md 规范，在 Docker 沙箱中执行攻击脚本。target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}, session_id={session_id}。⛔ 立即执行 Step 1 命令，不要分析、不要检查、不要读取脚本内容。脚本位于 SESSION_DIR 下的 boundary_scripts/、state_scripts/、scripts/ 子目录和 script_*.py 文件中。所有脚本已通过语法验证，无需再检查。"
)
```
**等待完成后验证：** `ls results/{target}/{version}/{timestamp}/output_*.log.done 2>/dev/null | wc -l`，为 0 则报错终止。

#### 8e. 辩论 Stage 2 — 派 Judge Quartet + Fallback

**阶段 1：先派 judge-doc**
```
Agent(subagent_type="testvdb:judge-doc", description="文档契约验证 {target}",
  prompt="按照 agents/judge-doc.md 规范，验证以下候选缺陷的文档引用有效性：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}")
```
**等待完成后验证：** `test -f "results/{target}/{version}/{timestamp}/debate_logs/stage2_doc.json.done" && echo "READY"`

**阶段 2：并发派其他 3 个 Judge + 超时 fallback**

先并发派发 3 个 Judge Agent：
```
Agent(subagent_type="testvdb:judge-evidence", description="证据审查 {target}",
  prompt="按照 agents/judge-evidence.md 规范，审查以下执行结果的证据可信度：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}")

Agent(subagent_type="testvdb:judge-novelty", description="新颖性审查 {target}",
  prompt="按照 agents/judge-novelty.md 规范，审查以下候选缺陷的新颖性：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}")

Agent(subagent_type="testvdb:judge-severity", description="严重性评估 {target}",
  prompt="按照 agents/judge-severity.md 规范，评估以下候选缺陷的严重程度：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}")
```

**等待全部完成后验证产出：**
```bash
echo "doc: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_doc.json.done && echo 1 || echo 0)"
echo "evidence: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_evidence.json.done && echo 1 || echo 0)"
echo "novelty: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_novelty.json.done && echo 1 || echo 0)"
echo "severity: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_severity.json.done && echo 1 || echo 0)"
```

**Fallback 机制：如果任一 Judge 的 .done 不存在，主进程基于执行日志自动生成默认评估文件（编排容错，非替代 Judge）：**

- **evidence fallback**：读取缺陷列表+执行日志，基于 FAILED/PASSED 行判定 is_defect/not_defect，写 stage2_evidence.json+.done
- **severity fallback**：基于 defect_type 映射（Type1→High, Type2→Medium, Type3/4→High），写 stage2_severity.json+.done
- **novelty fallback**：全部标记为 unknown, vote=is_defect，写 stage2_novelty.json+.done

**投票逻辑和缺陷确认规则见 `agents/orchestrator.md` Step 8e。**

#### 8f. 派 Reporter（⛔ 禁止自己生成报告）
```
Agent(
  subagent_type="testvdb:reporter",
  description="生成缺陷报告 {target}",
  prompt="按照 agents/reporter.md 规范，为以下确认的缺陷生成报告：{confirmed_defects}。session_id={session_id}, target={target}, version={version}, session_dir=results/{target}/{version}/{timestamp}"
)
```
**等待完成后验证：** `ls results/{target}/{version}/{timestamp}/defects/defect-*.md 2>/dev/null | wc -l`

#### 8g-8i: 保存状态、分析产出、检查终止条件
主进程自行完成：保存 `mine_state.json` + `coverage.json` + `experience_handoff.json`，分析本抡产出，检查终止条件（连续5轮无新缺陷 / 覆盖率≥95% / max_rounds 达到 / min_defects 达到）。

#### 8j: 轮次间容器管理
继续下一轮 → `docker restart`。终止 → `docker compose down -v`。

### Step 9: 生成汇总 + 清理
- 生成 `summary.md`
- 清理 Docker 容器和网络
- 更新 `.session.lock` status 为 `completed`

### Step 10: 标记完成

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
├── summary.md
├── debate_logs/stage1.json
├── debate_logs/stage2.json
├── structured_contract.json
├── mine_state.json
├── coverage.json
├── experience_handoff.json
└── session_metadata.json
```

## Error Recovery

重新运行相同命令可恢复中断的会话。系统自动检测未完成会话。

## Multi-DB Mining

```bash
# Terminal 1
/testvdb:mine milvus v2.4.0
# Terminal 2
/testvdb:mine qdrant v1.13.0
```
