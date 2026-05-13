# Deep Interview Spec: TestVDB FA Architecture Refactoring

## Metadata
- Interview ID: di-2026-05-12-fa-arch
- Rounds: 14
- Final Ambiguity Score: 3%
- Type: brownfield
- Generated: 2026-05-12
- Threshold: 20%
- Initial Context Summarized: no
- Status: PASSED

## Clarity Breakdown
| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|----------|
| Goal Clarity | 0.98 | 0.35 | 0.3430 |
| Constraint Clarity | 0.96 | 0.25 | 0.2400 |
| Success Criteria | 0.96 | 0.25 | 0.2400 |
| Context Clarity | 0.96 | 0.15 | 0.1440 |
| **Total Clarity** | | | **0.9670** |
| **Ambiguity** | | | **0.0330** |

## Goal

将 TestVDB 的 FA（Fuzzing Agent）系统从当前的硬编码单体架构重构为**插件化 + LLM 驱动认知**的架构，使扩展任何方向（新 target、新端点、新策略）都变得容易，同时让 FA 具备自适应策略调整能力。

核心原则：**好的架构不是预测未来，而是让未来容易发生**（Karpathy 规则）。

## Constraints

### 硬约束（红线，不可违反）
1. **验证管线不动**：双重复现 + 独立复核 + 提交级评审的确定性管线必须保留
2. **四型缺陷分类法不动**：IllegalSuccess / PoorDiagnostics / RuntimeFailure / StateLogicViolation
3. **沙箱隔离不动**：所有脚本必须在 Docker 沙箱中执行
4. **双 Agent 分离不动**：KA 和 FA 是不同的关注点，不能合并

### 软约束
5. **LLM 驱动认知**：FA 的认知能力通过结构化状态 + Prompt Engineering 实现，不加新算法
6. **Karpathy 规则**：简单性 > 复杂性，能用简单方案就不用复杂方案，代码应该一目了然
7. **不加注释**：遵循项目现有代码风格
8. **粗粒度拆分**：3 模块（FAOrchestrator / FAExecutor / TargetPlugin），不过度解耦
9. **最小接口**：TargetPlugin 只提供元信息 + 安全网 + Reviewer，不含策略提示
10. **每轮注入状态**：每轮 turn 在 user message 中注入 ExplorationState JSON，system prompt 保持静态
11. **两步实施**：AC3+AC1 一起做（拆解 + 插件化），然后 AC2（自适应策略）

## Non-Goals

- 不改变验证管线（双重复现 + 独立复核 + 提交级评审）
- 不改变四型缺陷分类法
- 不引入 ML/统计学习算法做 FA 认知
- 不引入向量存储或外部数据库做 FA 记忆
- 不合并 KA 和 FA
- 不在第一轮重构中支持新 target（Milvus/Weaviate），但架构必须使接入新 target 变得容易
- 不给 TargetPlugin 加策略提示方法（策略完全由 LLM 驱动）
- 不拆成 7 个细粒度模块（3 个粗粒度足够）

## Acceptance Criteria

### AC1: Target 插件化（可扩展性）
- [ ] 接入新 target 只需写一个实现 TargetPlugin trait 的插件模块 + 在入口注册
- [ ] TargetPlugin trait 只含 3 类方法：元信息（image/port/packages）、安全网（safety_nets）、Reviewer（create_reviewer）
- [ ] 不需要修改 FAOrchestrator、FAExecutor、classifier、sandbox 等核心模块
- [ ] 安全网探针从 FA loop 中解耦，成为 TargetPlugin 的一部分
- [ ] `match target.as_str()` 分支替换为 TargetPlugin 注册表查找

### AC2: FA 自适应策略（认知升级 MVP）
- [ ] FA 能根据执行结果自主判断"我应该换攻击方式"，不依赖硬编码的策略注入
- [ ] FA 接收全局探索视图（已测参数 + 结果 + 端点覆盖 + 策略效果统计），LLM 自行推断下一步
- [ ] 消除 `strategy_injections` 硬编码数组，替换为 LLM 驱动的策略推理
- [ ] 结构化状态在每轮 turn 结束时更新并注入下一轮 prompt
- [ ] 在 Qdrant 1.18.0 上 E2E 验证：FA 的探索效率不低于当前水平

### AC3: 巨型函数拆解（代码质量）
- [ ] `agentic_exploration_loop` 拆为 3 个模块：FAOrchestrator（循环控制 + LLM 调用）、FAExecutor（工具调用 + 断言追踪 + 错误状态机）、TargetPlugin（安全网 + 元信息 + Reviewer）
- [ ] 每个模块职责单一、可独立测试
- [ ] `cargo check` 通过，`cargo test` 全部通过
- [ ] 断言追踪、错误状态机、B2 协议等逻辑从 FA loop 中解耦到 FAExecutor

## Architecture Design

### 3-Module Decomposition

```
┌─────────────────────────────────────────────────────────┐
│                    FAOrchestrator                        │
│  - 循环控制 (turn 计数, B2 协议)                          │
│  - LLM 调用 + 消息管理                                   │
│  - 结构化状态注入 prompt                                  │
│  - 自适应策略触发（LLM 驱动，非硬编码）                     │
├─────────────────────────────────────────────────────────┤
│                    FAExecutor                            │
│  - 工具调用分发 (execute_test_script / submit_mre)        │
│  - 断言追踪 (keyword → structured results)               │
│  - 错误状态机 (consecutive_same_errors)                   │
│  - 执行结果 → 结构化状态更新                               │
├─────────────────────────────────────────────────────────┤
│                    TargetPlugin (trait)                  │
│  - target_image() / pip_packages() / db_port()          │
│  - safety_nets() -> Vec<SafetyNet>                      │
│  - create_reviewer() -> Box<dyn IndependentReviewer>    │
└─────────────────────────────────────────────────────────┘
```

### Structured State (全局探索视图)

```rust
struct ExplorationState {
    tested_params: Vec<ParamResult>,      // 已测参数 + 结果
    endpoint_coverage: Vec<EndpointCov>,  // 端点覆盖
    strategy_effectiveness: StrategyStats, // 策略效果统计
    consecutive_no_defect: usize,         // 连续无缺陷轮数
}

struct ParamResult {
    param_name: String,
    endpoint: String,
    result: TestResult,                   // Pass / Rejected / Defect
    defect_type: Option<DefectType>,
}

struct EndpointCov {
    endpoint: String,
    params_tested: usize,
    params_total: usize,
}

struct StrategyStats {
    total_tests: usize,
    defects_found: usize,
    rejections: usize,
    script_errors: usize,
}
```

### TargetPlugin Trait (最小接口)

```rust
trait TargetPlugin: Send + Sync {
    fn name(&self) -> &str;
    fn target_image(&self, version: &str) -> String;
    fn pip_packages(&self) -> Vec<&str>;
    fn db_port(&self) -> u16;
    fn safety_nets(&self) -> Vec<SafetyNet>;
    fn create_reviewer(&self) -> Option<Box<dyn IndependentReviewer>>;
}

struct SafetyNet {
    name: String,
    script: String,                       // 含 {{TESTVDB_DB_URL}} 占位符
}
```

### Adaptive Strategy Flow

```
Turn N:
  FAOrchestrator 构建 prompt:
    system_prompt (含契约, 静态)
    + user message: ExplorationState JSON + "Based on the exploration state above, decide your next action"
  → LLM 生成测试脚本
  → FAExecutor 执行 + 更新 ExplorationState
  → FAOrchestrator 检查是否需要干预 (B2 协议等)
  → 下一轮
```

### File Structure

```
src/
├── agent/
│   ├── orchestrator.rs   (FAOrchestrator - 循环控制 + LLM 调用 + 状态注入)
│   ├── executor.rs       (FAExecutor - 工具调用 + 断言追踪 + 错误状态机)
│   ├── state.rs          (ExplorationState + ParamResult + EndpointCov + StrategyStats)
│   ├── engine.rs         (KA loop, 不变)
│   ├── llm.rs            (LLM client, 不变)
│   ├── tools.rs          (工具定义, 不变)
│   └── classifier.rs     (分类器, 不变)
├── target/
│   ├── mod.rs            (TargetPlugin trait + SafetyNet struct)
│   └── qdrant.rs         (Qdrant 插件实现: 元信息 + 6安全网 + Reviewer)
├── contract/             (不变)
├── crawler/              (不变)
├── sandbox/              (不变)
├── report/               (不变)
└── review/               (不变)
```

### Implementation Phases

**Phase 1: AC3+AC1 (拆解 + 插件化)**
1. 创建 `src/target/mod.rs` — 定义 TargetPlugin trait + SafetyNet struct
2. 创建 `src/target/qdrant.rs` — 实现 Qdrant 插件（从 main.rs 迁移安全网 + 元信息 + Reviewer）
3. 创建 `src/agent/state.rs` — 定义 ExplorationState 及相关结构
4. 创建 `src/agent/executor.rs` — 从 main.rs 提取 FAExecutor（工具调用 + 断言追踪 + 错误状态机）
5. 创建 `src/agent/orchestrator.rs` — 从 main.rs 提取 FAOrchestrator（循环控制 + LLM 调用）
6. 重写 main.rs — 用新模块替代巨型函数，用 TargetPlugin 注册表替代 match 分支
7. 验证：cargo check + cargo test + E2E

**Phase 2: AC2 (自适应策略)**
1. 修改 FAOrchestrator — 每轮注入 ExplorationState JSON 到 user message
2. 修改 FAExecutor — 执行后更新 ExplorationState
3. 消除 strategy_injections 硬编码数组
4. 修改 system prompt — 移除硬编码策略提示，改为通用指导 + 状态驱动
5. 验证：cargo check + cargo test + E2E (Qdrant 1.18.0)

## Assumptions Exposed & Resolved
| Assumption | Challenge | Resolution |
|------------|-----------|------------|
| "更复杂的架构"意味着加更多功能 | Contrarian: 如果问题不是架构而是目标呢？ | 扩展性是核心问题，不是功能数量 |
| FA 需要跨 run 记忆 | Simplifier: 跨 run 记忆是否过度工程？ | 需要完整认知能力，但用 LLM 驱动而非算法驱动 |
| 需要算法做 FA 认知 | 混合 vs 纯 LLM | LLM 驱动：用结构化状态 + Prompt Engineering，不加新算法 |
| FA 认知 MVP 是"从拒绝推断" | 三个场景选哪个 | 自适应策略是 MVP——FA 自主判断换攻击方式 |
| 四条红线可能阻碍重构 | Contrarian: 如果红线不存在呢？ | 四条红线全部保留，重构范围是 FA 内部架构 |
| 拆成越多模块越好 | 3/5/7 模块选择 | 3 模块粗粒度——每个模块仍有足够上下文做正确决策 |
| TargetPlugin 需要策略提示 | 最小 vs 含策略 vs 含契约 | 最小接口——策略完全由 LLM 驱动，不需要 target 特定提示 |
| 结构化状态只需参数结果 | 参数列表 vs 全局视图 vs 完整上下文 | 全局探索视图——已测参数 + 结果 + 端点覆盖 + 策略效果统计 |
| 状态注入方式 | 每轮注入 vs 触发式 vs 动态 system prompt | 每轮注入状态——每轮 turn 在 user message 中注入 ExplorationState JSON |
| 实施顺序 | AC3→AC1→AC2 vs AC3+AC1→AC2 vs 全部一起 | AC3+AC1→AC2——拆解和插件化一起做，然后自适应策略 |
| 文件结构 | agent/ + target/ vs 全部 agent/ vs 最小变动 | agent/ + target/ 分层——最清晰的关注点分离 |

## Technical Context

### 当前架构问题
1. `agentic_exploration_loop` 是 ~400 行巨型函数，承担 7 种职责
2. 安全网探针（6 个 Python 脚本）硬编码在 FA loop 中
3. 策略注入（3 个 phase）硬编码为字符串数组
4. 断言追踪用 keyword 匹配的 HashSet，不可扩展
5. FA 没有跨 run 记忆，不会从正确拒绝中学习
6. Reviewer 只覆盖 Qdrant search 端点
7. 整条管线串行，无并行

### 关键代码位置
- FA loop: `src/main.rs:220-625`（`agentic_exploration_loop`）
- 安全网: `src/main.rs:232-291`（`SAFETY_NETS` const）
- 策略注入: `src/main.rs:358-376`（`strategy_injections` const）
- System prompt: `src/main.rs:295-343`
- 断言追踪: `src/main.rs:353,445-449`
- 错误状态机: `src/main.rs:350-351,476-496`
- B2 协议: `src/main.rs:382-411`
- Reviewer trait: `src/review/mod.rs`
- Qdrant reviewer: `src/review/qdrant.rs`
- Classifier: `src/agent/classifier.rs`
- Sandbox: `src/sandbox/manager.rs`
- Tools: `src/agent/tools.rs`

### 现有依赖
- Rust edition 2024
- DeepSeek API（OpenAI 兼容格式）
- Docker（沙箱隔离）
- 无外部 ML/向量数据库依赖

## Ontology (Key Entities)

| Entity | Type | Fields | Relationships |
|--------|------|--------|---------------|
| FAOrchestrator | core domain | turns, max_turns, messages, exploration_state | orchestrates FAExecutor, uses TargetPlugin |
| FAExecutor | core domain | assertion_tracker, error_state_machine | executes tools, updates ExplorationState |
| TargetPlugin | core domain | name, image, port, packages, safety_nets, reviewer | trait implemented per target |
| ExplorationState | core domain | tested_params, endpoint_coverage, strategy_stats | injected into LLM prompt by FAOrchestrator |
| SafetyNet | supporting | name, script | part of TargetPlugin |
| ParamResult | supporting | param_name, endpoint, result, defect_type | element of ExplorationState |
| StrategyStats | supporting | total_tests, defects_found, rejections, script_errors | element of ExplorationState |
| DefectDiscovery | supporting | classifier, verification_pipeline | unchanged |
| IndependentReviewer | supporting | trait | provided by TargetPlugin |

## Ontology Convergence
| Round | Entity Count | New | Changed | Stable | Stability Ratio |
|-------|-------------|-----|---------|--------|----------------|
| 1 | 3 | 3 | - | - | - |
| 2 | 5 | 2 | 0 | 3 | 100% |
| 3 | 5 | 0 | 0 | 5 | 100% |
| 4 | 6 | 1 | 0 | 5 | 83% |
| 5 | 7 | 1 | 0 | 6 | 86% |
| 6 | 8 | 1 | 0 | 7 | 88% |
| 7 | 9 | 1 | 0 | 8 | 89% |
| 8 | 10 | 1 | 0 | 9 | 90% |
| 9 | 10 | 0 | 2 (FALoop→FAOrchestrator+FAExecutor, StrategySystem→absorbed) | 8 | 100% |
| 10 | 10 | 0 | 1 (StructuredState→ExplorationState) | 9 | 100% |
| 11 | 9 | 0 | 0 | 9 | 100% |
| 12 | 9 | 0 | 0 | 9 | 100% |
| 13 | 9 | 0 | 0 | 9 | 100% |
| 14 | 9 | 0 | 0 | 9 | 100% |

## Interview Transcript

### Round 1
**Q:** 你说"设计更复杂的架构"，具体是指解决哪类问题？
**A:** 全面重构——同时解决拆解巨型函数、FA 认知升级、可扩展性三个方向，按优先级分阶段实施。
**Ambiguity:** 68% (Goal: 0.40, Constraints: 0.20, Criteria: 0.10, Context: 0.70)

### Round 2
**Q:** 重构完成后，什么会让你说"是的，这就是我想要的"？
**A:** 全部都要——发现新缺陷能力、新 target 接入成本低、代码可读性/可维护性。
**Ambiguity:** 60% (Goal: 0.50, Constraints: 0.20, Criteria: 0.30, Context: 0.70)

### Round 3
**Q:** 重构时，哪些现有设计是你绝对不想改变的？
**A:** 四条红线全部保留：验证管线不动、分类法不动、沙箱隔离不动、双 Agent 分离不动。
**Ambiguity:** 48% (Goal: 0.55, Constraints: 0.55, Criteria: 0.30, Context: 0.75)

### Round 4 (Contrarian Mode)
**Q:** 6 轮 E2E 只发现 1 个缺陷。你认为根本原因是什么？
**A:** 扩展性是核心问题——当前架构让扩展任何方面都很困难。重构的目标是让扩展变得容易，而不是预判哪个方向会成功。
**Ambiguity:** 39% (Goal: 0.70, Constraints: 0.55, Criteria: 0.45, Context: 0.80)

### Round 5
**Q:** 重构后，接入新 target 你期望的工作量是什么级别？
**A:** 1 个插件模块 + 注册——写一个实现 trait 的模块并在入口注册，不需要改核心模块。
**Ambiguity:** 32% (Goal: 0.75, Constraints: 0.60, Criteria: 0.60, Context: 0.80)

### Round 6 (Simplifier Mode)
**Q:** FA 的"认知升级"到底需要多强？
**A:** 需要完整认知能力——从正确拒绝中推断未测试攻击面、跨端点推断约束、自适应调整策略权重。
**Ambiguity:** 28% (Goal: 0.80, Constraints: 0.65, Criteria: 0.65, Context: 0.80)

### Round 7
**Q:** FA 的"完整认知能力"应该怎么实现？
**A:** LLM 驱动——用结构化状态 + Prompt Engineering 让 LLM 自己推理，不加新算法。
**Ambiguity:** 23% (Goal: 0.85, Constraints: 0.75, Criteria: 0.65, Context: 0.85)

### Round 8
**Q:** FA 认知升级的 MVP 验收标准是哪个场景？
**A:** 场景 3：自适应策略——FA 能自主判断"我应该换攻击方式"，而不是等硬编码注入。
**Ambiguity:** 18% (Goal: 0.88, Constraints: 0.80, Criteria: 0.75, Context: 0.85)

### Round 9
**Q:** FA loop 的 7 种职责应该拆成几个模块？
**A:** 3 模块粗粒度——FAOrchestrator（循环控制 + LLM 调用）、FAExecutor（工具调用 + 断言追踪 + 错误状态机）、TargetPlugin（安全网 + 元信息 + Reviewer）。
**Ambiguity:** 15% (Goal: 0.90, Constraints: 0.85, Criteria: 0.80, Context: 0.85)

### Round 10
**Q:** 注入 LLM 的"结构化状态"应该包含什么？
**A:** 全局探索视图——已测参数 + 结果 + 端点覆盖 + 策略效果统计。LLM 能看到全局，自行推断下一步。
**Ambiguity:** 11% (Goal: 0.92, Constraints: 0.88, Criteria: 0.85, Context: 0.88)

### Round 11
**Q:** TargetPlugin trait 应该包含哪些方法？
**A:** 最小接口——只提供元信息（image/port/packages）+ 安全网 + Reviewer。策略完全由 LLM 驱动，不需要 target 特定策略提示。
**Ambiguity:** 8% (Goal: 0.95, Constraints: 0.92, Criteria: 0.90, Context: 0.90)

### Round 12
**Q:** 自适应策略的 ExplorationState 应该怎么注入 LLM？
**A:** 每轮注入状态——每轮 turn 在 user message 中注入 ExplorationState JSON + "Based on the state, decide your next action"。System prompt 保持静态。
**Ambiguity:** 6% (Goal: 0.96, Constraints: 0.94, Criteria: 0.93, Context: 0.92)

### Round 13
**Q:** 三个 AC 的实施顺序应该怎么排？
**A:** AC3+AC1 → AC2——拆解巨型函数和引入 TargetPlugin 一起做（它们是拆解的直接产物），然后单独实现自适应策略。两步完成。
**Ambiguity:** 5% (Goal: 0.97, Constraints: 0.95, Criteria: 0.95, Context: 0.93)

### Round 14
**Q:** 新模块应该放在哪个目录？
**A:** agent/ + target/ 分层——FA 模块在 src/agent/（orchestrator.rs, executor.rs, state.rs），TargetPlugin 在 src/target/（mod.rs, qdrant.rs）。
**Ambiguity:** 3% (Goal: 0.98, Constraints: 0.96, Criteria: 0.96, Context: 0.96)
