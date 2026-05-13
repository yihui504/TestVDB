# TestVDB FA Architecture Refactoring — Implementation Plan

> **Spec:** `.trae/plan-spec/current-spec.md`
> **Status:** Phase 1+2 完成，Phase 3 待验证

## Goal

将 TestVDB 的 FA 系统从硬编码单体架构重构为**插件化 + LLM 驱动认知**架构。

## Implementation Phases

### Phase 1: AC3+AC1 — 拆解巨型函数 + Target 插件化 ✅

- [x] Step 1.1: 创建 TargetPlugin trait + SafetyNet struct (`src/target/mod.rs`)
- [x] Step 1.2: 实现 Qdrant TargetPlugin (`src/target/qdrant.rs`)
- [x] Step 1.3: 创建 ExplorationState 及相关结构 (`src/agent/state.rs`)
- [x] Step 1.4: 创建 FAExecutor (`src/agent/executor.rs`)
- [x] Step 1.5: 创建 FAOrchestrator (`src/agent/orchestrator.rs`)
- [x] Step 1.6: 重写 main.rs 用新模块替代巨型函数
- [x] Step 1.7: 验证 — cargo check ✅, cargo test 43 passed ✅

### Phase 2: AC2 — FA 自适应策略 ✅

- [x] Step 2.1: 每轮注入 ExplorationState JSON 到 user message
- [x] Step 2.2: FAExecutor 执行后更新 ExplorationState
- [x] Step 2.3: 消除 strategy_injections 硬编码数组
- [x] Step 2.4: 修改 system prompt 为通用指导 + 状态驱动
- [x] Step 2.5: 验证 — cargo check ✅, cargo test 43 passed ✅

### Phase 3: QA + E2E 验证 (Qdrant 1.18.0) ⏳

- [ ] Step 3.1: 在 Qdrant 1.18.0 上运行 E2E
- [ ] Step 3.2: 确认 FA 探索效率不低于重构前水平
- [ ] Step 3.3: 记录结论到 handoff.md

## Acceptance Criteria

| AC | Status | Verification |
|----|--------|-------------|
| AC3: 巨型函数拆解 | ✅ | 3 模块独立可测试 |
| AC1: Target 插件化 | ✅ | TargetPlugin trait + QdrantPlugin + Registry |
| AC2: FA 自适应策略 | ✅ | ExplorationState 注入 + 无硬编码策略 |
| E2E 验证 | ⏳ | Qdrant 1.18.0 上 FA 效率不降 |

## Baseline

- `cargo check`: 通过 (1 既有 warning: `async_fn_in_trait`)
- `cargo test`: 43 passed, 0 failed, 1 ignored
