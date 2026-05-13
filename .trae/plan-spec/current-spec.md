# TestVDB FA Architecture Refactoring — Canonical Spec

## Goal

将 TestVDB 的 FA 系统从硬编码单体架构重构为**插件化 + LLM 驱动认知**架构，使扩展任何方向（新 target、新端点、新策略）都变得容易，同时让 FA 具备自适应策略调整能力。

核心原则：**好的架构不是预测未来，而是让未来容易发生**（Karpathy 规则）。

## Constraints

### 硬约束（红线，不可违反）
1. **验证管线不动**：双重复现 + 独立复核 + 提交级评审的确定性管线必须保留
2. **四型缺陷分类法不动**：IllegalSuccess / PoorDiagnostics / RuntimeFailure / StateLogicViolation
3. **沙箱隔离不动**：所有脚本必须在 Docker 沙箱中执行
4. **双 Agent 分离不动**：KA 和 FA 是不同的关注点，不能合并

### 软约束
5. **LLM 驱动认知**：FA 的认知能力通过结构化状态 + Prompt Engineering 实现，不加新算法
6. **Karpathy 规则**：简单性 > 复杂性，代码应该一目了然
7. **粗粒度拆分**：3 模块（FAOrchestrator / FAExecutor / TargetPlugin），不过度解耦
8. **最小接口**：TargetPlugin 只提供元信息 + 安全网 + Reviewer，不含策略提示
9. **每轮注入状态**：每轮 turn 在 user message 中注入 ExplorationState JSON，system prompt 保持静态

## Architecture

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
    tested_params: Vec<ParamResult>,
    endpoint_coverage: Vec<EndpointCov>,
    strategy_effectiveness: StrategyStats,
    consecutive_no_defect: usize,
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
│   ├── orchestrator.rs   (FAOrchestrator)
│   ├── executor.rs       (FAExecutor)
│   ├── state.rs          (ExplorationState)
│   ├── engine.rs         (KA loop, 不变)
│   ├── llm.rs            (LLM client, 不变)
│   ├── tools.rs          (工具定义, 不变)
│   └── classifier.rs     (分类器, 不变)
├── target/
│   ├── mod.rs            (TargetPlugin trait + SafetyNet struct)
│   └── qdrant.rs         (Qdrant 插件实现)
```

## Acceptance State

- AC3: 巨型函数拆解 — ✅ 完成
- AC1: Target 插件化 — ✅ 完成
- AC2: FA 自适应策略 — ✅ 完成
- E2E 验证 (Qdrant 1.18.0) — ⏳ 待验证

## Invariants

1. 可信度优先于性能，宁可漏报也不误报
2. 正式缺陷报告必须经过全新沙箱下的双重复现
3. 前端 Agent 负责发散探索，后端 Pipeline 负责收敛与验真
4. 治理文档以 `.trae/plan-spec/current-plan.md` 与 `.trae/plan-spec/current-spec.md` 为准

## Non-Goals

- 不改变验证管线、分类法、沙箱隔离、双 Agent 分离
- 不引入 ML/统计学习算法做 FA 认知
- 不引入向量存储或外部数据库做 FA 记忆
- 不在第一轮重构中支持新 target（但架构必须使接入新 target 变得容易）
- 不给 TargetPlugin 加策略提示方法
