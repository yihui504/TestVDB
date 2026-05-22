# Qdrant v1.13.4 缺陷分析报告

**生成时间：** 2026-05-22  
**测试方法：** TestVDB mine 管线（确定性生成器 2 轮 + LLM 编排器 3 轮）  
**目标版本：** qdrant/qdrant:v1.13.4 (Docker)  

---

## 四数据库横向对比

| 数据库 | 确定性缺陷 | LLM 发现 | 聚类数 | 评价 |
|--------|-----------|---------|--------|------|
| Milvus v2.6.16 | 96 | 0 | ~20→3 real | 参数校验最弱 |
| Qdrant v1.13.4 | 13 | 1 | 3 (1 real) | 中等 |
| Weaviate 1.37.4 | 0 | 1 | 0 | 参数校验严格 |
| PgVector pg17 | 0 | 0 | 0 | PostgreSQL 级别校验 |

---

## Qdrant P0 缺陷（建议提交）

### 1. hnsw_ef=0 被接受（同 Milvus #49823）

| 字段 | 内容 |
|------|------|
| 缺陷名 | hnsw_ef=0 accepted |
| 类型 | ILLEGAL_SUCCESS |
| 影响 | POST /collections/{name}/points/search |
| 期望 | 拒绝 hnsw_ef=0（至少为 1 才能探测任何桶） |
| 实际 | 返回 200 和正常搜索结果 |
| MRE | `body["params"]={"hnsw_ef":0}` 搜正常返回 |

### 2. score_threshold 范围校验缺失

| 字段 | 内容 |
|------|------|
| 缺陷名 | score_threshold range validation missing |
| 类型 | ILLEGAL_SUCCESS |
| 影响 | POST /collections/{name}/points/search |
| 期望 | 拒绝 score_threshold ∉ [0, 1] |
| 实际 | score_threshold=-1 和 score_threshold=2 均返回 200 |

### 3. FLAT L2 距离排序不单调

| 字段 | 内容 |
|------|------|
| 缺陷名 | FLAT L2 distance ordering not monotonic |
| 类型 | METAMORPHIC_VIOLATION |
| 影响 | POST /collections/{name}/points/search |
| 复现 | 10 个点 (id=1..10, vector=[0.1*i,...])，搜索返回 scores 递增而非递减 |
| 分析 | FLAT 索引应精确排序，Euclid 距离结果应为单调递减 |
| 严重性 | 中 — 搜索结果排序可能不正确 |

### 4. limit 变化导致 top-1 结果不一致

| 字段 | 内容 |
|------|------|
| 缺陷名 | limit monotonicity violation |
| 类型 | METAMORPHIC_VIOLATION |
| 影响 | POST /collections/{name}/points/search |
| 复现 | 20 个点，limit=3 返回 top1=8，limit=10 返回 top1=13 |
| 期望 | limit 增大不应改变之前返回的 top-K 结果 |
| 严重性 | 高 — 破坏 metamorphic 关系，limit 参数语义不成立 |

---

## 已知良性模式（不建议提交）

- **search 端点参数注入**：body["shard_number"]=-1 等被接受 — Qdrant search 端点静默忽略未知 JSON 键，这是设计行为
- **版本不兼容导致的 DIFFERENTIAL_MISMATCH**：qdrant-client 1.16.1 与 server 1.13.4 版本不匹配

---

## 生成 Issue 草稿
