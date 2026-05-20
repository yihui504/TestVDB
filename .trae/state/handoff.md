# TestVDB 交接信息

## 最后更新: 2026-05-20

## 项目状态: 缺陷类型多样性增强 — Step 1-5 代码完成，Step 2 验证运行中

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
| 1 | Endpoint 修复（方案A：openapi.rs path.clone()） | ✅ 已完成 |
| 2 | 验证确定性生成器产出（≥4种DefectKind） | 🔄 运行中（mine --skip-verify --max-rounds 2） |
| 3 | LLM 批次1 — P1(消息截断) + L1(prompt重写) + L2(工具集替换) | ✅ 已完成 |
| 4 | LLM 批次2 — L3(信息孤岛) + L4(死代码启用) | ✅ 已完成 |
| 5 | LLM 批次3 — P2(收敛判断) + P3(沙箱复用) | ✅ 已完成 |
| 6 | LLM 端到端验证（增量缺陷 + 覆盖多样性） | ⬜ 未开始 |

### 代码修改记录

**Step 1: openapi.rs**
- 第 128 行：`format!("{}_{}", method.to_lowercase(), path.replace('/', "_").trim_matches('_'))` → `path.clone()`
- 第 265 行：同上
- 效果：OpenAPI fallback 的 endpoint_name 从 `post__v2_vectordb_entities_search` 变为 `/v2/vectordb/entities/search`

**Step 3: LLM 编排器批次1**
- tools.rs：移除 `get_fuzz_boundary_values_tool()` 和 `get_fuzz_api_sequence_tool()`，新增 `get_execute_api_sequence_tool()` 和 `get_compare_endpoints_tool()`
- orchestrator.rs：
  - P1: 消息历史截断（max 20 messages）
  - L1: system prompt 从"边界值探索"重写为"状态序列探索+跨端点语义推理"
  - L2: 工具集替换（5个工具：execute_test_script, submit_mre, execute_api_sequence, compare_endpoints, get_coverage_report）
  - 移除 fuzz_context 构建代码（boundary_cases + sequence_cases）
  - 移除 BoundaryValueGenerator 和 APISequenceExplorer 导入

**Step 4: LLM 编排器批次2**
- orchestrator.rs：
  - L3: 新增 `batch_defects_summary` 字段 + `with_batch_defects()` builder + 初始消息注入
  - L4: 移除 `#[allow(dead_code)]`，`build_behavioral_section()` 在 `run()` 中调用并注入初始消息

**Step 5: LLM 编排器批次3**
- orchestrator.rs：
  - P2: 收敛判断（连续3轮无新assertion + turn>=5 → 提前终止）
  - P3: 沙箱复用修复（尊重 LLM 的 fresh_sandbox=true 请求，销毁旧沙箱创建新的）

### 验证运行状态

- 第一次运行（旧代码）：96 缺陷（95 ILLEGAL_SUCCESS + 1 DIFFERENTIAL_MISMATCH）
- 第二次运行（新代码）：进行中，Round 2 mutation 98/3103
- 预计完成时间：约 1-2 小时

### 关键风险

1. **方案 A 可能不够**：如果 mine 运行结果仍然是 2 种缺陷类型，需要升级为方案 B（ContractStore 规范化）
2. **state/metamorphic/sequence 生成器可能产出 0 缺陷**：即使 endpoint 匹配修复了，Milvus 可能在这些维度上确实没有 bug
3. **LLM 编排器优化尚未验证**：Step 6 需要完整运行 LLM 编排器来验证增量产出

---

## 前序任务完成记录

### 改进计划（8步）— 全部完成

| Step | 内容 | 状态 |
|------|------|------|
| 0 | 前置条件验证 | ✅ |
| 1 | 精简版架构重构 | ✅ |
| 2 | Qdrant 生成器完整实现 | ✅ |
| 3 | Milvus 侧代码质量修复 | ✅ |
| 4 | 闭环反馈修复 | ✅ |
| 5 | 完整架构重构 | ✅ |
| 6 | 端到端验证 | ✅ |
| 7 | Full Cutover | ⬜ 不建议执行 |

---

## 关键技术决策

1. **Endpoint规范化**：方案A先行（openapi.rs 1行改动），不够再升级方案B
2. **LLM编排器角色**：从"边界值探索"改为"状态序列探索+跨端点语义推理"
3. **新工具集**：execute_api_sequence(中层数据流) + compare_endpoints(语义等价对比)
4. **分批验证策略**：P1→L1+L2→L3+L4→P2+P3，每批验证后再继续
5. **收敛判断**：连续3轮无新assertion + turn>=5 → 提前终止
6. **沙箱复用**：尊重LLM的fresh_sandbox=true请求

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

1. **等待 mine 运行完成**，检查缺陷类型分布是否 ≥ 4 种
2. 如果 < 4 种，升级为方案 B（ContractStore 规范化）
3. **Step 6: LLM 端到端验证**（增量缺陷 + 覆盖多样性）
4. 跟踪已提交 issue 状态（#49928/#49929/#49930）
5. 更新 diversity-enhancement-plan.md 状态
