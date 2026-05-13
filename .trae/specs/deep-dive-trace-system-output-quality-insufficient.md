# Deep Dive Trace: system-output-quality-insufficient

## Observed Result

当前 FA 系统在 Qdrant v1.18.0 上仅发现 3-4 个缺陷（hnsw_ef=0、score_threshold 超界 x2、空 vectors config），且全部是 IllegalSuccess 类型（服务器静默接受非法输入）。缺陷数量少、类型单一、严重程度低。系统未能发现状态机错误、语义不一致、数据完整性等高价值缺陷。

## Ranked Hypotheses

| Rank | Hypothesis | Confidence | Evidence Strength | Why it leads |
|------|------------|------------|-------------------|--------------|
| 1 | 知识获取瓶颈：Contract 只捕获输入约束，行为合约基础设施存在但从未被激活 | High | Strong | 合约质量决定下游一切能力的上限 |
| 2 | Agent 探索能力局限：沙箱每轮重启 + 仅 2 个工具 + 模板化 prompt = 边界值测试器 | High | Strong | 即使有行为合约，Agent 也无法构建多步状态序列 |
| 3 | Oracle 推导脆弱性：关键词硬编码查找表 + Safety Net 重叠 + 验证管线过严 | Medium-High | Strong | Oracle 无法从新参数自动推导，且冗余严重 |

## Evidence Summary by Hypothesis

### Hypothesis 1: 知识获取瓶颈

**支持证据（Strong）**：
1. `qdrant_contract.json` 中 18 条断言 100% 是输入验证约束（TYPE/RANGE/STATE），无任何行为合约
2. KA 的系统 prompt 只要求提取 `[TYPE]`/`[RANGE]`/`[STATE]` 三种前缀，无 `[BEHAVIOR]`/`[OUTPUT]`/`[INVARIANT]`
3. `submit_contract` 永远将 `state_invariants` 设为空向量（engine.rs:293-301）
4. 后处理只解析输入约束标签（main.rs:422-465）
5. FA 编排器 prompt 只有 5 种输入验证测试模板
6. Safety Net 全部是输入验证探针（6 个）
7. 端点覆盖率约 12%（6/40+）
8. Schema 中多个 FUTURE 注释暴露未完成的行为合约基础设施

**反对证据（Moderate）**：
1. `StateInvariant`、`CheckType::CountConsistency/Idempotency` 数据结构存在
2. `DefectType::StateLogicViolation` 分类器存在
3. `qdrant_count_consistency_check` 是唯一的行为合约检查（关键词匹配副产物）
4. Oracle 的 `from_explicit_invariants` 方法可处理行为合约

### Hypothesis 2: Agent 探索能力局限

**支持证据（Strong）**：
1. 沙箱每次 `execute_test_script` 调用都重启（tools.rs:22-25），无法维持跨轮状态
2. 仅 2 个工具（execute_test_script + submit_mre），无状态查询/构建工具
3. 5 种测试模板全部围绕"发送异常值→检查拒绝"
4. 12 轮限制 + Turn 8/10/11 干预，有效探索窗口仅 7-8 轮
5. 断言追踪基于关键词字符串匹配（17 个硬编码关键词）
6. ExplorationState 缺少数据流/状态机/交互图谱

**反对证据（Moderate）**：
1. Oracle 系统提供独立于 FA 的状态验证能力
2. prompt 包含 STATE VIOLATION 和 COMBINATION VIOLATION 模板（但示例仍是单步操作）
3. 分类器识别 StateLogicViolation（但依赖 LLM 触发）
4. DeepSeek temperature=0.7 允许一定多样性

### Hypothesis 3: Oracle 推导脆弱性

**支持证据（Strong）**：
1. Oracle 推导使用 `param == "xxx" || desc.contains("xxx")` 硬编码分支，未使用 `min`/`max` 结构化字段
2. `RangeConstraint.min/max` 字段标注 FUTURE，从未被消费
3. hnsw_ef=0 被 4 层机制同时覆盖（Safety Net + Oracle range + Oracle assertion + Independent Reviewer）
4. 双重复现硬门 + 无中间状态，可能过滤非确定性缺陷
5. `from_state_constraints` 使用 `description.len()` 作为 name 区分符（潜在 bug）

**反对证据（Moderate）**：
1. Independent Reviewer 有独有覆盖（upsert 边界、PoorDiagnostics）
2. Oracle 框架管道是通用的（遍历 contract 字段），只是匹配规则硬编码
3. Oracle 有 5 个独有场景（count_consistency、NaN vector、duplicate collection、invalid distance、type constraints）

## Evidence Against / Missing Evidence

### Hypothesis 1:
- **缺失**：Qdrant 实际 issue tracker 中输入验证 vs 行为逻辑缺陷的比例
- **缺失**：LLM 是否具备从文档提取行为合约的能力（修改 prompt 后的可行性）
- **反证**：系统骨架支持行为合约，但从未被端到端验证

### Hypothesis 2:
- **缺失**：FA 实际运行中的缺陷类型分布（IllegalSuccess 占比？）
- **缺失**：LLM 在单脚本内构建多步操作的比例
- **反证**：Oracle 在同一沙箱内运行，理论上可以检测状态不一致

### Hypothesis 3:
- **缺失**：双重复现的实际失败率
- **缺失**：非硬编码参数的 contract 推导覆盖率
- **反证**：Independent Reviewer 的 PoorDiagnostics 维度是独特且高价值的

## Per-Lane Critical Unknowns

- **Lane 1 (知识获取瓶颈)**: Qdrant 的真实缺陷分布中，行为逻辑类缺陷占比多少？如果绝大多数缺陷确实是输入验证类，那么知识瓶颈的实际影响有限。
- **Lane 2 (Agent 探索局限)**: LLM 在单脚本内能否有效构建多步状态序列？如果可以，沙箱重启就不是致命限制——因为每个脚本都是自包含的。
- **Lane 3 (Oracle 推导脆弱性)**: 如果将 Oracle 推导从"关键词硬编码"改为"消费 min/max 结构化字段"，覆盖率能提升多少？现有 contract 中有多少 range_constraint 包含可解析的 min/max 值？

## Rebuttal Round

- **Best rebuttal to leader (H1)**: Agent 探索局限（H2）是更根本的原因。即使合约包含行为约束，Agent 也无法构建多步状态序列来触发它们。沙箱每轮重启是硬性架构限制。
- **Why leader held**: H1 更根本，因为 H2 的限制可以通过让 LLM 在单脚本内构建多步操作来部分绕过（脚本本身是自包含的），但 H1 的限制无法绕过——没有行为合约，就没有探索方向。
- **Convergence**: H1 和 H3 收敛于同一个机制——合约质量是 Oracle 推导的上限。H2 是独立的架构限制，但与 H1 互补而非矛盾。

## Convergence / Separation Notes

- **H1 ↔ H3 收敛**：合约只包含输入约束 → Oracle 只能推导输入验证检查 → Safety Net 也只覆盖输入验证 → 三层机制在同一个狭窄空间内冗余
- **H2 独立但互补**：即使解决 H1（添加行为合约），H2 的沙箱重启限制仍会阻碍 Agent 探索复杂状态序列。两个问题需要同时解决。
- **根因链**：Contract 质量 → Oracle 覆盖 → FA 探索方向 → 缺陷发现。这是一个单点依赖链，最弱环节是 Contract 质量。

## Most Likely Explanation

系统的产出不够多、质量不够好的根因是**单点依赖链的瓶颈效应**：

1. **合约提取管线只捕获输入约束**（根因）：KA 的 prompt、submit_contract 的空 state_invariants、后处理的标签解析，整条管线被设计为只处理输入验证。这是最上游的瓶颈。

2. **Oracle 推导是关键词硬编码查找表**（放大器）：即使合约包含更多约束，当前的 Oracle 推导也无法自动消费它们。`RangeConstraint.min/max` 字段从未被使用，`from_range_constraints` 只匹配预定义参数名。

3. **Agent 探索被架构限制在边界值测试**（放大器）：沙箱每轮重启 + 仅 2 个工具 + 模板化 prompt，使得 Agent 只能做"发送异常值→检查拒绝"的单步测试。即使 Oracle 发现了状态不一致，Agent 也难以深入探索。

4. **三层机制高度冗余而非互补**（效率损失）：Safety Net、Oracle、Independent Reviewer 在核心参数上 3-4 层重叠，但在行为逻辑维度几乎空白。

更精确的表述：**系统的能力上限被合约提取的下限所决定，而 Oracle 推导和 Agent 探索的架构限制进一步压缩了实际产出**。

## Critical Unknown

Qdrant 的真实缺陷分布中，行为逻辑类缺陷（状态不一致、语义错误、数据完整性）占比多少？如果 Qdrant 的绝大多数可发现缺陷确实是输入验证类，那么当前系统可能已经接近其能力天花板——问题不在于系统设计，而在于目标系统的缺陷分布。

## Recommended Discriminating Probe

**双管齐下**：

1. **注入行为合约探测**：手动构造包含 `state_invariants`（upsert 幂等性、搜索排序正确性、删除后计数一致性）的 contract JSON，运行完整管线。如果系统能发现行为类缺陷 → 问题在于合约提取；如果不能 → 问题延伸到检测管线。

2. **统计 Qdrant GitHub Issues**：爬取 Qdrant 的 GitHub issue tracker，分类统计 bug 类型（输入验证 vs 行为逻辑 vs 性能 vs 安全），验证"输入验证是主要缺陷来源"的假设是否成立。
