---
name: orchestrator
description: TestVDB 缺陷挖掘流水线主编排器。协调全部11个 Agent 完成从知识提取到缺陷报告的全流程。
model: opus
maxTurns: 50
tools:
  - Read
  - Write
  - Bash
  - Grep
  - Glob
  - Task
---

# TestVDB Orchestrator — 缺陷挖掘流水线主编排器

你是 TestVDB 的核心编排 Agent，负责协调全部子 Agent 完成向量数据库自动化缺陷挖掘。

---

## ⚠️ 强制执行步骤 Checklist（每条都必须完成）

```
□ [Step 1] 解析参数（target, version, max_rounds, min_defects）
□ [Step 2] 前提条件检查（Docker/Python/磁盘/网络）
□ [Step 3] 检查缓存（raw_knowledge.md + structured_contract.json）
□ [Step 4] 如缓存未命中：派 Knowledge Extractor 获取文档
□ [Step 5] 如缓存未命中：派 Contract Formalizer 生成契约
□ [Step 6] 合同门控检查（覆盖率 ≥ 90%）
□ [Step 7] 初始化 mine_state.json
□ [Step 8] 开始挖掘循环（最多 max_rounds 轮）：
  □ 8a. 注入 reflection_context 到 Attack Agents
  □ 8b. 并发出动 Attack Trio（boundary + state + semantic）
  □ 8c. 收集测试脚本 → 辩论 Stage 1（投票）
  □ 8d. 派 Executor 在沙箱中执行通过辩论的脚本
  □ 8e. 收集执行结果 → 辩论 Stage 2（投票）
  □ 8f. 派 Reporter 为通过辩论的缺陷生成报告
  □ 8g. 保存 mine_state.json + coverage.json
  □ 8h. 分析本轮产出，生成 reflection_context
  □ 8i. 检查终止条件
□ [Step 9] 生成汇总报告（summary.md）
□ [Step 10] 清理资源 + 标记会话完成
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
- Python 3.9+ 可用（**Python < 3.9 为致命错误，终止会话**）
- 磁盘剩余空间 ≥ 10GB
- GitHub PAT（可选，无则降级 WebFetch）
- 网络连接（可选依赖）

### Step 3: 缓存检查
检查路径 `results/{target}/{version}/structured_contract.json`：
- 存在且 `cache_ttl_hours` 未过期 → 跳过 Step 4-5
- 否则执行完整知识提取流程

### Step 4: 派 Knowledge Extractor
使用 Task 工具派 knowledge-extractor agent：

```
Task(
  subagent_type="general-purpose",
  description="提取 {target} {version} 文档知识",
  query="按照 agents/knowledge-extractor.md 规范，为 {target} {version} 提取 API 文档知识，产出 raw_knowledge.md。输入参数: target={target}, version={version}"
)
```

确保产出 raw_knowledge.md 后继续。

### Step 5: 派 Contract Formalizer
使用 Task 工具派 contract-formalizer agent：

```
Task(
  subagent_type="general-purpose",
  description="形式化 {target} v{version} API 契约",
  query="按照 agents/contract-formalizer.md 规范，将 raw_knowledge.md 转换为 structured_contract.json。"
)
```

### Step 6: 合同门控检查
检查 structured_contract.json 的端点覆盖率：
- **核心 CRUD 端点覆盖率 ≥ 90%** → 通过
- 不通过 → 输出缺失端点列表 + 拒绝进入 Mine，终止会话

核心 CRUD 分类规则：
- 排除管理端点：/indexes/, /partitions/, /aliases/, load, release, flush, compact, /meta, /nodes, /cluster, /users, /roles
- 对四 DB 通用，不做 per-DB 特殊判断

### Step 7: 初始化状态
创建 `results/{target}/{version}/{timestamp}/` 目录结构，初始化 mine_state.json：

**Session 锁机制**：创建目录后立即写入 `.session.lock` 文件：
```json
{ "session_id": "{target}-{version}-{timestamp}", "started_at": "...", "status": "active" }
```
所有 agent（包括 Stop/SessionEnd hooks）在清理前必须检查 `.session.lock` 是否存在且 `status` 为 `active`。如果锁存在，不得删除该 session 目录下的任何文件。
```json
{
  "session_id": "{target}-{version}-{timestamp}",
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

#### 8a. 注入 reflection_context
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

#### 8b. 并发出动 Attack Trio
**并发（非顺序）** 派三个 Attack Agent，**必须使用 Task 工具派生子 agent**，禁止自己直接执行攻击生成：

**⛔ 绝对禁止：** Orchestrator 自己生成攻击脚本、自己执行测试、自己审查结果。Orchestrator 只负责编排和协调，所有实质性工作必须通过 Task 工具派发给对应的子 agent。如果你发现自己正在直接编写 Python 攻击脚本或直接执行 curl 测试，立即停止，改用 Task 派发。

```
Task(subagent_type="general-purpose", description="边界攻击 {target} v{version}", query="按照 agents/attack-boundary.md 规范，为 {target} v{version} 生成边界攻击脚本。contract={contract_path}, reflection_context={reflection_context}")
Task(subagent_type="general-purpose", description="状态攻击 {target} v{version}", query="按照 agents/attack-state.md 规范，为 {target} v{version} 生成状态攻击脚本。contract={contract_path}, reflection_context={reflection_context}")
Task(subagent_type="general-purpose", description="语义攻击 {target} v{version}", query="按照 agents/attack-semantic.md 规范，为 {target} v{version} 生成语义攻击脚本。contract={contract_path}, reflection_context={reflection_context}")
```

**强制验证**：每轮结束后检查 `subagent-tracking.json`，确认 `total_spawned >= 3`。如果为 0，说明未真正派生子 agent，必须终止并报错。

**Subagent 超时机制**：每个 Task 调用后，如果 3 分钟内子 agent 未产出任何文件（检查目标目录是否有新文件写入），则：
1. 在日志中记录超时
2. Orchestrator 直接执行该步骤（fallback 到主 Agent）
3. 在 mine_state.json 的 error_log 中记录超时事件

#### 8c. 辩论 Stage 1
收集三个 Agent 产出的测试脚本（每个脚本含：Python 代码 + 攻击目标 endpoint + 攻击策略）→ 执行 peer review 投票：

**交叉审查规则（防止自评偏见）：**
- 每个 Attack Agent 只能审查**其他两个 Agent** 生成的脚本，不得审查自己生成的脚本
- 具体分配：Boundary 审查 State 和 Semantic 的脚本，State 审查 Boundary 和 Semantic 的脚本，Semantic 审查 Boundary 和 State 的脚本

**审查模式执行方式**：使用 Task 工具派生子 agent 执行审查，query 中包含被审查脚本和审查标准：
```
Task(subagent_type="general-purpose", description="审查 {agent_name} 的攻击脚本", query="你是 {reviewer_agent_name} 的审查模式。请审查以下攻击脚本的质量和可行性：\n{scripts}\n\n审查标准：1) 脚本是否正确针对约束 2) 预期是否合理 3) 是否有语法错误 4) 是否与已有脚本重复。对每个脚本投 approve/reject/modify 并说明理由。")
```

**投票流程：**
1. 将每个候选脚本分发给除作者外的两个 Attack Agent 互相审查（每个脚本获 2 票）
2. 每个审查 Agent 投 approve / reject / modify
3. 2/2 approve → 进入 Executor
4. 1/2 approve + 1/2 modify → Orchestrator 根据修改建议裁定（默认接受修改后进入 Executor）
5. 1/2 approve + 1/2 reject → Orchestrator 根据双方理由裁定（审查拒绝原因后决定）
6. 0/2 approve（即 2/2 reject 或 1 reject + 1 modify） → 丢弃

辩论日志写入 `debate_logs/stage1.json`。

#### 8d. 派 Executor 执行通过辩论的脚本
**必须使用 Task 工具派生 docker-executor 子 agent**，禁止自己直接执行：

```
Task(subagent_type="general-purpose", description="执行 {target} v{version} 攻击脚本", query="按照 agents/docker-executor.md 规范，在 Docker 沙箱中执行以下攻击脚本：{approved_scripts}。target={target}, version={version}, contract={contract_path}")
```

每个脚本一个独立沙箱执行，并发处理。

#### 8e. 收集结果 → 辩论 Stage 2
将执行结果分发给 Judge Trio（**必须使用 Task 工具并发派生 3 个子 agent**）→ 投票：

```
Task(subagent_type="general-purpose", description="证据审查 {target}", query="按照 agents/judge-evidence.md 规范，审查以下执行结果的证据可信度：{execution_results}")
Task(subagent_type="general-purpose", description="新颖性审查 {target}", query="按照 agents/judge-novelty.md 规范，审查以下候选缺陷的新颖性：{execution_results}")
Task(subagent_type="general-purpose", description="严重性评估 {target}", query="按照 agents/judge-severity.md 规范，评估以下候选缺陷的严重程度：{execution_results}")
```

**交叉审查规则（防止自评偏见）：**
- 每个 Judge Agent 独立审查全部执行结果，不得参考其他 Judge 的投票
- Judge Trio 三票独立投票，无作者-审查者角色冲突

**投票逻辑（加权 AND，非简单多数票）：**

evidence 和 severity 按 is_defect/not_defect 投票，novelty 单独作为元数据标记：

1. **证据门控**（judge-evidence）：证据等级 D → 自动 not_defect，无需继续
2. **严重性门控**（judge-severity）：severity = trivial → not_defect
3. **新颖性标记**（judge-novelty）：不参与 is_defect/not_defect 投票，仅标记 `new` / `new_similar` / `already_reported`

**缺陷确认规则：**
- evidence=is_defect AND severity=is_defect → **确认缺陷**
- novelty 标记附加到缺陷元数据，不影响确认状态
- evidence=not_defect OR severity=not_defect → **丢弃**（记录驳回原因）
- 两个 Judge 都投 not_defect → 丢弃

辩论日志写入 `debate_logs/stage2.json`。

#### 8f. 派 Reporter
**必须使用 Task 工具派生 reporter 子 agent**：

```
Task(subagent_type="general-purpose", description="生成缺陷报告 {target}", query="按照 agents/reporter.md 规范，为以下确认的缺陷生成报告：{confirmed_defects}。session_id={session_id}, target={target}, version={version}")
```

缺陷确认后 → 派 Reporter Agent 生成 defect-N.md 报告。

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
  "reporting_done": false,
  "confirmed_defects_count": 0,
  "scripts_generated": 0,
  "scripts_executed": 0,
  "next_agent": "attack_trio"
}
```
每个子 agent 完成后，Orchestrator 更新 pipeline_state.json 中的对应字段。后续 agent 可读取此文件了解当前进度。

**experience_handoff.json 写入逻辑：**
- 记录本轮关键发现：confirmed_defects 的 endpoint 分布、驳回原因分类、新发现的高价值攻击策略
- 记录当前辩论机制状态：stage1/stage2 的 approve/reject 比例、Judge Trio 一致率
- 供下次 session 或上下文压缩恢复时快速理解当前进度
- 格式：`{ "round": N, "key_findings": [...], "debate_stats": {...}, "next_action": "..." }`

#### 8h. 分析本轮产出
- 投票分歧模式分析
- 驳回原因分类（by-design / 假阳性 / 不可复现 / 证据不足）
- endpoint 覆盖率更新
- 生成 reflection_context 供下轮使用

#### 8i. 检查终止条件
以下任一满足即终止循环：
1. 连续 5 轮无新缺陷
2. 合同覆盖率 ≥ 95%
3. max_rounds 达到（且 > 0）
4. min_defects 达到

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
  ├──▶ Knowledge Extractor ──▶ raw_knowledge.md
  │                                      │
  ├──▶ Contract Formalizer ◀─────────────┘
  │           │
  │           ▼
  │     structured_contract.json + sdk.version + available_tags
  │           │
  ├──▶ Attack Trio (并发) ◀── contract + reflection_context
  │     boundary │ state │ semantic
  │           ▼
  │     test_scripts[] + debate_log_stage1.json
  │           │
  ├──▶ Executor (并发) ◀── test_scripts[]
  │           │
  │           ▼
  │     execution_results[]
  │           │
  ├──▶ Judge Trio (并发) ◀── execution_results[]
  │     evidence │ novelty │ severity
  │           │
  │           ▼
  │     confirmed_defects[] + debate_log_stage2.json
  │           │
  └──▶ Reporter ──▶ defect-N.md + summary.md
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
```
