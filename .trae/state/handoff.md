# TestVDB 交接信息

## 最后更新: 2026-05-19

## 项目状态: 缺陷类型多样性增强 — 计划已精炼，待执行

---

## Plan/Spec 唯一存放目录

**`TestVDB/.trae/`** — 所有plan/spec只存放在此目录下

| 文件 | 路径 | 状态 |
|------|------|------|
| Plan（当前） | `.trae/plans/diversity-enhancement-plan.md` | IN PROGRESS (v3) |
| Plan（已完成） | `.trae/plans/testvdb-review-improvement-plan.md` | COMPLETED |
| Spec | `.trae/specs/deep-dive-review-previous-session-output.md` | ACTIVE |
| Trace | `.trae/specs/deep-dive-trace-review-previous-session-output.md` | ACTIVE |
| Handoff | `.trae/state/handoff.md` | 本文件 |

---

## 当前任务：缺陷类型多样性增强（6步）

| Step | 内容 | 状态 |
|------|------|------|
| 1 | Endpoint 修复（方案A：openapi.rs 1行改动） | ⬜ 未开始 |
| 2 | 验证确定性生成器产出（≥4种DefectKind，每种≥1个缺陷） | ⬜ 未开始 |
| 3 | LLM 批次1 — P1(消息截断) + L1(prompt重写) + L2(工具集替换) | ⬜ 未开始 |
| 4 | LLM 批次2 — L3(信息孤岛) + L4(死代码启用) | ⬜ 未开始 |
| 5 | LLM 批次3 — P2(收敛判断) + P3(沙箱复用) | ⬜ 未开始 |
| 6 | LLM 端到端验证（增量缺陷 + 覆盖多样性） | ⬜ 未开始 |

### 根因诊断

**确定性生成器零产出根因：**
OpenAPI 解析器将 `/v2/vectordb/entities/search` 转为 `post__v2_vectordb_entities_search`（下划线分隔），导致 `atc.endpoint.contains("entities/search")` 永远为 false。已确认 Milvus OpenAPI spec 无 operationId，fallback 分支一定会走。

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
- Mine 产出 ≥ 4 种 DefectKind，每种至少 1 个缺陷
- 验证：`mine --target milvus --skip-verify`，v2.6.16

**LLM 编排器：**
- 验收 A：1 次运行中 LLM 产出 ≥ 1 个确定性生成器未发现的缺陷
- 验收 B：LLM 探索的 API 序列覆盖 ≥ 5 种不同状态转换模式

### 新工具集设计

| 工具 | 类型 | 描述 |
|------|------|------|
| `execute_test_script` | 保留 | LLM 自由探索，直接写 Python 脚本 |
| `execute_api_sequence` | 新增 | 中层数据流：LLM 描述每步端点+参数+期望状态，工具自动生成脚本 |
| `compare_endpoints` | 新增 | 语义等价对比：LLM 描述"两个操作语义上应该等价"，工具分别执行并对比 |
| `submit_mre` | 保留 | 提交最小可复现示例 |
| `get_coverage_report` | 保留 | 返回 API 覆盖率报告 |
| ~~`fuzz_boundary_values`~~ | 移除 | 与确定性 boundary 生成器重复 |
| ~~`fuzz_api_sequence`~~ | 移除 | 与确定性 sequence 生成器重复 |

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
5. **Endpoint规范化**：方案A先行（openapi.rs 1行改动），不够再升级方案B
6. **LLM编排器角色**：从"边界值探索"改为"状态序列探索+跨端点语义推理"
7. **新工具粒度**：execute_api_sequence 用中层数据流，compare_endpoints 用语义等价对比
8. **分批验证策略**：P1→L1+L2→L3+L4→P2+P3，每批验证后再继续

---

## Shadow Mode 验证结果（2026-05-19）

### 结果摘要

| 指标 | Batch（手写探针） | Mine（确定性生成器+LLM） |
|------|------------------|------------------------|
| 唯一缺陷数 | 24 | 96 |
| 缺陷类型数 | 4 | 2（ILLEGAL_SUCCESS 95, DIFFERENTIAL_MISMATCH 1） |
| 端点覆盖 | ~15 | 45 |
| LLM编排器增量 | — | 0 |

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

### 编译状态
- cargo test: 255 passed, 0 failed, 1 ignored
- cargo build --release: 成功

---

## 下次开工

1. **Step 1**: Endpoint 修复 — openapi.rs 1行改动（path.clone()）
2. **Step 2**: 验证确定性生成器产出（`mine --target milvus --skip-verify`）
3. **Step 3**: LLM 批次1 — P1(消息截断) + L1(prompt) + L2(工具集)
4. **Step 4**: LLM 批次2 — L3(信息孤岛) + L4(死代码启用)
5. **Step 5**: LLM 批次3 — P2(收敛判断) + P3(沙箱复用)
6. **Step 6**: 端到端验证
7. 跟踪已提交 issue 状态
