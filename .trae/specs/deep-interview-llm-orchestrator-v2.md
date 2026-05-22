# Deep Interview Spec: LLM Orchestrator V2 架构改进

## Metadata
- Interview ID: llm-orchestrator-v2-2026-05-20
- Rounds: 11
- Final Ambiguity Score: 15%
- Type: brownfield
- Generated: 2026-05-20
- Updated: 2026-05-22
- Threshold: 0.2
- Status: ACTIVE — Phase 2 Deep Interview 完成（模糊度 13.3%）

## Clarity Breakdown
| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|----------|
| Goal Clarity | 0.95 | 0.35 | 0.3325 |
| Constraint Clarity | 0.90 | 0.25 | 0.2250 |
| Success Criteria | 0.85 | 0.25 | 0.2125 |
| Context Clarity | 0.90 | 0.15 | 0.1350 |
| **Total Clarity** | | | **0.905** |
| **Ambiguity** | | | **0.095** |

## Goal
让 LLM 在 TestVDB 中起四个关键作用：
1. **生成测试用例**：在确定性生成器基础上，通过创造性探索发现新缺陷（目标：每轮 ≥1 增量缺陷）— ✅ 验收A通过（V8/V18）
2. **分析缺陷根因**：自动分类缺陷根因（参数校验缺失/状态管理 bug/并发问题等，目标：准确率 ≥80%）— ✅ V23/V30 验证通过
3. **验证缺陷可复现性**：生成多维度验证脚本（跨参数组合、边界条件），目标：验证通过率 ≥90% — ✅ V30 验证通过（100%）
4. **生成缺陷报告**：根据缺陷类型自动选择最佳报告模板，目标：GitHub 接受率 ≥70% — ✅ V30 验证通过

### Phase 2 目标（Step 10-14）：从参数边界测试转向状态交互/并发测试

**核心问题**：当前 LLM 编排器产出不了增量 Bug — 发现的缺陷（nprobe=0、重复集合名）都是确定性生成器已覆盖的参数边界问题。

**根因**：3 层瓶颈
- 工具设计：`execute_api_sequence` 只生成线性串行脚本，无状态验证
- 不变量检查：只检查 `count<0`/`dimension<=0`，无法检测语义级缺陷
- Prompt 导向：10 种模式类别本质是参数边界变体

**Phase 2 目标**：让 LLM 编排器产出确定性生成器无法发现的增量 Bug（状态一致性、并发竞态、时序依赖）

| Step | 内容 | 新工具 | 目标 Bug 类型 |
|------|------|--------|-------------|
| 10 | 有状态模型测试 | `execute_stateful_test` | STATE_LOGIC_VIOLATION（rowCount 不一致、数据残留） |
| 11 | 并发竞态测试 | `execute_concurrent_test` | 并发计数不一致、幂等性违反 |
| 12 | 时序依赖测试 | `execute_timing_test` | flush→search 不可见、load→search 失败 |
| 13 | Prompt 重设计 + 语义不变量 | — | LLM 探索方向转向状态交互 |
| 14 | 实战验证 | — | 产出增量 Bug |

**验收标准**：至少 1 个确定性生成器无法发现的增量 Bug（STATE_LOGIC_VIOLATION 或并发竞态）

## Constraints
- 保持"确定性生成器 → LLM 编排器"串行流程
- 不限制 DeepSeek API 调用次数，优先保证产出质量
- 所有改进基于现有代码库（brownfield），不重构整体架构
- 最小可行验证先行：先改 system prompt 验证效果，再投入完整改进

## Non-Goals
- 不替代确定性生成器（boundary/mutation 等继续运行）
- 不引入并行执行（避免 Docker 资源竞争）
- 不切换 LLM 提供商（继续使用 DeepSeek）
- 不跨版本验证（聚焦 Milvus 2.6.16）

## Acceptance Criteria
- [x] 最小验证：修改 system prompt 强制工具使用顺序后，LLM 能在一次 mine 运行中使用 execute_api_sequence 或 compare_endpoints 至少 1 次
- [x] 增量缺陷：LLM 编排器每轮运行产出 ≥1 个确定性生成器未覆盖的缺陷（新 DefectKind 或新端点）— V8/V18 通过
- [x] 覆盖多样性：LLM 探索的 API 序列覆盖 ≥5 种不同状态转换模式 — V19 通过
- [x] 根因分析：LLM 能自动输出缺陷根因分类，准确率 ≥80% — V23/V30 通过
- [x] 验证通过率：LLM 生成的多维度验证脚本通过率 ≥90% — V30 通过（100%）
- [x] 报告质量：LLM 优化的缺陷报告 GitHub 接受率 ≥70% — V30 通过
- [ ] **Phase 2**: LLM 通过状态交互测试产出 ≥1 个确定性生成器无法发现的增量 Bug（STATE_LOGIC_VIOLATION 或并发竞态）

## Assumptions Exposed & Resolved
| Assumption | Challenge | Resolution |
|-----------|-----------|------------|
| LLM 0 增量是因为工具设计不好 | Contrarian 模式：可能是 Milvus 本身无此类 bug | 接受不确定性，先优化工具再验证 — ✅ V8 证实工具优化有效 |
| 需要保留全部 5 个工具 | Simplifier 模式：LLM 只用了 1 个 | 全部保留，但用组合策略强制使用顺序 — ✅ LLM 现在使用 execute_api_sequence |
| fresh_sandbox 每轮都需重建 | 智能状态检测可优化 | 检测数据库状态变化，无变化则复用 — ✅ 已实现 |
| 验证降级是 classifier 问题 | 实际根因是 initial_run stdout/stderr 为空 | V18 修复：ExecutionResult 新增 stdout/stderr 字段 |

## Technical Context

### 当前架构
- **确定性生成器**：9 类生成器（boundary/mutation/state/meta/seq/res/combo/diff/conc），零 LLM 参与
- **LLM 编排器**：FAOrchestrator，12 轮探索循环，5 个工具
- **验证流程**：verification_runner，确定性重跑 3 次
- **Safety Net**：205 个手写探针，turn 5/9 批量执行

### 关键文件
- `src/agent/orchestrator.rs`：LLM 编排器核心
- `src/agent/tools.rs`：工具定义
- `src/agent/executor.rs`：沙箱执行（新增 stdout/stderr 字段）
- `src/agent/classifier.rs`：缺陷分类（优先级已修复）
- `src/report/verification.rs`：验证流程
- `src/sandbox/manager.rs`：沙箱管理（python -u 模式）

### 当前阻塞点（已解决）
1. ~~**工具选择惯性**~~：✅ 代码层强制 turn<3 拒绝 execute_test_script
2. ~~**覆盖率追踪窄**~~：✅ execute_api_sequence/compare_endpoints 结果更新覆盖率
3. ~~**验证降级**~~：✅ initial_run stdout/stderr 修复
4. ~~**脚本 Traceback**~~：✅ api() 加 try/except，check_code 不再中断
5. ~~**收敛过早**~~：✅ 阈值 3→5，turn≥8 才检查

### 当前待解决
1. **LLM 探索范围窄**：倾向探索同一类缺陷（参数类型混淆），需要引导覆盖更多状态转换
2. **compare_endpoints 使用少**：LLM 很少选择此工具

## Ontology (Key Entities)

| Entity | Type | Fields | Relationships |
|--------|------|--------|---------------|
| Defect | core domain | type, endpoint, params, evidence, root_cause | discovered_by TestCase, verified_by Verification, reported_in Report |
| TestCase | core domain | generator, endpoint, params, script, result | discovers Defect, covers Endpoint |
| Endpoint | supporting | path, method, params, constraints | tested_by TestCase, has StateTransitions |
| StateTransition | supporting | from_state, action, to_state, invariant | part_of Endpoint, tested_by LLMSequence |
| LLMSequence | core domain | steps, invariant, result | generated_by LLM, tests StateTransitions |
| ContractStore | external system | type_constraints, range_constraints, observed_behaviors | feeds TestCase generators |
| Sandbox | external system | db_container, state, freshness | executes TestCase, managed_by Executor |

## Interview Transcript

### Round 1
**Q:** 你希望 LLM 在 TestVDB 的哪些具体环节起关键作用？
**A:** 生成测试用例（在确定性生成器之上）、分析缺陷根因、验证缺陷可复现性、生成缺陷报告
**Ambiguity:** 85% → 75%

### Round 2
**Q:** 确定性生成器和 LLM 的协同模式是什么？
**A:** 用户问"你推荐什么做法？" → 推荐串行增强 + 关键节点 LLM 介入
**Ambiguity:** 75% → 65%

### Round 3
**Q:** DeepSeek API 的预算限制？
**A:** 选择 A，不限制调用次数，优先保证产出质量
**Ambiguity:** 65% → 55%

### Round 4
**Q:** 四个关键作用的成功标准？
**A:** 全部接受（增量缺陷 ≥1、根因准确率 ≥80%、验证通过率 ≥90%、报告接受率 ≥70%），验证不用跨版本
**Ambiguity:** 55% → 45%

### Round 5 (Contrarian)
**Q:** LLM 0 增量是否因为 Milvus 本身无 bug？
**A:** 接受不确定性，先优化工具再验证
**Ambiguity:** 45% → 40%

### Round 6 (Simplifier)
**Q:** 如果只能保留 2 个工具？
**A:** 全部保留，但修改 system prompt 强制使用顺序
**Ambiguity:** 40% → 35%

### Round 7
**Q:** 哪种约束机制最有效？
**A:** 组合策略（Prompt 层级 + 代码层 + 工具描述优化）
**Ambiguity:** 35% → 30%

### Round 8
**Q:** 覆盖率追踪如何改进？
**A:** 三者结合（参数 + 端点 + 状态转换）
**Ambiguity:** 30% → 25%

### Round 9
**Q:** 沙箱管理如何优化？
**A:** 智能状态检测
**Ambiguity:** 25% → 20%

### Round 10
**Q:** Safety Net 协同如何改进？
**A:** LLM 生成 Safety Net
**Ambiguity:** 20% → 15%

### Round 11 (Ontologist)
**Q:** 核心实体是什么？
**A:** Defect（缺陷）
**Ambiguity:** 15% → 10%

### Round 12
**Q:** 先做最小验证还是完整改进？
**A:** 先做最小验证
**Ambiguity:** 10% → 9.5%

## Next Steps

### Phase 1（已完成）
1. ✅ **验收 B**：改进 LLM 探索多样性，覆盖 ≥5 种状态转换模式
2. ✅ **根因分析**：实现 LLM 自动分类缺陷根因
3. ✅ **验证增强**：LLM 生成多维度验证脚本
4. ✅ **报告优化**：LLM 审查优化缺陷报告

### Phase 2（Step 10-14）
1. ⬜ **Step 10**: 实现 `execute_stateful_test` 工具 — 有状态模型测试
2. ⬜ **Step 11**: 实现 `execute_concurrent_test` 工具 — 并发竞态测试
3. ⬜ **Step 12**: 实现 `execute_timing_test` 工具 — 时序依赖测试
4. ⬜ **Step 13**: Prompt 重设计 + 语义不变量增强
5. ⬜ **Step 14**: 实战验证 — 产出增量 Bug

---

## Phase 2 Deep Interview（2026-05-22）

### Phase 2 Clarity Breakdown
| Dimension | Score | Weight | Weighted |
|-----------|-------|--------|----------|
| Goal Clarity | 0.90 | 0.35 | 0.315 |
| Constraint Clarity | 0.85 | 0.25 | 0.213 |
| Success Criteria | 0.85 | 0.25 | 0.213 |
| Context Clarity | 0.85 | 0.15 | 0.128 |
| **Total Clarity** | | | **0.868** |
| **Ambiguity** | | | **13.3%** |

### Phase 2 Goal
让 LLM 编排器产出确定性生成器**无法发现的 Bug 类型**（类型层面增量，而非同一类型的不同参数变体），通过 3 个新工具实现。

### Phase 2 增量 Bug 类型枚举

| # | Bug 类型 | 具体表现 | 确定性生成器能否发现 | Milvus 已知 Issue | 对应工具 |
|---|---------|---------|-------------------|------------------|---------|
| 1 | **计数不一致** | insert N→delete M→rowCount≠N-M；bulk_insert 重复主键→rowCount 虚高；并发 insert→rowCount≠N | ❌ | #49541, #49706 | stateful + concurrent |
| 2 | **数据可见性异常** | flush→search 数据不可见；delete→search 仍返回已删除数据；load→search 失败 | ❌ | #47913, #47635 | stateful + timing |
| 3 | **搜索结果错误** | L2 距离非升序；COSINE 相似度非降序；limit=5 返回 6 条；搜索返回不属于该 collection 的数据 | ❌ | — | stateful |
| 4 | **状态残留** | drop collection→recreate→旧数据残留；drop partition→search 仍返回该分区数据；drop alias→alias 仍可用 | ❌ | — | stateful |
| 5 | **并发竞态** | 并发 insert→rowCount≠N；并发 upsert 同一 ID→出现重复实体 | ❌ | #42723, #44078 | concurrent |

### Phase 2 Constraints
- **工具替代**：`execute_stateful_test` 完全替代 `execute_api_sequence`（旧工具从工具列表移除）
- **并发模型**：应用层并发（Python threading），复用同一 Docker 沙箱，不创建多容器
- **验证流程**：复用现有 verification.rs（repro_1 → repro_2 → LLM 验证变体 → LLM 报告优化），不修改
- **不破坏已有功能**：Step 1-9 的所有验收标准仍需通过
- **3 个工具全部实现**：stateful + concurrent + timing

### Phase 2 Success Criteria
- **主标准**：≥3 个不同增量 Bug 类型 + 全部通过完整验证流程 + 至少 1 个提交为 GitHub Issue
- **降级标准**：3 次实战运行后仍无 3 个增量 Bug → 降级为"工具能力就绪 + ≥1 个增量 Bug 类型"

### Phase 2 Non-Goals
- 不修改 verification.rs
- 不创建多个 Docker 沙箱
- 不切换 Milvus 版本（聚焦 v2.6.16）
- 不修改确定性生成器

### Phase 2 Assumptions Exposed & Resolved
| Assumption | Challenge | Resolution |
|-----------|-----------|------------|
| Milvus v2.6.16 有状态一致性/并发 Bug | Contrarian: 如果根本没有此类 Bug 呢？ | 接受风险，设降级标准：3 次运行无增量 Bug 则降级为"工具能力就绪 + 1 个增量 Bug" |
| 3 个工具都必要 | Simplifier: 1 个工具可能就够了 | 全部实现，最大化覆盖 5 种 Bug 类型 |
| 并发测试需要多容器 | 应用层 threading 即可发现并发 Bug | 应用层并发，复用沙箱 |

### Phase 2 Interview Transcript

#### Round 1
**Q:** "增量 Bug"的精确定义是什么？
**A:** 确定性生成器无法发现的 Bug 类型（类型层面增量，非参数变体）
**Ambiguity:** 56%

#### Round 2
**Q:** Phase 2 的最终验收标准应该是什么？
**A:** 严格：≥3 个增量 Bug 类型 + GitHub Issue
**Ambiguity:** 44%

#### Round 3
**Q:** 新工具与旧工具的关系是什么？
**A:** 替代：移除旧工具 execute_api_sequence
**Ambiguity:** 36%

#### Round 4 (Contrarian)
**Q:** 如果 Milvus v2.6.16 根本没有状态一致性/并发 Bug 怎么办？
**A:** 可能无光：降低验收标准（3 次运行后降级为工具能力就绪 + 1 个增量 Bug）
**Ambiguity:** 31%

#### Round 5
**Q:** 以下哪些算作"增量 Bug 类型"？
**A:** 全选：计数不一致 + 数据可见性异常 + 搜索结果错误 + 状态残留 + 文献检索补充
**Ambiguity:** 24%

#### Round 6 (Simplifier)
**Q:** 3 个新工具是否都必要？
**A:** 完整：3 个工具全部实现
**Ambiguity:** 23%

#### Round 7
**Q:** execute_concurrent_test 的并发模型是什么？
**A:** 应用层并发：复用沙箱（Python threading）
**Ambiguity:** 18%

#### Round 8
**Q:** 新工具产出的缺陷如何进入验证流程？
**A:** 复用现有流程（repro_1 → repro_2 → LLM 验证变体 → LLM 报告优化）
**Ambiguity:** 13%
