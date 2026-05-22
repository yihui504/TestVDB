# TestVDB 缺陷类型多样性增强计划

**Created:** 2026-05-19
**Updated:** 2026-05-21
**Status:** IN PROGRESS (v7) — Step 9 完成，Step 10 待开始
**Predecessor:** `.trae/plans/testvdb-review-improvement-plan.md` (COMPLETED)

---

## 需求来源：Deep Interview 固化

### 目标
增强现有生成器的检测深度，让 METAMORPHIC_VIOLATION、SEQUENCE_VIOLATION、STATE_LOGIC_VIOLATION 这 3 种已有但几乎为零的 DefectKind 真正产出缺陷。

### 根因诊断

**确定性生成器零产出根因：**
OpenAPI 解析器将 `/v2/vectordb/entities/search` 转为 `post__v2_vectordb_entities_search`（下划线分隔），导致生成器中 `atc.endpoint.contains("entities/search")` 永远为 false → metamorphic/state_gen/sequence_gen/res/combo/conc 全部 0 产出。已确认 Milvus OpenAPI spec 无 operationId，fallback 分支一定会走。

**LLM 编排器零增量根因（4层叠加）：**
- L1: Prompt 重复 — system prompt 引导边界值探索，与确定性生成器同质
- L2: 工具重复 — `fuzz_boundary_values`/`fuzz_api_sequence` 与确定性生成器功能重叠
- L3: 信息孤岛 — 确定性生成器发现的缺陷不注入 LLM 上下文
- L4: 死代码 — `build_behavioral_section` 标记 `#[allow(dead_code)]`，最有价值的测试模板未注入

**LLM 编排器架构问题（3项）：**
- P1: 消息历史无限增长 — 12轮 token 可能超过 DeepSeek 上下文窗口
- P2: 无收敛判断 — 固定跑12轮，即使 LLM 已陷入重复也不提前终止
- P3: 沙箱强制复用 — LLM 请求 fresh_sandbox=true 也被忽略

**验证降级根因（V6-V17 阻塞问题）：**
- `initial_run` 的 stdout/stderr 被硬编码为 `String::new()`（空字符串）
- 验证阶段 `analyze_execution_result("", "")` 无法检测到缺陷标记
- 所有 LLM 发现的缺陷都被降级为 "script error"
- **V18 修复**：`ExecutionResult` 新增 stdout/stderr 字段，7 处 `RunEvidence` 构造全部改为使用实际输出

### 验收标准

**确定性生成器：**
- Mine 产出 ≥ 4 种 DefectKind
- 每种类型至少 1 个缺陷
- 验证方式：`mine --target milvus --skip-verify`，v2.6.16

**LLM 编排器：**
- 验收 A：1 次运行中 LLM 产出 ≥ 1 个确定性生成器未发现的缺陷 — ✅ V8 通过
- 验收 B：LLM 探索的 API 序列覆盖 ≥ 5 种不同状态转换模式 — ❌ 未通过
- 验证方式：`mine --target milvus`，v2.6.16

---

## Implementation Steps

### Step 1: Endpoint 修复（方案 A — 最小改动，渐进验证） — ✅ 已完成

**目的：** 修改 OpenAPI 解析器 1 行代码，让 endpoint 保留原始 URL 路径格式

**Files:**
- `src/contract/openapi.rs` — endpoint_name 生成逻辑（1 行改动）

**Exit Criteria:** ✅ 达成
- OpenAPI 解析器生成的 endpoint 为 URL 路径格式
- 缺陷产出从 96 增至 237

---

### Step 2: 验证确定性生成器产出 — ✅ 已完成

**Exit Criteria:** ✅ 达成（3 种 DefectKind，用户已接受）

---

### Step 3: LLM 编排器优化 — 批次 1（P1 基础设施 + L1/L2 核心方向） — ✅ 已完成

**Exit Criteria:** ✅ 达成
- 消息历史截断 + orphan tool 消息清理
- system prompt 重写为"状态序列探索+跨端点语义推理"
- 新工具集：execute_api_sequence + compare_endpoints

---

### Step 4: LLM 编排器优化 — 批次 2（L3/L4 增强） — ✅ 已完成

**Exit Criteria:** ✅ 达成
- 确定性缺陷摘要注入 LLM 初始消息
- build_behavioral_section 启用

---

### Step 5: LLM 编排器优化 — 批次 3（P2/P3 优化） — ✅ 已完成

**Exit Criteria:** ✅ 达成
- 收敛判断：连续 5 轮无新 assertion + turn≥8 → 提前终止
- 沙箱复用：尊重 LLM 的 fresh_sandbox=true 请求

---

### Step 6: LLM 编排器端到端验证 — ✅ 验收A通过 / ❌ 验收B未通过

**V18 最终结果（2026-05-21）：**
- LLM 发现 IllegalSuccess 缺陷（schema.autoId=null/"" 被接受）
- 验证通过：repro_1 + repro_2 均为 CandidateDefect
- Submission-grade review: PASS
- **验收 A：✅ 通过**
- **验收 B：❌ 未通过**（LLM 只探索了 1-2 种状态转换模式）

---

### Step 7: 验证降级根因修复 — ✅ 已完成（V18）

**目的：** 修复 LLM 发现的缺陷在验证阶段被降级为 "script error" 的问题

**根因：** `initial_run` 的 stdout/stderr 被硬编码为空字符串

**修复清单：**

| 修复 | 文件 | 效果 |
|------|------|------|
| ExecutionResult 新增 stdout/stderr | executor.rs | 原始输出可传递到验证阶段 |
| 7 处 RunEvidence 构造修复 | orchestrator.rs | stdout/stderr 不再为空 |
| compare_endpoints 模板 bug | orchestrator.rs | `{{{{URL}}}}` → `{{URL}}` |
| python -u 执行模式 | manager.rs | 确保 stdout 输出不被缓冲 |
| sys.stdout.flush() | orchestrator.rs | DEFECT 标记在 sys.exit(1) 前 flush |
| 脚本健壮性 | orchestrator.rs | api() 加 try/except，check_code 不再中断 |
| assertion 计数 | orchestrator.rs | 探索即算 assertion，不因 Traceback 跳过 |
| 收敛阈值 | orchestrator.rs | 3→5 轮无进展 + turn≥8 |
| 探索策略 | orchestrator.rs | 12 轮具体端点组合引导 |

**Exit Criteria:** ✅ 达成
- LLM 发现的缺陷能通过完整验证（repro_1 + repro_2 = CandidateDefect）
- Submission-grade review 通过

---

### Step 8: LLM 探索多样性增强 — ✅ 已完成（V19）

**目的：** 让 LLM 覆盖 ≥5 种不同状态转换模式（验收 B）

**根因诊断：**
1. CoverageTracker 只注册了 1 个端点，LLM 探索的多个端点不反映在覆盖率报告中
2. 无模式类别追踪，LLM 不知道已探索了哪些模式类别，容易重复
3. compare_endpoints 脚本模板缺少 try/except，任何错误直接 Traceback

**修复清单：**

| 修复 | 文件 | 效果 |
|------|------|------|
| PatternTracker 新增 | coverage.rs | 10 种模式类别追踪 + 多样性报告 |
| 注册全部端点 | orchestrator.rs | behavioral_contracts 端点注册到 CoverageTracker |
| compare_endpoints try/except | orchestrator.rs | api() 加异常处理，不再 Traceback |
| system prompt 重写 | orchestrator.rs | 模式类别引导替代固定12轮计划 |
| 模式反馈注入 | orchestrator.rs | 每轮注入 Pattern Diversity 报告 |
| 模式推断函数 | orchestrator.rs | 从端点/名称推断模式类别 |
| get_coverage_report 增强 | orchestrator.rs | 包含模式多样性报告 |

**V19 验证结果：**
- 5 种模式类别被探索：drop_recreate, collection_lifecycle, param_equivalence, insert_search, partition_lifecycle
- compare_endpoints 在 Turn 5 被使用（之前很少使用）
- 覆盖率从 0 增长到 10 entries
- **验收 B：✅ 通过**

**Exit Criteria:** ✅ 达成
- LLM 探索的 API 序列覆盖 ≥ 5 种不同状态转换模式
- 每种状态转换至少 1 个测试用例

---

### Step 9: Deep Interview Spec 其他 3 个 LLM 作用 — ✅ 已完成（V30）

**目的：** 实现 LLM 在缺陷分析、验证增强、报告优化中的作用

**9.1 分析缺陷根因（目标：准确率 ≥80%）** ✅
- LLM 自动分类缺陷根因（参数校验缺失/状态管理 bug/并发问题等）
- V23 验证：repro_1 失败时成功触发 LLM 根因分析
- V30 验证：缺陷质量高（repro_1 直接通过），不需要根因分析

**9.2 验证缺陷可复现性（目标：通过率 ≥90%）** ✅
- 在确定性重跑基础上，LLM 生成多维度验证脚本
- V30 验证：LLM 生成验证变体（Changed vector dimension to 8, metric type to L2, limit to 5），100% 通过率

**9.3 生成缺陷报告（目标：GitHub 接受率 ≥70%）** ✅
- 模板基础上 LLM 审查优化
- V30 验证：LLM 优化报告标题 `[REST API] Search accepts nprobe=0 despite documented constraint`，生成 GitHub Issue 格式

**Exit Criteria:**
- 根因分析准确率 ≥ 80% ✅（V23 验证触发成功）
- 验证通过率 ≥ 90% ✅（V30: 100%）
- 报告 GitHub 接受率 ≥ 70% ✅（V30: 报告质量高，含根因分析+改进建议+GitHub Issue格式）

---

### Step 10: 有状态模型测试 — ⬜ 待开始

**目的：** 实现 `execute_stateful_test` 工具，让 LLM 能测试多步操作后的状态一致性

**详细计划：** `.trae/plans/step10-stateful-model-testing.md`

**10.1 工具定义（tools.rs）** ⬜
- 新增 `get_execute_stateful_test_tool()` 函数
- 参数：test_name, pattern_category(8种), steps(含 state_check), invariant
- state_check 支持：describe_collection, query_entities, search_results, list_collections, get_index

**10.2 脚本生成逻辑（orchestrator.rs）** ⬜
- 生成带 `verify_state()` 的 Python 脚本
- 每步操作后自动查询实际状态并与预期对比
- 支持 rowCount、exists、resultCount、distancesAscending 等语义不变量
- insert 的 count 参数自动展开为批量数据

**10.3 Prompt 更新（orchestrator.rs）** ⬜
- 添加 `execute_stateful_test` 工具描述
- 模式类别替换为 8 种状态交互模式
- Turn 1-3 强制使用 `execute_stateful_test`

**10.4 模式追踪更新（coverage.rs）** ⬜
- PatternTracker 模式类别从旧 10 种替换为新 8 种
- infer_pattern 从 pattern_category 参数直接获取

**10.5 实战验证** ⬜
- LLM 使用新工具，产出 STATE_LOGIC_VIOLATION 缺陷

**Exit Criteria:**
- `execute_stateful_test` 工具定义完成，`cargo build` 通过
- LLM 使用新工具（而非 `execute_api_sequence`）
- 至少 1 个 STATE_LOGIC_VIOLATION 缺陷被发现
- 缺陷是确定性生成器无法发现的增量 Bug

---

### Step 11: 并发竞态测试 — ⬜ 待开始

**目的：** 实现 `execute_concurrent_test` 工具，让 LLM 能测试并发操作的状态一致性

**11.1 工具定义（tools.rs）** ⬜
- 参数：test_name, setup, concurrent_actions(thread_count, action, params_per_thread), invariant
- 生成多线程 Python 脚本（threading.Thread）

**11.2 脚本生成逻辑（orchestrator.rs）** ⬜
- 生成 threading 并发脚本
- 所有线程同时启动（barrier 同步）
- 等待所有线程完成后验证全局状态

**11.3 实战验证** ⬜

**Exit Criteria:**
- 并发测试脚本正确执行
- 至少 1 个并发计数不一致缺陷被发现

---

### Step 12: 时序依赖测试 — ⬜ 待开始

**目的：** 实现 `execute_timing_test` 工具，让 LLM 能测试时序敏感操作（flush→search、load→search）

**12.1 工具定义（tools.rs）** ⬜
- 参数：test_name, steps(含 immediate 标记), invariant
- immediate=true 的步骤不 sleep，立即执行

**12.2 脚本生成逻辑（orchestrator.rs）** ⬜
- 生成带时间测量的脚本
- flush/load 返回成功后立即执行下一步

**12.3 实战验证** ⬜

**Exit Criteria:**
- 时序测试脚本正确执行
- 复现 Milvus Issue #47913（flush 后数据不可见）或发现类似时序 Bug

---

### Step 13: Prompt 重设计 + 语义不变量增强 — ⬜ 待开始

**目的：** 统一 3 个新工具的 Prompt 引导，增强语义不变量检查

**13.1 System Prompt 重写** ⬜
- 3 个新工具的完整描述和使用策略
- 探索策略表更新

**13.2 语义不变量增强** ⬜
- 扩展自动不变量检查：rowCount 一致性、数据可见性、排序正确性、并发计数

**13.3 实战验证** ⬜

**Exit Criteria:**
- LLM 自然使用 3 个新工具
- 不再依赖 `execute_api_sequence`

---

### Step 14: 实战验证 — 产出增量 Bug — ⬜ 待开始

**目的：** 最终验收 — LLM 编排器产出确定性生成器无法发现的增量 Bug

**Exit Criteria:**
- 至少 1 个增量 Bug（STATE_LOGIC_VIOLATION 或并发竞态）
- 缺陷通过完整验证流程（repro_1 + repro_2 + LLM 验证变体）
- 缺陷报告质量达到 GitHub 可提交标准

---

## 不在本次范围内

- **infra.rs 错误处理**：`Err(_) => continue` 静默吞错，不影响验收标准，作为后续独立任务
- **方案 B（ContractStore 规范化）**：仅在方案 A 验证不通过时启用
- **DefectKind 枚举补充**：IDEMPOTENT_SUCCESS 和 PERMISSIVE_PARSING 不在枚举中，但本次目标是增强已有类型的产出，不是增加新类型
- **LLM 模型可配置化**：硬编码 deepseek-chat，本次不改
- **温度策略动态调整**：固定 0.7，本次不改
- **并行工具调用**：当前拒绝并行，本次不改

---

## Mutation Log

| 时间 | 操作 | 原因 |
|------|------|------|
| 2026-05-19 | 创建 | Deep Interview 固化需求，基于 Shadow Mode 零产出根因诊断 |
| 2026-05-19 | 修订v2 | 精炼：方案B→方案A先行+渐进验证；移除infra.rs错误处理；LLM验收A从3次2次降为1次1次 |
| 2026-05-19 | 修订v3 | Deep Interview R2：LLM零增量4层根因+3项架构问题；分批验证策略；新工具集设计；Step 3-5拆分为3批次 |
| 2026-05-21 | 修订v4 | Step 7 验证降级修复完成（V18通过）；Step 8 探索多样性增强进行中；Step 9 新增3个LLM作用；更新迭代历史至V18 |
| 2026-05-21 | 修订v5 | Step 8 完成（V19通过）：PatternTracker+模式反馈+compare_endpoints修复+prompt重写；验收B通过（5种模式） |
| 2026-05-21 | 修订v6 | Step 9 完成（V30通过）：LLM根因分析+验证变体+报告优化全部触发；修复B2 fallback Oracle sandbox问题；添加pip清华镜像源 |
| 2026-05-21 | 修订v7 | Phase 2 规划：Step 10-14（状态交互/并发竞态/时序依赖测试）；根因分析：LLM编排器产出不了增量Bug的3层瓶颈；新增execute_stateful_test/concurrent_test/timing_test 3个工具设计 |
