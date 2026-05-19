# Deep Dive Trace: review-previous-session-output

## Observed Result

TestVDB 项目 7 个 Phase 全部标记"完成"，但实际产出质量存在显著差距：代码实现有 placeholder、闭环从未完整运行、架构处于半迁移状态。

## Ranked Hypotheses

| Rank | Hypothesis | Confidence | Evidence Strength | Why it leads |
|------|------------|------------|-------------------|--------------|
| 1 | 代码实现未真正满足设计规格——Qdrant 侧大面积 placeholder、闭环反馈有逻辑漏洞、关键约束未利用 | High | Strong | 3个生成器 Qdrant 覆盖率极低，DefectKind 缺失导致闭环断裂，merge 无去重 |
| 2 | 端到端验证不完整——3轮闭环从未完整运行、Qdrant 零 Docker 验证、Shadow Mode 从未执行 | High | Strong | mine_run.log 只有 Round 1 且截断，无 Qdrant Docker compose，handoff 自认需端到端验证 |
| 3 | 架构处于半迁移状态——手写探针未移除、Plan 目标未达成、代码重复严重 | High | Moderate | Phase 4.8 未完成，main.rs 1278 行大量重复，Spec 目标 205→0 和 1→3-5 未验证 |

## Evidence Summary by Hypothesis

### Hypothesis 1: 代码实现未满足设计规格

**支持证据（强）：**

1. **Qdrant placeholder 实现**（CRITICAL）：
   - state_gen: 仅 3/9 实现，其余 6 种 placeholder（exit(0)）
   - metamorphic: 仅 2/7 实现，其余 5 种 placeholder
   - sequence_gen: 仅 1/20 实现，其余 19 种 placeholder
   - placeholder 脚本以 exit(0) 退出，不产生 defect 也不触发闭环反馈

2. **ContractStore.merge() 无去重**（CRITICAL）：
   - 所有集合字段直接 extend，约束可无限膨胀
   - required_params/enum_values 的 merge 也是追加而非去重

3. **DefectKind 缺失 MetamorphicViolation 和 StateLogicViolation**（MEDIUM→HIGH）：
   - analyzer.rs 的 from_defect_line() 无法识别这两类标记
   - Unknown 分支不做任何约束反哺，闭环对这两类缺陷静默失效

4. **MutationTestGenerator 未利用 range_constraints 和 enum_values**（HIGH）：
   - ParamInfo.enum_values 被收集但从未使用
   - 遗漏范围越界和非法枚举两种重要测试维度

5. **Qdrant mutation 脚本全部使用 PUT**（HIGH）：
   - 搜索端点用 PUT 会返回 405，测试无法真正执行

6. **SDK 连接地址硬编码**（HIGH）：
   - Milvus: localhost:19530，Qdrant: BASE（REST URL 而非 gRPC）

7. **extract_context() 端点推断极其脆弱**（HIGH）：
   - 通过下划线分割 test_name 推断，实际取到不完整路径

**反对证据（弱）：**
- Milvus 侧实现完整，7 个生成器全部有实际逻辑
- 大部分生成器有 dedup_by 去重
- 每个生成器有单元测试（但只覆盖 Milvus 分支）

### Hypothesis 2: 端到端验证不完整

**支持证据（强）：**

1. **Phase 5 三轮闭环从未完整运行**：
   - mine_run.log 只有 Round 1，无 Round 2/3
   - 日志在 mutation 1414/2241 处截断

2. **Phase 7 Qdrant 零 Docker 运行证据**：
   - 无 Qdrant docker-compose 文件
   - mine_run.log 全部是 Milvus 测试
   - testvdb_baseline.json target="milvus"

3. **Shadow Mode 从未执行**：
   - 无 Shadow Mode 对比日志
   - 无法证明"确定性测试 >= 手写探针"

4. **FAOrchestrator LLM 循环无运行证据**：
   - CreativeMutationPrompt 无真实 LLM 调用证据
   - LLM 测试标记 #[ignore]

5. **Phase 4.3-4.7 无真实 Docker 执行证据**：
   - state/metamorphic/sequence/resource/combo/diff/concurrent 只有单元测试

**反对证据（弱）：**
- mine_run.log 证明 boundary+mutation 在真实 Milvus Docker 中运行
- testvdb_baseline.json 有真实缺陷数据（25 个缺陷）
- milvus_bug_report.md 存在（1 个样本）
- volumes/milvus/ 有真实数据文件

### Hypothesis 3: 架构半迁移状态

**支持证据（强）：**

1. **Phase 4.8 "Full Cutover" 从未完成**：
   - 手写探针文件（probe_milvus.rs, probe_milvus_advanced.rs）仍然存在
   - batch 命令仍运行手写探针，mine 命令运行契约驱动测试
   - 两套系统并行但未整合

2. **Spec 目标未验证**：
   - "手写探针数 205 → 0"：未达成，手写探针仍在
   - "Submission-grade Bug/次 1 → 3-5"：未验证
   - "新VDB接入工作量 200+ → 文档URL+Docker配置"：Qdrant 需要手写 placeholder，未达成

3. **main.rs 1278 行，代码重复严重**：
   - run_batch() 和 run_batch_simple() 大量重复
   - run_generic_batch() 被调用 9 次，每次重新创建 TargetRegistry
   - run_mine() 承担过多职责（合约加载+增强+闭环+LLM+验证+Shadow）

4. **batch 和 mine 命令职责不清**：
   - batch 运行手写探针，mine 运行契约驱动测试
   - 两者共享 Docker 基础设施但独立实现

**反对证据（弱）：**
- TargetStyle enum 机制设计合理
- ContractStore 数据模型完整
- 闭环管线代码逻辑清晰

## Evidence Against / Missing Evidence

### Hypothesis 1:
- **缺失**：Qdrant placeholder 是有意渐进实现还是未完成？无 TODO/FIXME 标记
- **缺失**：闭环反馈在实际运行中是否过早收敛？需实际运行数据
- **缺失**：QdrantClient(url=BASE) 在当前版本是否默认使用 REST？

### Hypothesis 2:
- **缺失**：mine_run.log 为什么在 mutation 1414/2241 处中断？手动还是崩溃？
- **缺失**：FAOrchestrator::run() 是否曾经成功执行过？
- **缺失**：Qdrant 生成器生成的 Python 脚本是否语法正确？

### Hypothesis 3:
- **缺失**：Phase 4.8 是否有计划但未执行？还是刻意保留手写探针作为 fallback？
- **缺失**：如果移除手写探针，mine 命令的确定性测试能否完全替代？

## Per-Lane Critical Unknowns

- **Lane 1 (代码实现质量)**: Qdrant placeholder 是渐进实现策略还是未完成的半成品？DefectKind 缺失对闭环的实际影响有多大？
- **Lane 2 (端到端验证)**: 3轮闭环在真实环境中能否收敛？FAOrchestrator LLM 循环是否曾经成功运行？
- **Lane 3 (架构债务)**: Phase 4.8 Full Cutover 是否有明确的执行计划？手写探针是否应该保留为 fallback？

## Rebuttal Round

- **Best rebuttal to leader**: 项目确实能编译通过（252 tests），Milvus 侧有真实运行证据，1个 Bug Report 已生成——说明核心管线是通的，问题可能只是"未充分验证"而非"根本不工作"
- **Why leader held**: 编译通过 ≠ 端到端可用。Qdrant placeholder 和闭环断裂是结构性问题，不是验证不足。即使 Milvus 侧能工作，闭环反馈的 DefectKind 缺失意味着系统无法从自身发现中学习——这是核心能力的缺失

## Convergence / Separation Notes

- 通道 1 和通道 2 收敛于同一结论：**闭环反馈机制是核心创新但从未被验证**
- 通道 1 发现闭环代码有逻辑漏洞（DefectKind 缺失），通道 2 发现闭环从未完整运行——两者叠加意味着闭环可能即使运行也无法正确工作
- 通道 3 独立发现 Phase 4.8 未完成，与通道 1 的 Qdrant placeholder 形成呼应：系统声称"7/7 Phase 完成"但实际处于半迁移状态

## Most Likely Explanation

TestVDB 项目的 7 个 Phase 是按"编译通过 + 单元测试通过"的标准标记为"完成"的，而非按"端到端运行验证"的标准。这导致：
1. **Qdrant 侧实现严重不足**（placeholder 代替真实逻辑），但编译和 Milvus 单元测试通过
2. **闭环反馈有逻辑漏洞**（DefectKind 缺失、merge 无去重），但单元测试只验证了正向路径
3. **3轮闭环从未完整运行**，mine_run.log 截断说明运行时问题未解决
4. **手写探针未移除**，系统处于"新旧并行"的半迁移状态

## Critical Unknown

**3轮闭环在修复 DefectKind 缺失和 merge 去重问题后，能否在真实 Milvus Docker 环境中正确收敛？** 这是决定项目下一步方向的关键——如果闭环能工作，则优先修复代码质量问题；如果闭环根本无法收敛，则需要重新设计反馈机制。

## Recommended Discriminating Probe

1. **修复 DefectKind 缺失 + merge 去重** → 运行 `mine --target milvus` 完整 3 轮闭环 → 观察收敛行为
2. **对 Qdrant 运行 boundary + mutation 测试** → 验证生成的 Python 脚本是否语法正确且能执行
3. **运行 Shadow Mode** → 对比确定性测试 vs 手写探针的 Bug 发现能力
