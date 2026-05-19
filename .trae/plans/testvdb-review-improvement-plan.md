# TestVDB 复盘改进计划

**Created:** 2026-05-17
**Updated:** 2026-05-19
**Spec:** `.trae/specs/deep-dive-review-previous-session-output.md`
**Trace:** `.trae/specs/deep-dive-trace-review-previous-session-output.md`
**Status:** COMPLETED (Step 0-6 全部完成，Step 7 不建议执行)

---

## RALPLAN-DR Summary

### Principles

1. **架构优先，验证先行** — 先验证架构重构的前置条件（闭环基础是否可靠），再投资架构重构；架构清晰后功能修复更高效
2. **Qdrant 一等公民** — 所有生成器必须完整实现 Qdrant 支持，不接受 placeholder
3. **全量修复** — 不跳过任何已发现的问题，包括 Milvus 侧代码质量
4. **端到端验证** — 每步完成后必须在真实 Docker 环境验证
5. **先验证API再写测试** — Qdrant 实现前先用 curl 验证 API 行为

### Decision Drivers

1. **闭环反馈是核心创新但从未被验证** — 3 条 Trace 通道一致指向此结论
2. **Qdrant placeholder 不是渐进实现而是未完成的半成品** — 用户确认必须完整实现
3. **main.rs 1278 行是架构债务的集中体现** — 必须先清理才能有效修复功能

### Viable Options

#### Option A: 架构优先，验证先行 → 架构重构 → 生成器补齐 → 代码质量修复 → 闭环验证 → 完整重构 → 端到端验证 → Full Cutover（推荐）

**Approach:** 先用最小改动验证架构重构的前置条件（闭环基础），再逐步投资架构重构和功能补齐。Step 0 不是"功能优先"，而是"验证架构重构是否安全"。

**Pros:**
- 验证前置条件确保架构重构在可靠基础上进行
- 如果闭环基础不工作，避免在不确定基础上大规模重构
- 架构重构（Step 1/5）是主线，功能修复（Step 2/3/4）在清晰架构上更高效

**Cons:**
- 步骤更多（8步 vs 6步）
- 完整架构重构分两阶段（Step 1 精简版 + Step 5 完整版）

#### Option B: 纯架构优先 → 生成器补齐 → 闭环修复 → 端到端验证 → Full Cutover

**Approach:** 不做前置验证，直接按 E1→E2/E3(并行)→E4→E5→E6 顺序执行

**Pros:**
- 步骤更少（6步 vs 8步）
- 架构清晰后功能修复更高效

**Cons:**
- 架构重构本身有引入回归的风险（现有测试不覆盖 Qdrant 和 Docker 编排）
- 如果闭环根本不收敛，前期架构重构工时浪费
- 核心假设（闭环收敛）推迟到 Step 4 才验证，风险后置

**Invalidation rationale for Option B:** 跳过前置验证直接大规模重构，如果闭环基础不可靠则重构工时浪费。Step 0 的 DefectKind + merge 修复是架构重构的必要前提（否则无法验证重构未破坏反馈机制），3轮闭环测试是验证基础可靠性的 sanity check，总投入约 2 个文件的修改，风险极低。

#### Option C: 功能优先 → 闭环修复 → 架构重构 → 端到端验证

**Approach:** 先修 Qdrant placeholder 和闭环漏洞，再重构架构

**Pros:**
- 快速看到功能改进
- 闭环修复后可以立即验证

**Cons:**
- 在混乱架构上修功能效率低
- 架构重构时可能需要重新调整已修复的功能代码
- 代码重复问题在功能修复阶段会放大（9 处 run_generic_batch 调用、2 处 run_batch 重复）

**Invalidation rationale for Option C:** 用户明确选择"架构优先，功能后修"，且 main.rs 的代码重复会在功能修复阶段造成大量冗余工作。

---

## Implementation Steps

### Step 0: 前置条件验证（Go/No-Go 决策点）

**目的：** 验证架构重构的前置条件——闭环反馈基础是否可靠。DefectKind 缺失和 merge 无去重是闭环的必要修复，不是"功能优先"而是"验证基础"。如果闭环基础不工作，架构重构失去意义。

**Files:**
- `src/contract/analyzer.rs` — DefectKind 新增 + from_defect_line 修复
- `src/contract/store.rs` — merge() 去重

**Tasks:**

0.1. `analyzer.rs`: DefectKind 新增 MetamorphicViolation 和 StateLogicViolation
- from_defect_line() 识别 METAMORPHIC_VIOLATION 和 STATE_LOGIC_VIOLATION
- assimilate_batch() 中新 DefectKind 分支生成正确约束

0.2. `store.rs`: merge() 去重
- type_constraints: 按 endpoint+param_name 去重
- range_constraints: 按 endpoint+param_name 去重
- required_params: 按 endpoint+param 去重
- enum_values: 按 param+value 去重
- observed_behaviors: 按 description 去重（已有）

0.3. 在 Milvus Docker 中运行 3 轮闭环：
```bash
cargo run --release -- mine --target milvus --version 2.4 --contracts ./contracts
```

0.4. **Go/No-Go 决策：**
- Go（3轮闭环各轮均有日志输出，无crash/panic）→ 继续 Step 1
- No-Go（闭环crash或无输出）→ 重新设计反馈机制，暂停后续步骤

**Verification:**
```bash
cargo build --release
cargo test
cargo run --release -- mine --target milvus --version 2.4 --contracts ./contracts
```

**Exit Criteria:**
- DefectKind 覆盖 6/6 种（from_defect_line 对所有6种标记返回正确变体）
- merge() 去重：相同 endpoint+param 的约束不重复追加
- 3 轮闭环各轮均有日志输出，无 crash/panic
- Go/No-Go 决策做出

---

### Step 1: 精简版架构重构 — 提取 infra.rs

**目的：** 仅提取 Docker 编排层（重复最严重、提取最安全的部分），不改变调用结构。

**Files:**
- `src/infra.rs`（新建）
- `src/main.rs`（修改调用点）

**Tasks:**

1.1. 创建 `src/infra.rs`：
- 提取 `find_docker_network()` — 从 run_batch/run_batch_simple/run_generic_batch 中提取网络查找逻辑
- 提取 `ensure_runner_container()` — 容器创建+pip安装
- 提取 `execute_probe_script()` — 脚本写入+cp+exec+结果收集
- 提取 `cleanup_runner()` — 容器清理
- 提取 `cleanup_stale_containers()` — 开头清理旧容器
- 移入 `run_generic_batch()` — 整体移入 infra.rs（其核心是 Docker 编排，不是反馈循环）

1.2. 更新 main.rs 调用点：调用 infra.rs 的公共函数

1.3. 补充 Qdrant 路径的集成测试（至少验证 boundary 生成器在 Qdrant 模式下生成正确脚本）

**Verification:**
```bash
cargo build --release
cargo test
```

**Exit Criteria:**
- infra.rs 存在且编译通过
- run_generic_batch 在 infra.rs 中
- main.rs 调用 infra.rs 的函数
- cargo test 0 failed

---

### Step 2: Qdrant 生成器完整实现

**前置步骤：** 启动 Qdrant Docker，用 curl 验证 API 行为

**Files:**
- `src/agent/vdbfuzz/state_gen.rs`
- `src/agent/vdbfuzz/metamorphic.rs`
- `src/agent/vdbfuzz/sequence_gen.rs`
- `src/agent/vdbfuzz/mutation.rs`（修复 PUT 方法问题）
- `src/agent/vdbfuzz/diff_concurrent.rs`（修复 QdrantClient 连接）
- `docker-compose.qdrant.yml`（新建）

**Tasks:**

2.1. 创建 `docker-compose.qdrant.yml`

2.2. 启动 Qdrant Docker，用 curl 验证关键 API：
- PUT /collections/{name} — 创建集合
- POST /collections/{name}/points — 插入点
- POST /collections/{name}/points/search — 搜索
- DELETE /collections/{name} — 删除集合
- POST /collections/{name}/points/delete — 删除点
- GET /collections/{name} — 获取集合信息

2.3. 修复 `mutation.rs` Qdrant 脚本 HTTP 方法：
- 创建集合: PUT
- 插入点: PUT (upsert)
- 搜索: POST
- 删除点: POST
- 删除集合: DELETE

2.4. 实现 `state_gen.rs` Qdrant 6 种 placeholder：
- InsertDeleteInsertSearch
- UpsertChangesVector
- CreateDropCreateDim
- DropThenSearch
- PartitionDataIsolation
- InsertWithoutCollection

2.5. 实现 `metamorphic.rs` Qdrant 5 种 placeholder：
- NprobeMonotonicity (Qdrant 等价: exact vs hnsw)
- EfSearchMonotonicity (Qdrant 等价: ef 参数)
- InsertMonotonicity
- FlatL2Ordering
- FlatCosineOrdering

2.6. 实现 `sequence_gen.rs` Qdrant 19 种 placeholder

2.7. 修复 `diff_concurrent.rs` QdrantClient 连接：
- 使用 `QdrantClient(url=BASE, prefer_grpc=False)` 或 gRPC URL

**Verification:**
```bash
cargo build --release
cargo test
# 启动 Qdrant Docker 后运行
cargo run --release -- mine --target qdrant --version 1.17.1 --contracts ./contracts
```

**Exit Criteria:**
- 0 个 placeholder（state 9/9, meta 7/7, seq 20/20）
- Qdrant mutation 脚本使用正确 HTTP 方法（搜索用POST，删除集合用DELETE）
- cargo test 0 failed

---

### Step 3: Milvus 侧代码质量修复

**注意：** Step 2 和 Step 3 共享 mutation.rs 和 diff_concurrent.rs，需串行执行。Step 2 先完成 Qdrant HTTP 方法修复，Step 3 再添加 range/enum 约束利用和 SDK 参数化。

**Files:**
- `src/agent/vdbfuzz/mutation.rs`
- `src/agent/vdbfuzz/diff_concurrent.rs`
- `src/contract/analyzer.rs`

**Tasks:**

3.1. `mutation.rs`: 利用 range_constraints 生成范围越界测试
- 遍历 store.range_constraints
- 对每个 range 约束生成 AboveMax 和 BelowMin 测试

3.2. `mutation.rs`: 利用 enum_values 生成非法枚举测试
- 遍历 store.enum_values
- 对每个枚举参数生成 InvalidEnum 测试

3.3. `diff_concurrent.rs`: SDK 连接地址参数化
- Milvus: 使用环境变量或 {TESTVDB_DB_URL} 替代硬编码 localhost:19530
- 确保在非默认端口配置下正常工作

3.4. `analyzer.rs`: 修复 extract_context()
- BatchDefect 新增 endpoint 和 param_name 字段
- extract_context() 使用结构化信息而非字符串推断
- 更新所有 BatchDefect 创建处填充新字段
- **特别注意 Qdrant 端点路径格式**（/collections/{name}/... vs /v2/vectordb/...）

**Verification:**
```bash
cargo build --release
cargo test
```

**Exit Criteria:**
- MutationTestGenerator 利用 range+enum 约束（from_store 遍历 range_constraints 和 enum_values）
- extract_context() 使用 BatchDefect.endpoint 和 BatchDefect.param_name 字段，不依赖字符串推断
- SDK 连接地址从环境变量读取，非硬编码
- cargo test 0 failed

---

### Step 4: 闭环反馈修复

**Files:**
- `src/feedback_loop.rs`（Step 1 提取后的模块）
- `src/contract/analyzer.rs`（Step 0 已修复 DefectKind，Step 3 已修复 extract_context）

**Tasks:**

4.1. 确认 Step 0 的 DefectKind 和 merge 去重修复在架构重构后仍然正确

4.2. 添加闭环日志：
- 每轮开始时记录 type_constraints/range_constraints/observed_behaviors 数量
- 每轮结束时记录 new_observations 数量
- 收敛时记录收敛原因

4.3. 在 Milvus Docker 中运行完整 3 轮闭环：
```bash
cargo run --release -- mine --target milvus --version 2.4 --contracts ./contracts
```

4.4. 验证收敛行为：
- Round 1 应发现多个 defect
- Round 2 应发现新的 observed_behavior（反哺生效）
- Round 3 应收敛（当轮 new_observations == 0）或发现少量新观察

4.5. **闭环不收敛的 fallback 策略：**
- 如果 3 轮后仍不收敛，增加至 5 轮
- 如果 5 轮后仍不收敛，暂停并输出当前约束快照供人工审查
- 收敛标准：当轮 new_observations == 0

**Verification:**
```bash
cargo run --release -- mine --target milvus --version 2.4 --contracts ./contracts
```

**Exit Criteria:**
- 3 轮闭环各轮均有日志输出，无 crash/panic
- 收敛检测：当轮 new_observations == 0 时标记收敛并记录收敛原因
- 至少 1 个 Bug Report 包含：复现步骤、期望行为、实际行为、严重级别

---

### Step 5: 完整架构重构 — 剩余模块提取

**前置条件：** Step 0-4 完成，闭环已验证工作

**Files:**
- `src/contract_loader.rs`（新建）
- `src/feedback_loop.rs`（新建）
- `src/verification_runner.rs`（新建）
- `src/main.rs`（精简）

**Tasks:**

5.1. 创建 `src/contract_loader.rs`：
- 提取 `load_and_augment_contract()` — 合约加载+增强+OpenAPI合并+ContractStore构建
- 提取 `augment_contract()` — 从 main.rs 移入
- 注意：`run_knowledge_agent()` 保留在 main.rs（涉及 LLM/Sandbox/Git 编排，不属于合约加载）

5.2. 创建 `src/feedback_loop.rs`：
- 提取 `run_deterministic_round()` — 从 main.rs 移入（调用 infra.rs 的 run_generic_batch）

5.3. 创建 `src/verification_runner.rs`：
- 提取 `verify_batch_defects()` — BatchDefect→CandidateDefect→沙箱复现→BugReport
- 提取 `verify_mine_defect()` — LLM 发现的缺陷验证

5.4. 精简 main.rs：
- 保留 CLI 解析、命令分发、run_knowledge_agent
- 目标行数 < 200

**Exit Criteria:**
- 3 个新模块文件存在且编译通过
- main.rs < 200 行
- cargo test 0 failed

---

### Step 6: 端到端验证

**Tasks:**

6.1. Milvus 端到端：
```bash
cargo run --release -- mine --target milvus --version 2.4 --contracts ./contracts --shadow
```

6.2. Qdrant 端到端：
```bash
cargo run --release -- mine --target qdrant --version 1.17.1 --contracts ./contracts
```

6.3. Shadow Mode 对比：
- 确定性测试 Bug 发现数量 vs 手写探针
- 确定性测试端点覆盖率 vs 手写探针

6.4. 记录 Shadow Mode 结果，决定是否执行 Step 7

**Exit Criteria:**
- Milvus 3轮闭环完整运行，各轮有日志输出
- Qdrant 至少 Round 1 运行成功（有 defect 输出）
- Shadow Mode 对比数据记录（Bug数量 + 端点覆盖率两个维度）

---

### Step 7: Full Cutover（条件性）

**前置条件：** Shadow Mode 证明确定性测试 Bug 数量 >= 手写探针 且 端点覆盖率 >= 手写探针

**Tasks:**

7.1. 归档手写探针文件：
- `src/target/probe_milvus.rs` → `src/target/archive/probe_milvus.rs`
- `src/target/probe_milvus_advanced.rs` → `src/target/archive/probe_milvus_advanced.rs`

7.2. 更新 `src/target/mod.rs`：移除 probe_milvus 模块引用

7.3. 更新 `src/target/milvus.rs`：safety_nets() 返回空 Vec 或移除方法

7.4. 更新 batch 命令：改为运行契约驱动测试

7.5. cargo test 全部通过

**Exit Criteria:**
- 手写探针文件移至 archive/ 目录
- batch 命令运行契约驱动测试（非手写探针）
- cargo test 0 failed

---

## ADR

### Decision
架构优先，验证先行 → 架构重构(精简版) → 生成器补齐 → 代码质量修复 → 闭环验证 → 完整重构 → 端到端验证 → Full Cutover

### Drivers
1. 闭环反馈从未被验证（3 条 Trace 通道一致结论）— 架构重构前必须验证基础可靠
2. Qdrant placeholder 是未完成的半成品（用户确认必须完整实现）
3. main.rs 代码重复是架构债务，用户选择架构优先

### Alternatives Considered
1. **纯架构优先** — 不做前置验证直接重构。被否决：闭环基础未验证，如果闭环不收敛则重构工时浪费。Step 0 的前置验证仅涉及2个文件的修改，风险极低但信息价值极高。
2. **仅修 Qdrant** — 最小修复范围。被否决：用户要求全量修复，Milvus 侧也有多个代码质量问题。

### Why Chosen
"架构优先，验证先行"既尊重用户"架构优先"的选择，又回应了Architect的风险关切。Step 0 不是"功能优先于架构"，而是"验证架构重构的前置条件"——DefectKind + merge 修复是验证重构安全性的必要前提，3轮闭环测试是验证基础可靠性的 sanity check。架构重构（Step 1/5）是主线，功能修复（Step 2/3/4）在清晰架构上更高效。

### Consequences
- Step 0 是 Go/No-Go 决策点，如果闭环基础不工作则暂停后续步骤
- 步骤更多（8步 vs 原计划6步），但每步更小更安全
- 完整架构重构推迟到 Step 5，但 Step 1 已解决最严重的代码重复

### Follow-ups
- Step 7 Full Cutover 依赖 Shadow Mode 验证结果
- 如果 Shadow Mode 不通过，保留手写探针作为 fallback
- 后续可考虑新 VDB 插件接入（Weaviate/Chroma）

---

## Mutation Log

| 时间 | 操作 | 原因 |
|------|------|------|
| 2026-05-17 | 创建 | 基于 deep-dive trace + interview 精炼 |
| 2026-05-17 | 修订v2 | Critic评审修正：原则改为"架构优先，验证先行"；消除Step 0/3重复；量化验收标准；修正Option B否决逻辑 |
| 2026-05-18 | 代码审计 | Step 0-3 和 Step 5 大部分已在代码中实现，更新进度标记 |

---

## Progress Tracker

| Step | 计划标记 | 代码真实状态 | 验证方式 | 剩余工作 |
|------|----------|-------------|----------|----------|
| **Step 0** | ✅ | ✅ 已完成 | cargo test 255/255, Milvus闭环Round1发现150+缺陷 | 无 |
| **Step 1** | ✅ | ✅ 已完成 | infra.rs存在, 8处infra::调用, cargo test通过 | 无 |
| **Step 2** | ⬜ | **✅ 已完成** | 4个生成器Qdrant分支全部实现, 0个placeholder/todo! | 无 |
| **Step 3** | ⬜ | **✅ 已完成** | mutation from_store用range+enum, extract_context实现, TESTVDB_DB_URL替代hardcode | 无 |
| **Step 4** | ⬜ | **✅ 已完成** | 反馈循环前启动DB Sandbox + 9处错误日志 + run_generic_batch_with_sandbox | 无 |
| **Step 5** | ⬜ | **✅ 已完成** | contract_loader/feedback_loop/verification_runner已提取, main.rs=143行 | commands.rs 679→300行 ✅ |
| **Step 6** | ⬜ | **✅ 已完成** | Qdrant端到端验证通过, Round1发现13缺陷, 资源释放正常 | 无 |
| **Step 7** | ⬜ | **⬜ 不建议执行** | Shadow Mode结论：两种模式互补，Mine广度4×但深度不足，不建议Full Cutover | 保留手写探针 |

### Step 4 关键发现

**问题：** `run_mine()` 在反馈循环（commands.rs:462）前不启动DB容器。`run_generic_batch()` 调用 `find_docker_network()` 查找运行中的容器，找不到时返回Err被 `if let Ok(defects)` 静默吞掉，导致Round 1发现0缺陷→立即假收敛。

**修复方案：**
1. 在反馈循环前，用Sandbox启动DB容器
2. 将 `if let Ok(defects)` 改为 `if let Ok(defects) { ... } else { warn!(...) }` 记录错误
3. 反馈循环结束后清理Sandbox

### Milvus 缺陷提交记录

**测试版本：** Milvus v2.6.16 (Docker standalone)

**发现的6项真实缺陷（手动验证）：**

| # | 缺陷 | 端点 | 官方Issue | 状态 | 接受率预估 |
|---|------|------|-----------|------|-----------|
| 1 | `dbName=""` 被接受 | collections/list, describe, drop | [#49889](https://github.com/milvus-io/milvus/issues/49889) | open | 70-80% |
| 2 | `Request-Timeout` 接受非integer类型 | 所有端点 | [#49890](https://github.com/milvus-io/milvus/issues/49890) | open | 60-70% |
| 3 | 重复创建collection返回success | collections/create | [#49824](https://github.com/milvus-io/milvus/issues/49824) | closed by author | 20-30% |
| 4 | `nprobe=0` 被接受 | entities/search | [#49823](https://github.com/milvus-io/milvus/issues/49823) | open, triage/accepted, milestone 2.6.18 | 90%+ |
| 5 | 未知字段被静默忽略 | 所有端点 | 无直接issue | — | 20-30% |
| 6 | `filter=""` 被接受（已修复） | entities/query | [#49844](https://github.com/milvus-io/milvus/issues/49844) | triage/accepted, 已在v2.6.16修复 | — |

**v2.6.x文档补充（2026-05-19）：**
- #49889 补充评论：v2.6.x List Collections 仍定义 dbName 为 "The name of an existing database"
- #49890 补充评论：v2.6.x 文档未列出 Request-Timeout header，但功能仍可用（v2.3.x→v2.4.x→v2.6.x 文档时间线对比表）

### 待修复工具链问题

1. ~~**verification_runner stderr判断bug**：classifier.rs第248行任何非空stderr判定为RetryableScriptError，导致102个候选缺陷全部被rejected~~ → **已修复 2026-05-19**：catch-all改为Pass，is_script_error()新增10种异常+traceback检测
2. ~~**MRE脚本endpoint路径错误**：verification生成的复现脚本使用了错误的endpoint路径~~ → **已修复 2026-05-19**：sandbox_runner.rs URL替换逻辑与infra.rs对齐，支持4种占位符格式

### 交接信息（2026-05-19）

**已完成：**
- Step 0-6 全部完成
- Milvus v2.6.16集成测试完成，6项真实缺陷已确认
- 2项缺陷(#49889, #49890)已提交到官方仓库并补充v2.6.x文档证据
- 资源释放机制正常工作（WSL2 memory=8GB, VHDX压缩到2.6GB）

**待执行：**
- Step 7: Full Cutover（条件性执行，需Shadow Mode验证）
- verification_runner修复（classifier.rs stderr判断逻辑）
- 跟踪已提交issue状态（#49889和#49890等待triage/accepted标签）
- Batch模式测试：`batch --target qdrant` 验证batch_runner模块
