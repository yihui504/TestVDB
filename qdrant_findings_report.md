# Qdrant v1.13 Batch 探针缺陷报告

**生成时间：** 2026-05-23  
**目标版本：** qdrant/qdrant:latest (Docker)  
**探针模式：** Batch (手写 SafetyNet 探针)  
**探针总数：** 64 | **通过：** 49 | **缺陷：** 14 | **错误：** 1

---

## 缺陷清单

### 🔴 P0 — 数据损坏（4 个，需验证是否为量化伪影）

> ⚠️ 这些缺陷可能是 Qdrant 默认启用的标量量化（scalar quantization）导致的预期精度损失，非真正的数据损坏。需对未开启量化的集合复现。

| # | 名称 | 类型 | 描述 |
|---|------|------|------|
| 1 | upsert_readback_vector | DATA_CORRUPTION | 写入向量 [0.1, 0.2, 0.3, 0.4]，读回 [0.18257418, ...] |
| 2 | upsert_readback_overwrite | DATA_CORRUPTION | 覆盖写入向量 [0.5, ...]，读回 [0.37904903, ...] |
| 3 | update_payload_readback | DATA_CORRUPTION | payload 更新后向量从 0.1 变为 0.18257418 |
| 4 | batch_upsert_readback | DATA_CORRUPTION | 批量 upsert 后向量精度丢失 |

**验证步骤**：创建不含 `quantization_config` 的集合后复现。

---

### 🟠 P1 — 非法操作成功（7 个）

| # | 名称 | 类型 | 描述 |
|---|------|------|------|
| 5 | hnsw_ef=0 | ILLEGAL_SUCCESS | 搜索参数 hnsw_ef=0 被接受并返回结果 |
| 6 | oversampling=0 | ILLEGAL_SUCCESS | 量化参数 oversampling=0 被接受 |
| 7 | score_threshold_negative | ILLEGAL_SUCCESS | score_threshold 负数值被接受 |
| 8 | score_threshold_above_one | ILLEGAL_SUCCESS | score_threshold=2.0（>1.0）被接受 |
| 9 | zero_point_id | ILLEGAL_SUCCESS | point id=0 被接受 |
| 10 | very_large_dimension | ILLEGAL_SUCCESS | 向量维度 65536 被接受（可能 OOM） |
| 11 | empty_vector_values | ILLEGAL_SUCCESS | 空向量被接受 |

---

### 🟡 P1 — 状态逻辑违规（2 个）

| # | 名称 | 类型 | 描述 |
|---|------|------|------|
| 12 | payload_filter_consistency | STATE_LOGIC_VIOLATION | filter color=red 返回了 color=None 的点 |
| 13 | clear_points_count | STATE_LOGIC_VIOLATION | clear points 后 count 仍为 5，期望 0 |

---

### 🟢 P2 — 诊断不足（1 个）

| # | 名称 | 类型 | 描述 |
|---|------|------|------|
| 14 | upsert_wrong_dimension | POOR_DIAGNOSTICS | wait=true 正确拒绝(400)，但 wait=false 返回 200 + acknowledged，数据被静默丢弃 |

---

## 错误（1 个，非 Qdrant 缺陷）

| 名称 | 问题 | 原因 |
|------|------|------|
| inf_vector_search | JSON 序列化错误 | Python `requests` 库不支持 Infinity 值的 JSON 序列化，需改用 `json.dumps(allow_nan=False)` 的替代方案或自定义编码器 |

---

## 待验证项

- [ ] DATA_CORRUPTION × 4：对无量化集合复现，排除量化伪影
- [ ] hnsw_ef=0：确认 HNSW 规范是否要求 ef ≥ 1
- [ ] score_threshold：确认文档是否限制范围 [0.0, 1.0]
- [ ] 65536 维：确认是否有合理上限
