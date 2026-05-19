# TestVDB 交接信息

## 最后更新: 2026-05-19

## 项目状态: 缺陷类型多样性增强 — 计划已制定，待执行

---

## Plan/Spec 唯一存放目录

**`TestVDB/.trae/`** — 所有plan/spec只存放在此目录下

| 文件 | 路径 | 状态 |
|------|------|------|
| Plan（当前） | `.trae/plans/diversity-enhancement-plan.md` | IN PROGRESS |
| Plan（已完成） | `.trae/plans/testvdb-review-improvement-plan.md` | COMPLETED |
| Spec | `.trae/specs/deep-dive-review-previous-session-output.md` | ACTIVE |
| Trace | `.trae/specs/deep-dive-trace-review-previous-session-output.md` | ACTIVE |
| Handoff | `.trae/state/handoff.md` | 本文件 |

---

## 当前任务：缺陷类型多样性增强（5步）

| Step | 内容 | 状态 |
|------|------|------|
| 1 | Endpoint 规范化（方案B：ContractStore统一endpoint为URL路径格式） | ⬜ 未开始 |
| 2 | 验证确定性生成器产出（≥4种DefectKind，每种≥1个缺陷） | ⬜ 未开始 |
| 3 | infra.rs 错误处理修复（Err(_) => continue → warn日志） | ⬜ 未开始 |
| 4 | LLM 编排器优化（状态序列探索 + 跨端点语义推理） | ⬜ 未开始 |
| 5 | LLM 编排器端到端验证（增量产出 + 覆盖多样性） | ⬜ 未开始 |

### 根因诊断

**问题：** metamorphic/state_gen/sequence_gen/res/combo/conc 六个生成器全部零产出

**根因：** OpenAPI 解析器将 `/v2/vectordb/entities/search` 转为 `post__v2_vectordb_entities_search`（下划线分隔），合同 JSON 的 api_endpoint 为 `search+create_collection`（加号分隔），导致生成器中 `atc.endpoint.contains("entities/search")` 永远为 false

**修复方案：** 在 ContractStore 中增加规范化方法，统一 endpoint 为 URL 路径格式

### 验收标准

**确定性生成器：**
- Mine 产出 ≥ 4 种 DefectKind，每种至少 1 个缺陷
- 验证：`mine --target milvus --skip-verify`，v2.6.16

**LLM 编排器：**
- 验收 A：3 次运行中至少 2 次 LLM 产出 ≥ 1 个确定性生成器未发现的缺陷
- 验收 B：LLM 探索的 API 序列覆盖 ≥ 5 种不同状态转换模式

---

## 前序任务完成记录

### 改进计划（8步）— 全部完成

| Step | 内容 | 状态 |
|------|------|------|
| 0 | 前置条件验证（DefectKind+merge去重+3轮闭环Go/No-Go） | ✅ 已完成 |
| 1 | 精简版架构重构 — 提取 infra.rs | ✅ 已完成 |
| 2 | Qdrant 生成器完整实现（消除所有placeholder） | ✅ 已完成 |
| 3 | Milvus 侧代码质量修复（range/enum约束+SDK参数化+extract_context） | ✅ 已完成 |
| 4 | 闭环反馈修复（日志+收敛验证+fallback策略） | ✅ 已完成 |
| 5 | 完整架构重构（contract_loader/feedback_loop/verification_runner） | ✅ 已完成 |
| 6 | 端到端验证（Milvus+Qdrant+Shadow Mode） | ✅ 已完成 |
| 7 | Full Cutover | ⬜ 不建议执行（Shadow Mode结论：两种模式互补） |

---

## 关键技术决策

1. **Milvus错误码判断**：HTTP状态码始终200，必须检查`r.json().get('code')`
2. **Milvus认证头**：所有请求带`Authorization: Bearer root:Milvus`
3. **PERMISSIVE_PARSING**：Go JSON默认忽略未知字段，不是缺陷
4. **IDEMPOTENT_SUCCESS**：drop不存在资源返回成功是幂等行为，不是缺陷
5. **Endpoint规范化**：ContractStore统一使用URL路径格式（如`/v2/vectordb/entities/search`），生成器用`contains()`匹配
6. **LLM编排器角色**：从"边界值探索"改为"状态序列探索+跨端点语义推理"，与确定性生成器互补

---

## Shadow Mode 验证结果（2026-05-19）

### 验证配置
- 目标：Milvus v2.6.16 (milvusdb/milvus:v2.6.16)
- Mine命令：`testvdb.exe mine --target milvus --version 2.6.16 --contracts ./contracts --skip-verify --max-rounds 2 --shadow`
- Batch命令：`testvdb.exe batch --target milvus`

### 结果摘要

| 指标 | Batch（手写探针） | Mine（确定性生成器+LLM） |
|------|------------------|------------------------|
| 唯一缺陷数 | 24 | 96 |
| 缺陷类型数 | 4（ILLEGAL_SUCCESS/IDEMPOTENT_SUCCESS/PERMISSIVE_PARSING/SEQUENCE_VIOLATION） | 2（ILLEGAL_SUCCESS 95, DIFFERENTIAL_MISMATCH 1） |
| 端点覆盖 | ~15 精选高风险 | 45 全覆盖 |
| LLM编排器增量 | — | 0（12轮探索无新发现） |

### 核心结论
1. **Mine广度占优（4×），Batch深度占优（4种类型 vs 2种）**
2. **Mine零产出根因：endpoint格式不匹配，非Milvus行为正确**
3. **LLM编排器未产出增量价值**
4. **两种模式互补性显著，不建议Full Cutover**

### 产出文件
- `shadow_mode_results/shadow_mode_report.md` — 完整对比报告
- `shadow_mode_results/mine_defects.json` — Mine模式96个缺陷数据
- `shadow_mode_results/batch_baseline.json` — Batch模式24个缺陷基线
- `shadow_mode_results/filtered_real_defects.md` — 筛选后的真实缺陷清单

---

## v2.6.16 缺陷验证与提交记录（2026-05-19）

| 缺陷 | 验证结果 | 提交状态 |
|------|---------|---------|
| 缺陷3（重复ID count=-1） | NOT A BUG — insertCount=1, by design (#49849) | 不提交 |
| 缺陷4（create-drop-create维度丢失） | NOT REPRODUCED — 维度正确返回8 | 不提交 |
| P0-B（32768维OOM） | CONFIRMED | #49928 |
| P0-A（REST/SDK create_index不一致） | CONFIRMED | #49929 |
| P1-A（nprobe=-1） | CONFIRMED | #49823补充评论 |
| P1-B（collectionName=""） | NOT REPRODUCED | 不提交 |
| searchParams校验缺失 | CONFIRMED | #49930 |

### GitHub Issue状态
| # | 标题 | 状态 |
|---|------|------|
| #49823 | REST API v2 accepts nprobe=0 | open, triage/accepted, milestone 2.6.18 |
| #49928 | Default proxy.maxDimension=32768 too permissive | open, 待triage |
| #49929 | REST/SDK inconsistent default index creation | open, 待triage |
| #49930 | searchParams (ef/nprobe) validation gap | open, 待triage |

---

## 环境信息

### Milvus Docker
- docker-compose: docker-compose.milvus.yml
- 3容器: etcd + MinIO + milvus-standalone
- 端口: 19530
- Batch运行容器: testvdb-batch-milvus（已安装pymilvus）

### 编译状态
- cargo test: 255 passed, 0 failed, 1 ignored
- cargo build --release: 成功

---

## 下次开工

1. **Step 1**: Endpoint 规范化 — 修改 openapi.rs + store.rs + from_structured_contracts
2. **Step 2**: 验证确定性生成器产出（`mine --target milvus --skip-verify`）
3. **Step 3**: infra.rs 错误处理修复
4. **Step 4**: LLM 编排器优化（system prompt + 工具集）
5. **Step 5**: LLM 编排器端到端验证
6. 跟踪已提交 issue 状态（#49928/#49929/#49930 等待 triage）
