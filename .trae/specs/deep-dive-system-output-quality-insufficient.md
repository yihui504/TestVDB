# Deep Dive Spec: 全链路改造 — 从输入验证测试器到行为逻辑缺陷发现引擎

## Goal

将 FA 系统从"边界值测试器"升级为"行为逻辑缺陷发现引擎"，使其能系统性发现状态不一致、语义错误、接口不一致、诊断质量等四类高价值缺陷，覆盖 Qdrant 全部 40+ 端点。

## Constraints (Red Lines)

1. **验证管线不变**：双重复现 + 独立审查流程保持原样
2. **分类方法兼容**：DefectType 枚举保持兼容，新增类型映射到现有体系
3. **沙箱隔离不变**：不引入新的安全风险
4. **现有 Safety Net 不删除**：保留作为兜底机制

## Non-Goals

- 不实现竞态条件检测（第二阶段）
- 不实现资源泄漏检测（第三阶段）
- 不替换 LLM 自报机制（行为合约检查是补充，不是替代）
- 不修改 Qdrant 本身

## Acceptance Criteria

### AC1: Contract Schema 扩展 — 行为合约定义

- `StructuredContract` 新增 `behavioral_contracts: Vec<BehavioralContract>` 字段，与现有 `state_invariants` 并存（兼容模式）
  - `StateInvariant` 保留为 Oracle 专用简化格式（现有代码不变）
  - `BehavioralContract` 是更丰富的扩展格式（新增）
  - 两者并存，Oracle 优先使用 `behavioral_contracts`，fallback 到 `state_invariants`
- `BehavioralContract` 包含四类（`BehaviorCategory` 枚举）：
  - `StateConsistency`：操作后状态验证（如 upsert N 点后 count==N）
  - `SemanticCorrectness`：输出语义验证（如搜索结果按 score 降序）
  - `InterfaceConsistency`：跨接口一致性验证（如 gRPC vs REST 同结果）
  - `DiagnosticQuality`：错误消息质量验证（如拒绝时提及参数名）
- 每个 BehavioralContract 包含：`name`、`category`（枚举四类）、`endpoints: Vec<String>`、`precondition_script: String`、`verification_script: String`、`expected_outcome: String`
- 现有 contract 无 `behavioral_contracts` 时系统正常降级（Oracle 不运行行为检查）
- `RangeConstraint` 的 `min`/`max` 字段从 `Option<String>` 改为 `Option<f64>`，KA 和后处理负责填充

### AC2: KA 提取扩展 — 行为合约提取

- **混合来源策略**：人工编写核心行为合约模板（保底）+ KA 自动提取（扩展）
- **人工模板**：为 Qdrant 核心 5 端点（search, create, upsert, delete, scroll）编写 20+ 条行为合约模板，存储在 `contracts/qdrant_behavioral_templates.json`
- **KA 扩展**：系统提示词新增 `[BEHAVIOR]` 前缀和四类子前缀：
  - `[BEHAVIOR:STATE]` — 状态一致性约束
  - `[BEHAVIOR:SEMANTIC]` — 语义正确性约束
  - `[BEHAVIOR:INTERFACE]` — 接口一致性约束
  - `[BEHAVIOR:DIAGNOSTIC]` — 诊断质量约束
- `submit_contract` 填充 `behavioral_contracts` 字段
- 后处理解析 `[BEHAVIOR:*]` 标签到对应结构化字段
- **端点扩展策略**：两步法
  1. **独立 OpenAPI 解析器**：新增 `src/contract/openapi.rs` 模块，直接解析 Qdrant OpenAPI JSON spec，自动生成 endpoint registry 和基础 contract（参数名、类型、约束）
  2. **KA 补充行为合约**：在解析器生成的基础 contract 上，KA 补充行为合约
  - 核心 5 端点（search, create, upsert, delete, scroll）人工编写详细行为合约模板
  - 其余端点由解析器自动生成基础信息，KA 按需补充

### AC3: Oracle 推导重构 — 结构化优先 + 关键词 fallback

- **结构化推导优先**：
  - `from_range_constraints` 优先消费 `RangeConstraint.min/max` 字段：`if min.is_some() { generate_boundary_check(param, min, endpoint) }`
  - 新增 `from_behavioral_contracts` 推导路径：从 `BehavioralContract.verification_script` 直接生成 Oracle 检查
  - **端点类型模板**：按端点类型（search/create/upsert/delete/scroll）定义通用脚本模板，用参数填充
    - search 模板：接受 `param_name`、`boundary_value`、`expected_rejection_code`，自动生成 setup→insert→search→verify 脚本
    - create 模板：接受 `param_name`、`boundary_value`，自动生成 create→verify 脚本
    - upsert 模板：接受验证类型（count/vector/payload），自动生成 setup→upsert→verify 脚本
    - 替换现有独立函数（`qdrant_search_probe`、`qdrant_create_size_probe` 等）
- **关键词 fallback**：保留现有 `from_assertions` 关键词匹配作为 fallback，当结构化字段为空时仍可工作
- **去重逻辑**：Safety Net / Oracle / Independent Reviewer 三层机制去重——同一缺陷场景只由最合适的层检查：
  - Oracle 优先检查从 contract 推导的场景
  - Independent Reviewer 检查 Oracle 未覆盖的独特场景（PoorDiagnostics、upsert 边界）
  - Safety Net 仅保留 Oracle 和 Reviewer 都不覆盖的场景（oversampling=0、empty_vector）

### AC4: FA 探索能力增强 — 单脚本多步操作

- **单脚本多步模型**：不跨轮复用沙箱，而是增强 LLM 在单个脚本内构建多步操作序列的能力
  - System prompt 新增行为合约测试模板，引导 LLM 编写自包含的多步脚本
  - 示例模板：
    ```
    STATE CONSISTENCY TEST:
    1. Setup: create collection, insert N points
    2. Action: perform operation (delete, update, etc.)
    3. Verify: check state matches expectation (count, vector content, etc.)
    4. Print [DEFECT: STATE_LOGIC_VIOLATION] if state diverges
    ```
- **Prompt 增强**：新增 4 种行为合约测试模板：
  - STATE CONSISTENCY：执行操作序列后验证状态
  - SEMANTIC CORRECTNESS：验证输出满足语义约束
  - INTERFACE CONSISTENCY：对比不同接口对同一操作的结果
  - DIAGNOSTIC QUALITY：验证错误消息包含有意义的信息
- **工具不变**：仍使用 `execute_test_script` + `submit_mre`，但 prompt 引导 LLM 在单脚本内构建更复杂的操作序列
- ExplorationState 新增 `tested_behaviors: HashSet<String>` 字段追踪已测试的行为合约类型

### AC5: Independent Reviewer 扩展 — 行为逻辑审查

- QdrantIndependentReviewer 新增行为逻辑探测场景：
  - 状态一致性：upsert→count、delete→count、update→vector 一致性
  - 语义正确性：search 排序验证、score_threshold 过滤验证、offset+limit 分页验证
  - 诊断质量：验证 400/422 响应体包含相关参数名（扩展 PoorDiagnostics 到更多端点）
- PoorDiagnostics 检查扩展：对每个被拒绝的请求，验证错误消息是否包含触发参数的名称

### AC6: E2E 验证

- 在 Qdrant v1.18.0 上运行，系统能发现至少 2 个行为逻辑类缺陷（非 IllegalSuccess）
- 现有输入验证缺陷发现能力不退化
- `cargo check` 和 `cargo test` 全部通过
- 端点覆盖率从 12% 提升到 80%+

## Assumptions Exposed

1. **假设**：LLM 能从文档中提取行为合约
   - **风险**：行为合约比输入约束更难从文档中提取，可能需要多轮交互
   - **缓解**：人工模板保底，KA 提取是扩展而非唯一来源

2. **假设**：单脚本多步操作足够发现行为逻辑缺陷
   - **风险**：某些缺陷需要跨轮次的状态累积才能触发
   - **缓解**：LLM 可以在单脚本内模拟任意复杂的操作序列，包括 setup→action→verify 模式

3. **假设**：行为合约的验证脚本可以自动生成
   - **风险**：复杂的行为验证（如 gRPC vs REST 对比）可能难以自动生成
   - **缓解**：人工模板提供核心验证脚本，Oracle 从结构化字段生成基础检查

4. **假设**：Qdrant v1.18.0 确实存在行为逻辑类缺陷
   - **风险**：如果 Qdrant 行为实现正确，系统无法发现此类缺陷
   - **缓解**：GitHub issue #8617、#9024、#7462 已证实此类缺陷存在

## Technical Context

### 当前架构（瓶颈分析）

```
Contract (仅输入约束) → Oracle (关键词硬编码推导) → FA (边界值测试器)
     ↓                        ↓                           ↓
  18 条断言              7 个硬编码分支              12 轮 × 单步测试
  2 个端点               Safety Net 重叠             沙箱每轮重启
```

### 目标架构

```
Contract (输入约束 + 行为合约) → Oracle (结构化推导) → FA (行为逻辑探索器)
     ↓                               ↓                        ↓
  输入约束 + 4 类行为合约      参数化推导 + 行为检查    单脚本多步 + 语义验证
  40+ 端点                    三层去重                  Prompt 引导
```

### 改造依赖图

```
AC1 (Schema) ──→ AC2 (KA + 模板) ──→ AC3 (Oracle) ──→ AC4 (FA Prompt) ──→ AC5 (Reviewer) ──→ AC6 (E2E)
     │                                    │                    │
     └── 端点注册表扩展 ──────────────────┘                    │
     └── RangeConstraint.min/max 类型改 f64 ──┘               │
                                                              │
              行为合约模板 (人工编写) ─────────────────────────┘
```

## Trace Findings

Trace 三通道调查发现：

1. **知识获取瓶颈**（High confidence）：Contract 100% 是输入约束，KA prompt 只要求 [TYPE]/[RANGE]/[STATE]，state_invariants 永远为空，整条管线从提取到后处理行为合约无处可入。

2. **Agent 探索局限**（High confidence）：沙箱每轮重启 + 仅 2 个工具 + 模板化 prompt = 边界值测试器。12 轮中有效探索窗口仅 7-8 轮。

3. **Oracle 推导脆弱性**（Medium-High confidence）：关键词硬编码查找表，RangeConstraint.min/max 从未被消费，核心参数 3-4 层冗余。

**Qdrant 真实缺陷分布**：149 个开放 bug issue 中，120 个关于 incorrect/inconsistent/corruption（行为逻辑类），5 个关于 validation/reject/accept（输入验证类）。当前系统只能发现后者的子集。

**根因**：单点依赖链的瓶颈效应——合约质量决定下游一切能力的上限。系统的能力上限被合约提取的下限所决定。

## Interview Transcript

1. Q: "优先解决哪个维度？" → A: "两者都要，但质量优先"
2. Q: "优先覆盖哪些行为合约类型？" → A: "全部四种（状态一致性、语义正确性、接口一致性、诊断质量）"
3. Q: "改造策略？" → A: "全链路改造（彻底）"，附加："开工前先推送代码保存状态"
4. Q: "端点范围？" → A: "全端点覆盖"
5. Q: "行为合约来源？" → A: "混合：模板保底 + KA 扩展"
6. Q: "沙箱复用模型？" → A: "单脚本多步（安全）"
7. Q: "端点扩展方式？" → A: "核心人工 + 其余自动"
8. Q: "Oracle 推导重构？" → A: "结构化优先 + 关键词 fallback"
9. Q: "BehavioralContract 和 StateInvariant 关系？" → A: "并存（兼容）"
10. Q: "参数化脚本生成？" → A: "端点类型模板（简洁）"
11. Q: "OpenAPI 提取？" → A: "你来决定" → 决策：解析器 + KA 两步

## Ontology

| Entity | Definition |
|--------|-----------|
| BehavioralContract | 行为合约定义，描述系统应满足的行为约束，包含验证脚本 |
| StateConsistency | 操作后状态验证类合约（如 upsert→count） |
| SemanticCorrectness | 输出语义验证类合约（如搜索排序正确性） |
| InterfaceConsistency | 跨接口一致性验证类合约（如 gRPC vs REST） |
| DiagnosticQuality | 错误消息质量验证类合约（如拒绝时提及参数名） |
| 结构化推导 | 从 contract 的结构化字段（min/max/precondition/verification_script）自动生成 Oracle 检查 |
| 单脚本多步 | LLM 在单个 Python 脚本内构建 setup→action→verify 多步操作序列，无需跨轮沙箱复用 |
| 行为合约模板 | 人工编写的行为合约定义，包含验证脚本，作为系统保底 |

## Ontology Convergence

- 初始：BehavioralContract 和 StateInvariant 概念重叠 → 澄清：两者并存，StateInvariant 保留为 Oracle 专用简化格式，BehavioralContract 是更丰富的扩展格式，Oracle 优先使用 behavioral_contracts，fallback 到 state_invariants
- 初始：沙箱复用和沙箱隔离矛盾 → 澄清：采用单脚本多步模型，不跨轮复用沙箱，在脚本内自包含多步操作
- 初始：人工模板和 KA 提取矛盾 → 澄清：人工模板是保底（保证最低质量），KA 提取是扩展（增加覆盖面），两者合并后去重
