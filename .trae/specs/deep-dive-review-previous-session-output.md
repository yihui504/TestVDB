# Deep Dive Spec: TestVDB 上一实战复盘改进

**Created:** 2026-05-17
**Source:** deep-dive trace + interview
**Trace:** `.trae/specs/deep-dive-trace-review-previous-session-output.md`

---

## 1. 目标

将 TestVDB 从"7 Phase 标记完成但实际半迁移"状态，推进到"架构清晰、功能完整、端到端验证通过"的可用状态。

### 1.1 量化目标

| 指标 | 当前值 | 目标值 |
|------|--------|--------|
| Qdrant 生成器实际覆盖率 | state 33%, meta 29%, seq 5% | 全部 100%（无 placeholder） |
| 闭环反馈完整运行 | 从未完成 | 3 轮闭环在 Milvus Docker 中收敛 |
| Shadow Mode 对比 | 从未运行 | 确定性测试 vs 手写探针完整对比 |
| DefectKind 覆盖 | 4/6 种 | 6/6 种 |
| ContractStore.merge 去重 | 无 | 按 endpoint+param 去重 |
| MutationTestGenerator 约束利用 | 仅 type+required | type+range+enum+required 全利用 |
| main.rs 行数 | 1278 | < 200（提取模块化） |
| 端到端验证 | 仅 Milvus boundary+mutation | Milvus 全流程 + Qdrant 全流程 |

---

## 2. 可执行条目

### E1: 架构重构 — main.rs 模块化

**模块划分：**
```
src/
  infra.rs            — Docker容器创建/网络查找/pip安装/脚本执行/清理
  contract_loader.rs  — 合约加载+增强+OpenAPI合并
  feedback_loop.rs    — 3轮闭环+收敛检测
  verification_runner.rs — BatchDefect→CandidateDefect→沙箱复现→BugReport
  main.rs             — CLI解析+命令分发（<200行）
```

**验收标准：**
- [ ] 提取 Docker 基础设施层：`infra.rs`（容器创建/网络查找/pip安装/脚本执行/清理）
- [ ] 提取合约加载层：`contract_loader.rs`（从文件/KA加载+增强+OpenAPI合并）
- [ ] 提取闭环执行层：`feedback_loop.rs`（3轮闭环+收敛检测）
- [ ] 提取验证层：`verification_runner.rs`（BatchDefect→CandidateDefect→沙箱复现→BugReport）
- [ ] main.rs 仅保留 CLI 解析和命令分发，行数 < 200
- [ ] cargo test 全部通过

### E2: Qdrant 生成器完整实现

**前置步骤：** 先启动 Qdrant Docker，用 curl 验证 API 行为，然后基于实际行为写测试

**验收标准：**
- [ ] state_gen.rs: 9/9 种状态测试有真实 Qdrant 测试逻辑（非 placeholder）
- [ ] metamorphic.rs: 7/7 种蜕变测试有真实 Qdrant 测试逻辑
- [ ] sequence_gen.rs: 20/20 种序列测试有真实 Qdrant 测试逻辑
- [ ] 所有 Qdrant 脚本使用正确的 HTTP 方法（PUT/POST/DELETE 匹配 Qdrant API）
- [ ] 所有 Qdrant 脚本使用正确的请求体格式
- [ ] Qdrant docker-compose 文件创建并可用
- [ ] cargo test 全部通过

### E3: Milvus 侧代码质量修复

**验收标准：**
- [ ] MutationTestGenerator.from_store() 利用 range_constraints 生成范围越界测试
- [ ] MutationTestGenerator.from_store() 利用 enum_values 生成非法枚举测试
- [ ] Diff 测试 SDK 连接地址使用 {TESTVDB_DB_URL} 而非硬编码 localhost:19530
- [ ] Qdrant Diff 测试 QdrantClient 使用正确连接方式（prefer_grpc=False 或 gRPC URL）
- [ ] extract_context() 使用结构化信息（从 BatchDefect 传递 endpoint/param）而非字符串推断
- [ ] cargo test 全部通过

### E4: 闭环反馈修复

**收敛标准：** 当轮无新 observed_behavior 时收敛（修复去重后此逻辑才可靠）

**验收标准：**
- [ ] DefectKind 枚举新增 MetamorphicViolation 和 StateLogicViolation
- [ ] from_defect_line() 能识别 METAMORPHIC_VIOLATION 和 STATE_LOGIC_VIOLATION
- [ ] ContractStore.merge() 按 endpoint+param_name 去重（type_constraints, range_constraints）
- [ ] required_params merge 去重（同 endpoint 不重复添加同 param）
- [ ] enum_values merge 去重（同 param 不重复添加同 value）
- [ ] assimilate_batch() 的约束反哺也做去重检查
- [ ] 3 轮闭环在 Milvus Docker 中完整运行（各轮均有日志输出，无 crash/panic）并收敛（当轮 new_observations == 0）
- [ ] cargo test 全部通过

### E5: 端到端验证

**验收标准：**
- [ ] `mine --target milvus` 完整 3 轮闭环运行成功（各轮均有日志输出）
- [ ] `mine --target qdrant` 至少 Round 1 运行成功（有 defect 输出）
- [ ] Shadow Mode 对比：确定性测试 vs 手写探针的 Bug 发现数量
- [ ] 至少 1 个 Submission-grade Bug Report 通过验证管线（包含复现步骤、期望行为、实际行为、严重级别）
- [ ] Qdrant docker-compose 文件创建并可用

### E6: Full Cutover（条件性）

**前置条件：** E1-E5 全部完成，Shadow Mode 证明确定性测试 >= 手写探针

**Shadow Mode 验证标准：** Bug 发现数量 >= 手写探针 **且** 端点覆盖率 >= 手写探针

**验收标准：**
- [ ] 手写探针文件归档（移至 archive/ 目录）
- [ ] batch 命令改为运行契约驱动测试
- [ ] 移除 probe_milvus.rs 和 probe_milvus_advanced.rs 的编译引用
- [ ] cargo test 全部通过

---

## 3. 执行顺序

```
E1 (架构重构)
  │
  ├─→ E2 (Qdrant 生成器完整实现)
  │     │
  │     └─→ E3 (Milvus 侧代码质量修复)
  │           │
  │           └─→ E4 (闭环反馈修复)
  │                 │
  │                 └─→ E5 (端到端验证)
  │                       │
  │                       └─→ E6 (Full Cutover, 条件性)
  │
  └─→ E3 可与 E2 并行
```

**关键路径：** E1 → E2/E3(并行) → E4 → E5 → E6

---

## 4. 约束

| 约束 | 说明 |
|------|------|
| 架构优先，验证先行 | 先验证架构重构的前置条件（闭环基础），再重构 main.rs，再在清晰架构上修复功能 |
| Qdrant 一等公民 | 所有生成器必须完整实现 Qdrant 支持，不接受 placeholder |
| 全量修复 | 不跳过任何已发现的问题 |
| 端到端验证 | 每个 E 完成后必须在真实 Docker 环境验证 |
| 不引入新依赖 | 所有修复在现有 Rust + Python 生态内完成 |
| cargo test 0 failed | 每步完成后编译和测试必须通过 |

---

## 5. 风险

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| 架构重构引入回归 | 中 | 测试失败 | 重构前确保 cargo test 基线，重构后对比 |
| Qdrant API 行为与假设不符 | 中 | 脚本执行失败 | 先用 curl 验证 Qdrant API 行为再写脚本 |
| 3轮闭环无法收敛 | 中 | 核心机制失效 | 添加详细日志观察每轮约束变化 |
| Shadow Mode 对比不通过 | 中 | 无法移除手写探针 | 保留手写探针作为 fallback |
| Docker 资源不足 | 低 | 测试环境不稳定 | 限制并发容器数 |

---

## 6. Trace Findings

### 最可能解释

7 个 Phase 按"编译通过+单元测试通过"标准标记完成，而非"端到端运行验证"标准。导致 Qdrant 侧严重不足、闭环有逻辑漏洞、系统处于半迁移状态。

### 各通道关键未知项解决情况

| 未知项 | 解决方案 |
|--------|---------|
| Qdrant placeholder 是渐进还是未完成？ | 用户确认：必须完整实现，不接受 placeholder |
| 闭环反馈是否过早收敛？ | E4 修复后通过 E5 端到端验证 |
| Phase 4.8 是否有计划？ | E6 条件性执行，依赖 Shadow Mode 验证 |
| 手写探针是否保留？ | E6 移除，但依赖 Shadow Mode 验证通过 |

### 塑造 Interview 的证据

- 3 条通道一致指向：闭环反馈是核心创新但从未被验证
- Qdrant placeholder 不是"渐进实现"而是"未完成的半成品"
- main.rs 的代码重复是架构债务的集中体现，必须先清理

---

## 7. Interview Transcript

| # | 问题 | 用户回答 |
|---|------|---------|
| 1 | Qdrant 支持的优先级？ | 一等公民，必须完整实现 |
| 2 | 半迁移状态如何处理？ | 先修复再验证再切换 |
| 3 | 闭环优先级？ | 生成器优先，闭环后修 |
| 4 | 修复范围？ | 全量修复 |
| 5 | 架构重构和功能修复关系？ | 架构优先，功能后修 |
| 6 | main.rs 模块划分方案？ | 同意4模块提取方案 |
| 7 | 闭环收敛标准？ | 无新观察即收敛（需修复去重后才可靠） |
| 8 | Qdrant API 行为验证方式？ | 先验证API再写测试 |
| 9 | Shadow Mode 阈值？ | Bug数量和端点覆盖两者都 >= 手写 |
