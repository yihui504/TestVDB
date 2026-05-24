# TestVDB 修复执行计划

**版本**: v3 (共识达成版)
**日期**: 2026-05-24
**共识流程**: Planner → Architect(R1/R2/R3) → Critic(R1 ITERATE/R2 ITERATE/R3 APPROVE)
**方案**: B — 分层验证架构

---

## RALPLAN-DR 摘要

### 原则（5条）

1. **语义验证优先**：probe脚本必须验证参数是否真正生效，而非仅检查HTTP状态码
2. **结构化契约**：契约参数必须用结构化类型替代点分路径，从类型系统层面消除语义混淆
3. **渐进式修复**：每个阶段独立可验证，前一阶段通过后才进入下一阶段
4. **最小侵入**：只修改必须修改的代码，不重构无关模块
5. **可返工性**：每个阶段有明确的验收标准和返工触发条件

### 决策驱动因素（Top 3）

1. **假阳性消除率**：修复后boundary运行的假阳性率应从42%降至<15%
2. **回归安全性**：所有现有测试必须继续通过，不能引入新的破坏性变更
3. **架构可维护性**：契约格式重构后，新增endpoint/参数不应需要修改probe.rs中的硬编码逻辑

### 方案选择

| 方案 | 描述 | 优点 | 缺点 | 选择 |
|------|------|------|------|------|
| A | 渐进式修复（先probe语义验证，后契约重构） | 立即降低假阳性率 | 修改probe.rs核心逻辑，违反最小侵入 | ✗ |
| **B** | **分层验证架构** | **probe零修改，独立SemanticGate模块** | **SemanticGate需额外实现** | **✓** |
| C | 一次性全面重写 | 完整解决方案 | 最高风险，最难验证 | ✗ |

**选择理由**：方案B在"最小侵入"和"语义验证优先"两项核心原则上优于方案A。probe.rs的ILLEGAL_SUCCESS判定是纯句法黑盒测试，在其中注入语义判断会破坏其确定性本质。SemanticGate作为独立可选层，不修改probe逻辑，风险可控。

---

## 全局策略

### "不确定"状态决策策略

安全测试场景下，"不确定"默认视为"潜在缺陷"（宁可假阳性不可假阴性）。Ambiguous结果不导致降级，只标注置信度。

### Ambiguous下游传播规则

| 组合 | 处理 |
|------|------|
| Ambiguous + repro_1通过 | 正常进入repro_2（当前行为不变） |
| Ambiguous + repro_1失败 | 报告中标注"低置信度"，仍走LLM修复路径 |
| ConfirmedIgnored + repro_1通过 | 直接拒绝（SemanticGate确认参数被忽略，非缺陷） |
| ActuallyApplied + repro_1通过 | 降级为"设计行为"标注（参数生效但可能不是bug） |

### 假阳性率双指标体系

- **严格假阳性率** = ConfirmedIgnored数 / (ConfirmedIgnored + ActuallyApplied数)，排除Ambiguous → 用于AC1阈值判定
- **广义假阳性率** = (假阳性数 + Ambiguous中实际非缺陷数) / 总ILLEGAL_SUCCESS数 → 作为补充监控视角
- 广义假阳性率 > 50%时触发"分类器校准建议"产出（非返工）

### 跨目标可达性分级

| 目标 | 可达性 | SemanticGate实现 |
|------|--------|-----------------|
| Qdrant | 可达 | create_collection后等待green状态，GET /collections/{name}对比配置 |
| Milvus | 部分可达 | describe_collection回读部分配置，无法回读的参数返回Ambiguous |
| Weaviate | 不可达 | 返回Ambiguous，降级到纯ILLEGAL_SUCCESS模式 |
| PgVector | 不可达 | 返回Ambiguous，降级到纯ILLEGAL_SUCCESS模式 |

### 警告区间

AC与RT之间的灰色地带定义为"警告区间"——不触发返工，但需产出根因分析报告（含至少1条可操作改进建议）。

---

## Phase 0 (前置): 基线度量

### 目标

建立假阳性率和真阳性召回率的基线数据，构建golden set。

### 实现

1. 用当前代码对Qdrant运行完整boundary策略
2. 人工标注每个ILLEGAL_SUCCESS结果为真阳性/假阳性
3. 构建golden set：从标注结果中选取真阳性样本
4. 双人标注校验

### 验收标准

| ID | 标准 | 量化指标 |
|----|------|---------|
| AC1 | 产出基线度量报告 | 包含假阳性率、真阳性数、repro_1通过率 |
| AC2 | 基线数据经人工审查确认 | 审查通过 |
| AC3 | golden set经双人标注校验 | Cohen's Kappa >= 0.8 |
| AC4 | golden set规模达标 | 总样本 >= 30，覆盖 >= 5种参数类型，每种 >= 3个 |

### 返工触发条件

无（这是度量步骤）

### 产出物

- 基线度量报告（假阳性率、真阳性数、repro_1通过率、verification执行时间）
- Golden set（双人标注校验通过的真阳性案例集）
- 若基线显示合同约束不足，触发A1(BFS爬取)作为补充

---

## Phase 1a (P0): SemanticGate — 消除L3根因 ✅ COMPLETED

### 目标

在verification层插入独立的参数生效检查模块，不修改probe.rs。

### 实现

1. 新增 `src/report/semantic_gate.rs` 模块
2. 定义独立trait `SemanticGateProvider`（不修改TargetPlugin）：
   ```rust
   #[async_trait]
   pub trait SemanticGateProvider: Send + Sync {
       fn target_name(&self) -> &str;
       async fn check_param_effect(
           &self,
           sandbox: &Sandbox,
           port: u16,
           mre_code: &str,
           defect_type: &DefectType,
       ) -> ParamEffect;
   }

   pub enum ParamEffect {
       ConfirmedIgnored,   // 服务器接受参数但未生效
       Ambiguous,          // 无法确定
       ActuallyApplied,    // 参数确实生效了
   }
   ```
3. Qdrant实现：create_collection后等待collection状态变为green，再GET /collections/{name}，对比请求值与实际配置
4. Milvus实现：用describe_collection回读部分配置，无法回读的参数返回Ambiguous
5. Weaviate/PgVector：返回Ambiguous
6. 在 `verification.rs` 的 `verify_candidate_defect` 中，repro_1之前插入语义门检查

### 验收标准

| ID | 标准 | 量化指标 |
|----|------|---------|
| AC1 | 严格假阳性率从基线降至 | < 15% |
| AC2 | 对golden set的召回率 | >= 90% |
| AC3 | Qdrant目标的不确定率 | < 15% |
| AC4 | 所有现有测试通过 | probe.rs零修改 |
| AC5 | SemanticGate为可选模块 | 禁用后系统行为与当前完全一致 |

### 警告区间

| 指标 | 警告区间 | 行动 |
|------|---------|------|
| 召回率 | 85% - 90% | 产出根因分析（含>=1条可操作改进建议），优化但不回退 |

### 返工触发条件

| ID | 条件 | 行动 |
|----|------|------|
| RT1 | 召回率 < 85% | 禁用SemanticGate，回退到纯ILLEGAL_SUCCESS模式 |
| RT2 | Qdrant不确定率 > 30% | SemanticGate判定逻辑重新设计（增加等待时间或重试） |
| RT3 | 任何现有测试失败 | 修复回归后再继续 |
| RT4 | 广义假阳性率 > 50% | 产出分类器校准建议 |

### 度量时机

Phase 1a完成后对Qdrant运行完整boundary策略，产出度量报告。

---

## Phase 1b (P0): MRE有效性门 + Classifier扩展 — 消除L1+L2 ✅ COMPLETED

### 目标

扩展classifier支持PARAM_IGNORED子类型，增加MRE语义有效性检查。

### 实现

1. 扩展 `classifier.rs` 的 `detect_defect_type` 支持 PARAM_IGNORED 子类型
2. 更新 `defect_signal_to_type()` 映射和 `tools.rs` 中的分类描述
3. 在 `FuzzTestCase` 中增加可选的 `semantic_assertion` 字段（非破坏性变更）
4. 在verification流程中增加MRE退出码与DEFECT标记一致性检查

### 验收标准

| ID | 标准 | 量化指标 |
|----|------|---------|
| AC1 | repro_1通过率 | >= 80% |
| AC2 | 假阳性在验证阶段被拦截率 | >= 80% |
| AC3 | 真阳性在验证阶段被误杀率 | < 10% |
| AC4 | verification执行时间增加 | < 20% |

### 警告区间

| 指标 | 警告区间 | 行动 |
|------|---------|------|
| repro_1通过率 | 60% - 80% | 调查根因并局部修复 |

### 返工触发条件

| ID | 条件 | 行动 |
|----|------|------|
| RT1 | repro_1通过率 < 60% | 回退到无MRE门 |
| RT2 | 真阳性误杀率 > 15% | 调整语义断言阈值 |
| RT3 | verification执行时间增加 > 50% | 优化语义检查逻辑 |
| RT4 | 任何现有测试失败 | 修复回归后再继续 |

### 度量时机

Phase 1b完成后对Qdrant运行完整boundary策略，产出度量报告。

---

## Phase 2 (P1): 外部验证机制 — 消除L6 ✅ COMPLETED

### 目标

在LLM分析结果后增加确定性校验步骤，打破自引用确认偏差闭环。

### 实现

1. 在 `contract_loader.rs` 的LLM提取后，用OpenAPI spec做交叉验证
2. 矛盾判定更严格：只有当OpenAPI spec明确声明了与LLM矛盾的约束时才算冲突，OpenAPI spec缺失约束不算冲突
3. 冲突处理：保留LLM结果但标注低置信度，不自动拒绝

### 验收标准

| ID | 标准 | 量化指标 |
|----|------|---------|
| AC1 | LLM提取的约束与OpenAPI spec的冲突检测率 | >= 90%（对已知冲突案例） |
| AC2 | 误报冲突率 | < 20% |
| AC3 | 不自动拒绝任何LLM提取的约束 | 只标注置信度 |

### 返工触发条件

| ID | 条件 | 行动 |
|----|------|------|
| RT1 | 误报冲突率 > 40% | 交叉验证逻辑重新设计 |
| RT2 | 冲突检测率 < 70% | 扩展冲突判定规则 |
| RT3 | LLM提取约束总数下降 > 30% | 交叉验证逻辑过度抑制，放宽矛盾判定 |

### 度量时机

Phase 2完成后对Qdrant运行contract提取，与OpenAPI spec对比。

---

## Phase 3 (P2): 契约格式渐进改善 — 消除L5 ✅ COMPLETED

### 目标

用工具函数渐进式改善契约格式，不修改序列化格式。

### 实现

1. 在 `ContractStore` 中增加 `parse_param_name()` 工具函数
2. `boundary.rs` 和 `probe.rs` 在需要时调用 `parse_param_name()` 获取结构化信息
3. 保留 `param_name: String` 的序列化格式，不修改OpenAPI管道
4. 逐步将 `strip_endpoint_prefix` 和 `dot_to_nested_json` 的调用点替换为 `parse_param_name()`

### 验收标准

| ID | 标准 | 量化指标 |
|----|------|---------|
| AC1 | parse_param_name()对现有所有param_name的解析准确率 | = 100%（覆盖strip_endpoint_prefix已支持的6种前缀） |
| AC2 | 新增endpoint时不需要修改probe.rs中的硬编码前缀列表 | 无硬编码修改 |
| AC3 | 所有现有测试通过 | 测试全绿 |
| AC4 | 序列化格式不变 | contract_store.json可正常加载 |

### 返工触发条件

| ID | 条件 | 行动 |
|----|------|------|
| RT1 | 解析准确率 < 100% | 修复解析逻辑后再继续 |
| RT2 | 任何现有测试失败 | 回退到strip_endpoint_prefix方式 |

---

## Phase 4 (P3): 黄金测试语义化 — 消除L4

### 目标

将byte-for-byte断言改为语义等价性断言，允许probe判定逻辑改进。

### 实现

1. 提取脚本中的关键断言模式（如 `if r.status_code == 200`、`sys.exit(1)`），用正则匹配而非完整AST解析
2. 语义等价性定义：两个脚本包含相同的HTTP请求URL、相同的判定逻辑、相同的DEFECT标记
3. 增加端到端测试：对已知bug（如shard_number=-1），验证MRE脚本能正确检测

### 验收标准

| ID | 标准 | 量化指标 |
|----|------|---------|
| AC1 | 修改probe判定逻辑时不需要更新golden输出 | 无golden更新 |
| AC2 | 语义等价性断言的假阳性率 | < 5% |
| AC3 | 语义等价性断言的假阴性率 | = 0% |
| AC4 | 所有现有测试通过 | 测试全绿 |

### 返工触发条件

| ID | 条件 | 行动 |
|----|------|------|
| RT1 | 语义等价性断言的假阴性率 > 0% | 回退到byte-for-byte断言 |
| RT2 | 正则匹配失败率 > 10% | 简化语义等价性定义 |
| RT3 | 语义等价性测试运行时间增加 > 50% | 简化正则匹配逻辑 |

---

## Phase依赖关系图

```
Phase 0 (基线度量)
  │
  ├── golden set ──→ Phase 1a (SemanticGate)
  │                     │
  │                     ├── ParamEffect枚举 ──→ Phase 1b (MRE有效性门)
  │                     │                        │
  │                     │                        └── PARAM_IGNORED ──→ Phase 2 (外部验证)
  │                     │                                              │
  │                     └── parse_param_name() ──→ Phase 3 (契约格式) ──→ Phase 4 (黄金测试)
  │
  └── 基线度量报告 ──→ 所有Phase的度量对比基准
```

---

## ADR: 分层验证架构

### Decision

采用方案B（分层验证架构），在verification层插入独立的SemanticGate模块，不修改probe.rs的判定逻辑。

### Drivers

1. probe.rs的ILLEGAL_SUCCESS判定是纯句法黑盒测试，注入语义判断会破坏其确定性本质
2. SemanticGate作为可选层，禁用后系统行为与当前完全一致，风险可控
3. 三值逻辑（ConfirmedIgnored/Ambiguous/ActuallyApplied）承认判定歧义的存在，比二值逻辑更安全

### Alternatives Considered

| 方案 | 否决理由 |
|------|---------|
| 方案A（修改probe判定逻辑） | 违反"最小侵入"原则，probe层不应承担语义判断 |
| 方案C（一次性全面重写） | 最高风险，最难验证，返工成本最大 |
| 直接修改TargetPlugin trait | 所有4个target实现都需要更新，侵入性过高 |
| 完整AST解析Python脚本 | 模板变量（如{{TESTVDB_DB_URL}}）无法直接解析 |

### Why Chosen

SemanticGate是唯一同时满足"语义验证优先"和"最小侵入"两项核心原则的方案。

### Consequences

- 正面：probe.rs零修改，golden测试不受影响，SemanticGate可选禁用
- 负面：SemanticGate增加verification执行时间（约20%），跨目标不对称（Milvus/Weaviate/PgVector无法完全验证）
- 后续：Phase 3的parse_param_name()可逐步替代strip_endpoint_prefix，最终消除L5

### Follow-ups

- SemanticGate的Qdrant实现需要处理异步optimizer的时序问题（等待green状态）
- Milvus的describe_collection API覆盖度需要实测确认
- Phase 1a完成后评估是否需要为Milvus实现部分SemanticGate
