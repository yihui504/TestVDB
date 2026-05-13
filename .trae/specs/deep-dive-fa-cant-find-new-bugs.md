# Deep Dive Spec: FA 自动 Oracle — 从边界值探测器到状态机错误发现引擎

## Goal

让 FA 能自主发现状态机错误和语义不一致类缺陷，不再完全依赖 LLM 自报 `[DEFECT:...]` 标记。核心机制：从 contract 自动推导不变量，在每次测试脚本执行后自动检查，结果注入 LLM 上下文引导后续探索。

## Constraints (Red Lines)

1. **验证管线不变**：双重复现 + 独立审查流程保持原样
2. **分类方法不变**：DefectType 枚举保持兼容，Oracle 发现映射到现有类型
3. **沙箱隔离不变**：Oracle 在同一沙箱内运行，不引入新的安全风险
4. **双 Agent 分离不变**：KA 和 FA 的职责边界不变

## Non-Goals

- 不实现竞态条件检测（第二阶段）
- 不实现资源泄漏检测（第三阶段）
- 不修改 KA 的 contract 生成逻辑（本次仅扩展 schema，KA 填充是后续工作）
- 不替换 LLM 自报机制（Oracle 是补充，不是替代）

## Acceptance Criteria

### AC1: Contract Schema 扩展 — 不变量定义
- contract JSON schema 新增 `state_invariants` 字段
- 每个 invariant 包含：`name`、`check_type`（count_consistency / existence_check / value_range / idempotency）、`endpoint`、`precondition`、`assertion_script`
- 现有 contract 无 `state_invariants` 时系统正常降级（Oracle 不运行）

### AC2: Oracle 模块实现
- 新增 `src/agent/oracle.rs` 模块
- `Oracle::derive_invariants(contract) -> Vec<InvariantCheck>`：从 contract 的 `state_invariants` 和现有 `assertions` 推导可自动检查的不变量
- `Oracle::run_checks(sandbox_context, invariants) -> Vec<OracleFinding>`：在沙箱中执行不变量检查
- `OracleFinding` 结构：`{ invariant_name, violated: bool, evidence: String, defect_type: Option<DefectType> }`
- 单元测试覆盖 derive 和 run 逻辑

### AC3: FA 循环集成
- FA 每次执行 `execute_test_script` 后，自动运行 Oracle 检查
- Oracle 发现注入 LLM 上下文，格式：`=== ORACLE FINDINGS ===\n{findings_json}\n=== END ORACLE ===`
- LLM 看到 Oracle 发现后可聚焦探索（如 Oracle 发现 count 不一致，LLM 可深入测试该端点）
- Oracle 发现的缺陷自动记录到 ExplorationState（与 LLM 自报的缺陷并行追踪）

### AC4: 从现有 Assertions 自动推导不变量
- 即使 contract 没有 `state_invariants`，Oracle 也从现有 assertions 推导基础不变量：
  - `[RANGE] X must be > 0` → 推导 "发送 X=0 后检查是否被接受"
  - `[STATE] must create before search` → 推导 "不创建集合直接搜索，检查是否返回 200"
  - `[TYPE] X must be string` → 推导 "发送 X=123 后检查是否被接受"
- 这些推导的不变量与显式定义的 `state_invariants` 合并执行

### AC5: E2E 验证
- 在 Qdrant v1.18.0 上运行，Oracle 能自动发现至少 1 个 LLM 未自报的缺陷
- 现有安全网仍然正常工作
- `cargo check` 和 `cargo test` 全部通过

## Assumptions Exposed

1. **假设**：从 assertions 推导的不变量足够发现状态机错误
   - **风险**：推导可能不完整，遗漏关键不变量
   - **缓解**：AC4 是最小可行推导，显式 `state_invariants` 可补充

2. **假设**：Oracle 在同一沙箱中运行不会干扰 FA 的测试结果
   - **风险**：Oracle 的查询可能改变数据库状态
   - **缓解**：Oracle 只执行读操作（GET 请求），不修改状态

3. **假设**：LLM 能有效利用 Oracle 发现来引导探索
   - **风险**：LLM 可能忽略 Oracle 发现，继续按模板行事
   - **缓解**：Oracle 发现以高优先级注入上下文，prompt 中明确指示关注 Oracle 发现

## Technical Context

### 当前架构
```
FA Loop:
  LLM → execute_test_script → [stdout/stderr] → LLM → ...
                                    ↓
                              classifier (依赖 LLM 自报标记)
```

### 目标架构
```
FA Loop:
  LLM → execute_test_script → [stdout/stderr] → Oracle → [findings] → LLM → ...
                                    ↓                              ↓
                              classifier                  ExplorationState
                           (依赖 LLM 自报标记)         (新增 oracle_findings)
```

### Contract Schema 扩展
```json
{
  "endpoints": [...],
  "assertions": [...],
  "type_constraints": [...],
  "range_constraints": [...],
  "state_constraints": [...],
  "state_invariants": [
    {
      "name": "count_consistency_after_upsert",
      "check_type": "count_consistency",
      "endpoint": "/collections/{name}/points",
      "precondition": "collection exists with N points",
      "assertion_script": "assert count == N after upsert of M points"
    }
  ]
}
```

### Oracle Finding 注入格式
```
=== ORACLE FINDINGS ===
[
  {
    "invariant_name": "count_consistency_after_upsert",
    "violated": true,
    "evidence": "Inserted 5 points but count returned 3",
    "defect_type": "StateLogicViolation"
  }
]
=== END ORACLE ===

The Oracle detected a potential defect! Focus your next test on this finding to refine and confirm it.
```

## Ontology

| Entity | Definition |
|--------|-----------|
| InvariantCheck | 从 contract 推导的自动检查规则，包含检查脚本和预期结果 |
| OracleFinding | Oracle 检查结果，标记不变量是否被违反 |
| check_type | 不变量检查类型：count_consistency / existence_check / value_range / idempotency |
| 推导不变量 | 从现有 assertions 自动生成的不变量检查，无需显式定义 |
| 显式不变量 | contract 中 `state_invariants` 字段定义的不变量 |

## Ontology Convergence
- 初始：InvariantCheck 和 SafetyNet 概念重叠 → 澄清：SafetyNet 是兜底机制（FA 失败后运行），Oracle 是增强机制（FA 每轮运行）
- 初始：Oracle 和 Classifier 职责重叠 → 澄清：Classifier 判断 LLM 脚本输出，Oracle 主动检查系统状态

## Trace Findings

Trace 三通道调查发现：
1. **架构假设错配**（High confidence）：系统只能发现"单参数边界校验缺失"，对状态机错误/语义不一致架构上不支持
2. **信息不对称**（High confidence）：FA 只有 2 个工具和稀疏 contract，safety nets 拥有 FA 无法获取的先验知识
3. **LLM 认知瓶颈**（Medium-High confidence）：模板化 prompt 锚定 LLM 在教科书式测试模式

Oracle 机制同时缓解三个瓶颈：
- 架构层面：引入自动状态检查，不再完全依赖 LLM 自报
- 信息层面：Oracle 从 contract 推导知识，弥补 FA 的信息盲区
- 认知层面：Oracle 发现注入 LLM 上下文，引导 LLM 跳出模板框架

## Interview Transcript

1. Q: "FA 的核心目标是什么？" → A: "发现新类型缺陷"
2. Q: "哪些缺陷类型优先？" → A: "你来决定" → 决策：状态机错误 > 语义不一致 > 竞态条件 > 资源泄漏
3. Q: "Oracle 机制选择？" → A: "自动 Oracle"
4. Q: "不变量来源？" → A: "Contract 推导"
5. Q: "执行模型？" → A: "后置检查（集成到 FA 循环）"
