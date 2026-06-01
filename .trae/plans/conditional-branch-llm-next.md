# Oracle 性能优化计划

> Based on: deep-interview (4 rounds, 用户明确需求)
> Created: 2026-05-30
> Status: ACTIVE
> Consensus: Planner → Architect(×2) → Critic(×2) → v4

---

## RALPLAN-DR Summary

### Principles (5)

1. **价值密度优先** — 先跑最可能发现缺陷的检查，mutation > assertion > range > behavior
2. **单变量验证** — 每步实施后独立测量效果，量化贡献
3. **通用性** — 所有修改对 Qdrant/Milvus/Weaviate/PgVector 生效
4. **回归安全** — 441个测试通过，violation集合不缩减
5. **简洁优先** — 最小代码实现目标，不做过度抽象

### Decision Drivers (Top 3)

1. **运行时间** — 5.5h → 1h 是硬指标
2. **缺陷检出率** — 不能漏掉真实缺陷
3. **通用性** — 必须对所有 target plugin 生效

### Viable Options

| 方案 | 核心思路 | 预估效果 | 风险 |
|------|---------|---------|------|
| A: 裁剪+排序+batch_size增大 | 断言模式去重+mutation优先+batch_size 6→20 | ~30-45min | 裁剪可能误删 |
| B: 纯并行+缓存 | 保持9411检查不变，并行执行+结果缓存 | ~1.5-2h | 不治本，复杂度高 |

**方案B失效理由**：9411检查即使4x并行仍需~1.5h，根本问题是"不该跑这么多"。但并行执行作为方案A的可选叠加优化是合理的（零语义风险，2-3x加速）。

### 关键设计决策（经3轮共识审查确定）

1. **去重key = (check_type, defect_type_pattern)**：从脚本中提取`[DEFECT: XXX]`标记，相同检查类型+相同缺陷类型的检查只保留1个。无DEFECT标记的回退到`(check_type, name_prefix)`。**不用sha256(script)**（每个脚本本身就不同，sha256无效）。
2. **去重位置 = derive_oracle_checks()内部**：不在ContractStore（约束≠检查），不在Oracle运行时（太晚）。
3. **mutation优先级最高(10)**：与现有代码`splice(0..0, mutation_checks)`设计意图一致。
4. **去掉violation密度动态停止**：去重后~210个检查全部跑完只需~7分钟，不值得增加动态停止的复杂度和过早停止的风险。submit_mre时仍跑完所有剩余检查（但数量已大幅减少）。
5. **batch_size 6→20**：低风险高收益，减少调度开销。

---

## 实施步骤

### Step 0: 审计分析（无代码改动）

**目标**：量化当前Oracle检查的组成和执行成本

**行动**：
1. 统计每个target的Oracle检查数量（按source分类）
2. 测量单个Oracle检查的平均执行时间（docker exec写入+执行）
3. 统计历史上哪些source类型发现了violation（从testvdb_run.log提取）
4. 识别behavioral_contracts中的重复模式（按defect_type_pattern分组统计）

**验收标准**：
- [ ] 审计报告包含：检查数量分解、平均执行时间、各source的violation发现率
- [ ] 数据驱动后续步骤的参数选择

### Step 1: 检查裁剪 — 断言模式去重

**目标**：减少Oracle检查数量（9411 → ~200-300）

**行动**：
1. 在 `TargetPlugin` trait 中添加默认的 `deduplicate_checks()` 方法
2. 去重key = `(check_type, defect_type_pattern)`，其中 `defect_type_pattern` 从脚本中用正则 `\[DEFECT:\s*(\w+)\]` 提取
3. 无DEFECT标记的脚本，回退到 `(check_type, name_prefix)` — name中的前缀如 `bc_upsert_`、`bc_delete_` 可区分语义
4. 相同key的检查只保留第一个（第一个通常是from_behavioral_contracts中最先出现的，覆盖面最广）
5. Qdrant已有 `seen: HashSet<String>` 去重，替换为trait默认方法

**改动文件**：
- `src/target/mod.rs`：添加 `deduplicate_checks()` 默认实现 + `extract_defect_pattern()` 辅助函数
- `src/target/qdrant.rs`：移除手动的 `seen: HashSet`，改用trait默认方法
- `src/target/milvus.rs`：添加去重调用

**验收标准**：
- [ ] Qdrant检查数量从9411降至<500
- [ ] Milvus检查数量同比例下降
- [ ] cargo test 441通过
- [ ] 采样验证：随机选10个去重组，执行保留代表+1个被去除检查，结果一致

### Step 2: 优先级排序 — mutation最高

**目标**：高价值检查排在前面，确保在有限时间内先跑最重要的

**行动**：
1. 给 `InvariantSource` 添加 `priority()` 方法
2. 优先级：DerivedFromMutation(10) > ContractExplicit(9) > DerivedFromAssertion(8) > DerivedFromState(7) > DerivedFromRange(6) > DerivedFromType(5) > DerivedFromBehavior(4)
3. 在 `Oracle::new()` 中按优先级降序排序checks数组
4. 移除 `orchestrator.rs` 中的 `splice(0..0, mutation_checks)` — 排序后mutation自然在最前

**改动文件**：
- `src/target/mod.rs`：InvariantSource添加priority()方法
- `src/agent/oracle.rs`：Oracle::new()中添加排序
- `src/agent/orchestrator.rs`：移除splice(0..0)，排序已处理

**验收标准**：
- [ ] checks数组按优先级排序，mutation在前
- [ ] cargo test通过
- [ ] 独立测量：排序后前50个检查中mutation占比>50%

### Step 3: batch_size增大 + submit_mre行为不变

**目标**：减少调度开销，submit_mre仍跑完所有剩余（但数量已大幅减少）

**行动**：
1. `oracle_batch_size` 从6增大到20
2. submit_mre的 `while oracle.has_pending()` 循环保持不变（去重后~210个检查，全部跑完约7分钟，可接受）
3. 不添加时间预算或violation密度动态停止（简洁优先）

**改动文件**：
- `src/agent/orchestrator.rs`：oracle_batch_size 6→20

**验收标准**：
- [ ] cargo test通过
- [ ] 独立测量：每轮LLM后Oracle时间变化（6→20个检查/轮）

### Step 4: 端到端验证

**目标**：确认优化后Mine总运行时间<1h，violation不遗漏

**行动**：
1. 运行Qdrant完整Mine（12轮LLM）
2. 记录总运行时间、Oracle时间占比、violation数量
3. 对比优化前后violation集合
4. 在Milvus上验证通用性

**验收标准**：
- [ ] Qdrant Mine总运行时间<1h
- [ ] 发现的violation集合 ⊇ 优化前发现的violation集合
- [ ] Milvus同样受益

### Step 5（可选）: 并行执行

**前提**：Step 4验证后仍超1h

**行动**：
1. 将 `run_next_batch` 内部的串行for循环改为 `futures::join_all` 并行执行
2. 并行度3-4（避免Docker daemon过载）
3. 零语义风险：不改变检查集合、优先级

**改动文件**：
- `src/agent/oracle.rs`：run_next_batch改为并行

**验收标准**：
- [ ] 并行执行无数据竞争
- [ ] cargo test通过
- [ ] 单批执行时间降低2-3x

---

## 风险与缓解

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| 断言模式去重误删不同行为的检查 | 中 | 高 | Step 1采样验证：10个去重组对比保留vs去除检查结果 |
| mutation内部无排序，1837个独占前面 | 低 | 中 | Step 2后mutation在前但batch_size=20，每轮跑20个不同类型 |
| 去重后检查数仍>500 | 低 | 中 | 审计数据驱动调整去重策略 |

## 共识审查历史

| 轮次 | 角色 | 判定 | 关键反馈 |
|------|------|------|---------|
| 1 | Architect | ITERATE | 裁剪策略需三元组去重；mutation优先级应最高；时间预算替代数量预算；批量合并降级 |
| 1 | Critic | ITERATE | 加入并行执行；去重在derive_oracle_checks内部；时间预算基于审计数据；mutation内部排序 |
| 2 | Architect | ITERATE | sha256(script)去重更可靠；采样验证裁剪安全性；violation密度动态停止替代固定预算 |
| 2 | Critic | ITERATE | **sha256对BC无效**（每个脚本本身就不同）；去掉动态停止（~210个检查7分钟跑完）；采样验证改为"保留vs去除"对比 |
