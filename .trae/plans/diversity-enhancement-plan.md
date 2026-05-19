# TestVDB 缺陷类型多样性增强计划

**Created:** 2026-05-19
**Updated:** 2026-05-19
**Status:** IN PROGRESS
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

### 验收标准

**确定性生成器：**
- Mine 产出 ≥ 4 种 DefectKind
- 每种类型至少 1 个缺陷
- 验证方式：`mine --target milvus --skip-verify`，v2.6.16

**LLM 编排器：**
- 验收 A：1 次运行中 LLM 产出 ≥ 1 个确定性生成器未发现的缺陷
- 验收 B：LLM 探索的 API 序列覆盖 ≥ 5 种不同状态转换模式
- 验证方式：`mine --target milvus --skip-verify`，v2.6.16

---

## Implementation Steps

### Step 1: Endpoint 修复（方案 A — 最小改动，渐进验证）

**目的：** 修改 OpenAPI 解析器 1 行代码，让 endpoint 保留原始 URL 路径格式

**Files:**
- `src/contract/openapi.rs` — endpoint_name 生成逻辑（1 行改动）

**Tasks:**

1.1. `openapi.rs`: 修改 endpoint_name fallback 逻辑
- 修改前：`format!("{}_{}", method.to_lowercase(), path.replace('/', '_').trim_matches('_'))`
- 修改后：`path.clone()` 保留原始路径如 `/v2/vectordb/entities/search`
- 注意：当 operationId 存在时仍使用 operationId（不改动此分支）

1.2. 编译验证：
```bash
cargo build --release
cargo test
```

**Exit Criteria:**
- OpenAPI 解析器生成的 endpoint 为 URL 路径格式
- cargo test 全部通过

---

### Step 2: 验证确定性生成器产出

**目的：** 修复后验证 metamorphic/state_gen/sequence_gen 是否真正产出缺陷

**Tasks:**

2.1. 在 `run_mine` 中 `build_contract_store` 之后，打印前几条 type_constraints 的 endpoint 值，确认格式正确

2.2. 运行 Mine 模式验证：
```bash
cargo run --release -- mine --target milvus --version 2.6.16 --contracts ./contracts --skip-verify --max-rounds 2
```

2.3. 检查结果：
- 缺陷类型数 ≥ 4？→ 达标，Step 1 完成
- 缺陷类型数 < 4？→ 进入 Step 2B

2.4. **Step 2B（条件性）**：如果方案 A 不够，升级为方案 B
- 在 ContractStore 中增加规范化方法
- 处理合同 JSON 的 `search+create_collection` 拆分问题
- 重新验证

**Exit Criteria:**
- Mine 产出 ≥ 4 种 DefectKind
- 每种类型至少 1 个缺陷

---

### Step 3: LLM 编排器优化 — 批次 1（P1 基础设施 + L1/L2 核心方向）

**目的：** 修复 token 爆炸风险 + 重写 prompt 和工具集，让 LLM 做确定性生成器无法覆盖的探索

**Files:**
- `src/agent/orchestrator.rs` — system prompt + 工具集 + 消息管理
- `src/agent/tools.rs` — 工具定义

**Tasks:**

3.1. **P1: 消息历史截断**
- 实现滑动窗口：保留 system prompt + 最近 N 轮消息（N=6）
- 对工具输出做长度限制（截断超过 2000 字符的输出）
- 保留关键信息：缺陷发现、覆盖率变化、错误模式

3.2. **L1: 重写 system prompt**
- 移除"边界值探索"和"序列探索"引导
- 新增"状态序列探索"引导：构造复杂多步 API 序列（create→insert→index→drop→create→search），发现状态不一致
- 新增"跨端点语义推理"引导：发现 REST vs SDK 行为差异、跨端点状态不一致
- 新增探索策略：T1-2 状态序列、T3-4 跨端点对比、T5+ 自由探索

3.3. **L2: 工具集替换**
- 移除：`fuzz_boundary_values`、`fuzz_api_sequence`
- 新增：`execute_api_sequence` — LLM 声明式描述多步 API 序列，工具自动生成脚本并执行
- 新增：`compare_endpoints` — LLM 指定两个端点和相同输入，工具自动对比响应差异
- 保留：`execute_test_script`（LLM 自由探索）、`submit_mre`、`get_coverage_report`
- 最终工具集：`execute_test_script`、`execute_api_sequence`、`compare_endpoints`、`submit_mre`、`get_coverage_report`

**Verification:**
```bash
cargo build --release
cargo test
```

**批次间验证：** 运行 Mine 模式，确认 LLM 不再重复边界值探索，开始产出状态序列/跨端点对比测试

**Exit Criteria:**
- 消息历史不超过 N 轮
- system prompt 包含状态序列探索和跨端点语义推理引导
- 新工具集可用，旧工具已移除
- cargo test 全部通过

---

### Step 4: LLM 编排器优化 — 批次 2（L3/L4 增强）

**目的：** 让 LLM 能利用确定性生成器的发现和 Contract 的行为约束

**Files:**
- `src/agent/orchestrator.rs` — 信息注入 + 死代码启用
- `src/commands.rs` — 确定性缺陷传递

**Tasks:**

4.1. **L3: 信息孤岛修复**
- 在 `run_mine` 中，将确定性生成器发现的缺陷摘要注入 LLM 的初始消息
- 格式："确定性生成器已发现以下缺陷类型：[ILLEGAL_SUCCESS x95, DIFFERENTIAL_MISMATCH x1]。请专注于发现其他类型的缺陷。"
- 避免重复：LLM 产出与确定性方法同质缺陷时，不重复计数

4.2. **L4: 死代码启用**
- 启用 `build_behavioral_section()`，将 behavioral contracts 的脚本模板注入 system prompt
- 移除 `#[allow(dead_code)]` 标记
- 让 LLM 参考 Contract 中的行为约束来设计测试

**Verification:**
```bash
cargo build --release
cargo test
```

**批次间验证：** 运行 Mine 模式，确认 LLM 能利用确定性缺陷信息和行为约束

**Exit Criteria:**
- LLM 初始消息包含确定性缺陷摘要
- `build_behavioral_section` 被调用，输出注入 prompt
- cargo test 全部通过

---

### Step 5: LLM 编排器优化 — 批次 3（P2/P3 优化）

**目的：** 收敛判断 + 沙箱复用修复

**Files:**
- `src/agent/orchestrator.rs` — 收敛逻辑 + 沙箱管理

**Tasks:**

5.1. **P2: 收敛判断**
- 实现 `consecutive_no_defect` 计数器：连续 N 轮无新发现则提前终止（N=3）
- 利用 `coverage_ratio`：覆盖率 > 80% 且无新缺陷则终止
- 终止时自动提交最后发现的缺陷（如有）

5.2. **P3: 沙箱复用修复**
- 允许 LLM 在请求 `fresh_sandbox=true` 时销毁旧沙箱创建新沙箱
- 添加速率限制：每 N 轮最多 1 次重建（N=3）
- 重建时记录日志

**Verification:**
```bash
cargo build --release
cargo test
```

**Exit Criteria:**
- 连续 3 轮无新发现时提前终止
- LLM 可请求新沙箱（有速率限制）
- cargo test 全部通过

---

### Step 6: LLM 编排器端到端验证

**目的：** 验证 LLM 编排器的增量产出和覆盖多样性

**Tasks:**

6.1. 运行 Mine 模式：
```bash
cargo run --release -- mine --target milvus --version 2.6.16 --contracts ./contracts --skip-verify --max-rounds 2
```

6.2. 验收 A：LLM 产出 ≥ 1 个确定性生成器未发现的缺陷

6.3. 验收 B：LLM 探索的 API 序列覆盖 ≥ 5 种不同状态转换模式

**Exit Criteria:**
- 验收 A 和验收 B 均满足

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
