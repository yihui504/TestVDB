# Milvus 2.6.16 Shadow Mode 验证对比报告

**生成时间：** 2026-05-19  
**目标版本：** milvusdb/milvus:v2.6.16

---

## 1. 验证概述

### 1.1 目标

通过 Shadow Mode 对比两种测试策略在 Milvus 2.6.16 上的缺陷发现能力：
- **Batch 模式**：手写探针 SafetyNets，基于领域知识编写针对性测试
- **Mine 模式**：确定性测试生成器（合同驱动）+ LLM 编排器，自动从 OpenAPI 合同推导违规场景并执行反馈循环

### 1.2 方法

| 维度 | Batch 模式 | Mine 模式 |
|------|-----------|----------|
| 测试来源 | 手写 SafetyNet 探针 | OpenAPI 合同自动推导 + LLM 智能探索 |
| 合同基础 | 无（纯领域知识） | 45 端点、434 类型约束、10 范围约束、40 必填参数 |
| 生成策略 | 人工设计 | 3 策略（boundary/mutation/diff），1336 违规场景 |
| 反馈循环 | 无 | 2 轮（Round 1: 513 缺陷 → Round 2: 943 缺陷） |
| LLM 编排 | 无 | 12 轮 Agentic Exploration + 20 Oracle 检查 + 65 SafetyNet 探针 |
| 执行环境 | Docker 沙箱 | Docker 沙箱（每轮独立） |

---

## 2. Batch 模式结果摘要

| 指标 | 数值 |
|------|------|
| 总探针数 | 205 |
| 通过 | 163 |
| 缺陷（总） | 42 |
| 缺陷（唯一） | 24 |
| 错误 | 0 |
| 通过率 | 79.5% |
| 缺陷率 | 20.5% |

### 2.1 缺陷类型分布

| 缺陷类型 | 数量 | 占比 |
|----------|------|------|
| ILLEGAL_SUCCESS | 16 | 66.7% |
| IDEMPOTENT_SUCCESS | 4 | 16.7% |
| PERMISSIVE_PARSING | 2 | 8.3% |
| SEQUENCE_VIOLATION | 2 | 8.3% |

### 2.2 缺陷清单

| # | 名称 | 类型 | 描述 |
|---|------|------|------|
| 1 | search_nprobe_zero | ILLEGAL_SUCCESS | nprobe=0 被接受 |
| 2 | search_nprobe_negative | ILLEGAL_SUCCESS | nprobe=-1 被接受 |
| 3 | duplicate_collection | ILLEGAL_SUCCESS | 重复集合名被接受 |
| 4 | drop_nonexistent_index | IDEMPOTENT_SUCCESS | 删除不存在索引成功 |
| 5 | drop_nonexistent_partition | IDEMPOTENT_SUCCESS | 删除不存在分区成功 |
| 6 | alter_invalid_ttl | ILLEGAL_SUCCESS | 负数 TTL 被接受 |
| 7 | get_empty_ids | ILLEGAL_SUCCESS | 空 ID 数组被接受 |
| 8 | get_nonexistent_ids | ILLEGAL_SUCCESS | 不存在实体 ID 被接受 |
| 9 | drop_nonexistent_database | IDEMPOTENT_SUCCESS | 删除不存在数据库成功 |
| 10 | drop_nonexistent_collection | IDEMPOTENT_SUCCESS | 删除不存在集合成功 |
| 11 | coll_list_empty_db | ILLEGAL_SUCCESS | 空 dbName 列集合被接受 |
| 12 | coll_has_nonexistent | ILLEGAL_SUCCESS | 查询不存在集合返回成功 |
| 13 | part_has_nonexistent | ILLEGAL_SUCCESS | 查询不存在分区返回成功 |
| 14 | alias_list_empty_name | ILLEGAL_SUCCESS | 空 collectionName 列别名被接受 |
| 15 | db_list_invalid_param | ILLEGAL_SUCCESS | 无效参数列数据库被接受 |
| 16 | create_mutation_oversized_dimension | ILLEGAL_SUCCESS | 超大维度被接受 |
| 17 | create_mutation_unknown_param | PERMISSIVE_PARSING | 未知参数被接受 |
| 18 | create_mutation_extra_fields | PERMISSIVE_PARSING | 多余字段被接受 |
| 19 | insert_mutation_oversized_data | ILLEGAL_SUCCESS | 超大数据被接受 |
| 20 | insert_mutation_boundary_float_data | ILLEGAL_SUCCESS | 边界浮点数据被接受 |
| 21 | query_mutation_null_injection_filter | ILLEGAL_SUCCESS | Null 注入过滤器被接受 |
| 22 | sequence_6 | SEQUENCE_VIOLATION | 重复 ID 插入计数不一致 |
| 23 | state_create_drop_create_dim | SEQUENCE_VIOLATION | 重建集合后维度丢失 |
| 24 | resource_large_dimension | ILLEGAL_SUCCESS | 32768 维集合创建（可能 OOM） |

---

## 3. Mine 模式结果摘要

| 指标 | 数值 |
|------|------|
| 唯一缺陷 | 96 |
| 生成场景总数 | 1336 |
| 违规目标 | 1243 |

### 3.1 策略维度分布

| 策略 | 生成场景数 | 唯一缺陷数 |
|------|-----------|-----------|
| boundary | 1024 | 85 |
| mutation | 430 | 10 |
| diff | 2 | 1 |
| state | 0 | 0 |
| meta | 0 | 0 |
| seq | 0 | 0 |
| res | 0 | 0 |
| combo | 0 | 0 |
| conc | 0 | 0 |

### 3.2 缺陷类型分布

| 缺陷类型 | 数量 | 占比 |
|----------|------|------|
| ILLEGAL_SUCCESS | 95 | 98.96% |
| DIFFERENTIAL_MISMATCH | 1 | 1.04% |

### 3.3 反馈循环详情

| 轮次 | 合同快照 | 发现缺陷 | 新观察 |
|------|---------|---------|--------|
| Round 1 | 434 类型约束 + 10 范围约束 | 513 | 86 |
| Round 2 | 520 类型约束 + 10 范围约束 + 86 观察行为 | 943 | — |

### 3.4 LLM 编排器结果

| 指标 | 数值 |
|------|------|
| 探索轮次 | 12 |
| Oracle 检查项 | 20 |
| SafetyNet 探针 | 65 |
| Oracle 发现违规 | 2（search_nprobe_positive、create_no_duplicate） |
| **新发现缺陷** | **0** |

LLM 编排器发现的 2 个违规（nprobe=0 被接受、重复集合名被接受）均为确定性生成器已覆盖的已知缺陷，未产出增量发现。

### 3.5 Mine 缺陷分类概览

**Boundary 类（85 个）** — 系统性参数边界测试：

| 参数类别 | 测试模式 | 示例 |
|----------|---------|------|
| 数值参数下界/零/负数 | below_min / zero / negative | offset=0, nprobe=-1, dim=0, nlist=-1, efconstruction=0, rerank=-1 |
| 字符串参数空值 | empty_string | dbName="", partitionName="", filter="", userName="" |
| 类型混淆 | float_type / string_type | Request-Timeout=3.5, Request-Timeout="abc" |
| 必填参数缺失 | missing_required | fields_missing, indexParams_missing |
| 嵌套参数 | params.* / schema.* / searchParams.* | params.consistencyLevel="", searchParams.radius="abc" |

**Mutation 类（10 个）** — 变异测试：

| 名称 | 描述 |
|------|------|
| create_index_null | create_index 传 null |
| create_index_oversized | create_index 传超大值 |
| create_index_type_confusion | create_index 类型混淆 |
| _v2_vectordb_create_index_unknown_param | create_index 未知参数 |
| _v2_vectordb_create_index_extra_fields | create_index 多余字段 |
| unknown_null / unknown_oversized / unknown_type_confusion | 通用变异 |
| _unknown_unknown_param / _unknown_extra_fields | 通用变异 |

**Diff 类（1 个）**：

| 名称 | 描述 |
|------|------|
| diff_create_index | DIFFERENTIAL_MISMATCH: REST 成功但 SDK 失败 |

---

## 4. 对比分析

### 4.1 缺陷发现数量对比

| 指标 | Batch | Mine | 倍率 |
|------|-------|------|------|
| 唯一缺陷数 | 24 | 96 | 4.0× |
| 总探针/场景数 | 205 | 1336 | 6.5× |
| 缺陷发现效率（缺陷/探针） | 11.7% | 7.2% | 0.6× |

Mine 模式在绝对数量上以 4 倍优势领先，但单探针效率低于 Batch。Batch 模式凭借领域知识实现了更高的命中率。

### 4.2 缺陷类型分布对比

| 缺陷类型 | Batch | Mine | 分析 |
|----------|-------|------|------|
| ILLEGAL_SUCCESS | 16 (66.7%) | 95 (98.96%) | Mine 高度集中于非法成功类 |
| IDEMPOTENT_SUCCESS | 4 (16.7%) | 0 | **Batch 独有** — 幂等性违规 |
| PERMISSIVE_PARSING | 2 (8.3%) | 0 | **Batch 独有** — 宽松解析 |
| SEQUENCE_VIOLATION | 2 (8.3%) | 0 | **Batch 独有** — 时序违规 |
| DIFFERENTIAL_MISMATCH | 0 | 1 (1.04%) | **Mine 独有** — REST/SDK 行为不一致 |

**关键发现：** Mine 模式的缺陷类型极度集中于 ILLEGAL_SUCCESS（99%），缺乏语义层面的多样性。Batch 模式虽然数量少，但覆盖了 4 种不同缺陷类型，揭示了更深层的系统行为异常。

### 4.3 端点覆盖率对比

| 覆盖维度 | Batch | Mine |
|----------|-------|------|
| 涉及端点数 | ~15 | 45（全覆盖） |
| 覆盖方式 | 精选高风险端点 | 系统性遍历所有端点 |
| 参数覆盖 | 关键参数 | 全参数（434 类型约束 + 10 范围约束） |

Mine 模式实现了 45 个端点的全覆盖，而 Batch 模式聚焦于约 15 个高风险端点。Mine 的系统性覆盖确保了无遗漏，但大量缺陷集中在 search 端点的参数注入（Milvus 对 search 请求中的额外参数采取静默忽略策略），属于同一根因的重复表现。

### 4.4 Mine 独有发现 vs Batch 独有发现

#### Mine 独有发现（约 78 个）

Mine 独有发现主要集中在以下几类：

1. **Search 端点参数静默接受（~60 个）**
   - 根因：Milvus search 端点对请求体中的额外参数（offset=0, dim=-1, nlist=0, efconstruction=-1, for=0 等）不做校验，静默忽略而非拒绝
   - 本质上是同一漏洞（参数过滤缺失）在不同参数名上的重复表现

2. **跨端点空字符串接受（~12 个）**
   - dbName=""、partitionName=""、Authorization=""、password=""、userName="" 等
   - 多个端点对空字符串参数缺乏校验

3. **类型混淆（~6 个）**
   - Request-Timeout=3.5（float 替代 int）、Request-Timeout="abc"（string 替代 int）
   - searchParams.radius/range_filter 类型混淆

4. **DIFFERENTIAL_MISMATCH（1 个）**
   - diff_create_index：REST API 成功但 PyMilvus SDK 失败，揭示客户端/服务端行为不一致

#### Batch 独有发现（约 8 个）

| 缺陷 | 类型 | 价值评估 |
|------|------|---------|
| duplicate_collection | ILLEGAL_SUCCESS | 高 — 重复创建集合应被拒绝 |
| drop_nonexistent_index | IDEMPOTENT_SUCCESS | 中 — 删除不存在资源应返回错误 |
| drop_nonexistent_partition | IDEMPOTENT_SUCCESS | 中 |
| drop_nonexistent_database | IDEMPOTENT_SUCCESS | 中 |
| drop_nonexistent_collection | IDEMPOTENT_SUCCESS | 中 |
| get_empty_ids / get_nonexistent_ids | ILLEGAL_SUCCESS | 中 — 查询空/不存在 ID 应返回错误 |
| sequence_6 | SEQUENCE_VIOLATION | 高 — 重复 ID 插入计数不一致 |
| state_create_drop_create_dim | SEQUENCE_VIOLATION | 高 — 状态转换后维度丢失 |
| query_mutation_null_injection_filter | ILLEGAL_SUCCESS | 高 — Null 注入安全风险 |
| resource_large_dimension | ILLEGAL_SUCCESS | 高 — 32768 维可能 OOM |
| insert_mutation_oversized_data | ILLEGAL_SUCCESS | 中 |
| insert_mutation_boundary_float_data | ILLEGAL_SUCCESS | 中 |
| coll_list_empty_db / coll_has_nonexistent / part_has_nonexistent | ILLEGAL_SUCCESS | 中 |
| alias_list_empty_name / db_list_invalid_param | ILLEGAL_SUCCESS | 中 |
| create_mutation_unknown_param / create_mutation_extra_fields | PERMISSIVE_PARSING | 中 |

Batch 独有发现虽然数量较少，但包含了更高严重性的缺陷：
- **SEQUENCE_VIOLATION**（状态转换违规）是 Mine 完全无法发现的类别
- **IDEMPOTENT_SUCCESS**（幂等性违规）同样超出 Mine 的合同推导能力
- **Null 注入**和**资源耗尽**类缺陷具有实际安全影响

---

## 5. 结论与建议

### 5.1 核心结论

1. **Mine 在广度上占优，Batch 在深度上占优**
   - Mine 发现 96 个唯一缺陷（4× Batch），但 99% 为 ILLEGAL_SUCCESS，同质化严重
   - Batch 发现 24 个唯一缺陷，覆盖 4 种缺陷类型，语义多样性更高

2. **Mine 的"数量优势"需审慎解读**
   - 约 60 个 Mine 独有缺陷源自同一根因（search 端点参数过滤缺失），去重后实际独立漏洞约 5-8 个
   - 去重后 Mine 独立新发现约 15-20 个，Batch 独立新发现约 10-12 个

3. **LLM 编排器未产出增量价值**
   - 12 轮探索、20 项 Oracle 检查、65 项 SafetyNet 探针，未发现确定性生成器未覆盖的新缺陷
   - 当前 LLM 编排器的探索策略仍停留在已知缺陷空间的验证，缺乏真正的创造性发现

4. **两种模式互补性显著**
   - Mine 擅长系统性参数边界扫描（广度）
   - Batch 擅长语义级业务逻辑测试（深度）
   - 合并后可覆盖更完整的缺陷空间

### 5.2 建议

1. **短期：合并两种模式的缺陷集**
   - 将 Batch 的 IDEMPOTENT_SUCCESS、PERMISSIVE_PARSING、SEQUENCE_VIOLATION 类缺陷纳入 Mine 的合同模型
   - 将 Mine 的 DIFFERENTIAL_MISMATCH 发现纳入 Batch 的 SafetyNet 探针

2. **中期：增强 Mine 的缺陷类型多样性**
   - 在合同推导中增加幂等性约束（删除不存在资源应返回错误）
   - 增加状态序列约束（create → drop → create 应保持一致性）
   - 增加安全约束（Null 注入、资源耗尽）

3. **中期：优化 LLM 编排器**
   - 当前 LLM 编排器仅做"验证已知缺陷"，应改为"探索未知缺陷空间"
   - 引入覆盖率引导的探索策略，优先测试未覆盖的参数组合和端点交互

4. **长期：构建统一测试框架**
   - 以 Mine 的系统性覆盖为基线，Batch 的语义探针为增强
   - 建立"Mine 扫描 → Batch 深挖 → LLM 探索"的三层测试流水线
