---
name: contract-schema
description: TestVDB 结构化契约 JSON Schema 参考。当 Contract Formalizer Agent 或相关 Agent 需要了解契约格式时自动加载。
version: 1.1.0
---

# Contract JSON Schema Reference

## 触发条件

Contract Formalizer Agent 生成契约 JSON 时自动加载。非用户手动触发。

## Schema Version: 1.1

## 顶层结构

```json
{
  "target": "<string> - milvus/qdrant/weaviate/pgvector>",
  "version": "<string> - 目标版本",
  "cache_ttl_hours": "<integer> - 契约缓存有效期（小时），默认 168（7天）",
  "cached_at": "<string> - 契约生成时间（ISO 8601），用于计算缓存是否过期",
  "sdk": { ... },
  "docker": { ... },
  "api_endpoints": [ ... ],
  "endpoint_registry": [ ... ],
  "constraints": {
    "type_constraints": [ ... ],
    "range_constraints": [ ... ],
    "state_constraints": [ ... ]
  },
  "assertions": [ ... ],
  "behavioral_contracts": [ ... ],
  "state_invariants": [ ... ],
  "data_types": [ ... ]
}
```

## 端点字段

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| path | string | Yes | Endpoint path (e.g., `search+points`, `CREATE TABLE`) |
| method | string | Yes | HTTP method or `SQL` |
| category | string | Yes | `collections/points/search/index/management/ddl/dml/dql` |
| description | string | No | Human-readable description |
| source_url | string | Yes | 该端点文档的原始 URL，用于证据链追溯 |
| doc_version | string | Yes | 该端点文档的版本号 |
| parameters | array | No | Parameter definitions |

## 端点注册表字段

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| path | string | Yes | 端点路径（与 api_endpoints 中的 path 一一对应） |
| method | string | Yes | HTTP 方法 |
| source_url | string | Yes | 该端点文档的原始 URL |
| doc_version | string | Yes | 该页面的文档版本号 |
| doc_quote | string | No | 文档中关于该端点的关键描述（1-2 句） |
| verified_at | string | Yes | 验证时间（ISO 8601） |

## 约束字段

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| constraint_id | string | Yes | Unique ID: `{target}_{type}_{endpoint}_{counter}` |
| endpoint | string | Yes | Referenced endpoint path |
| description | string | Yes | Human-readable constraint |
| assertion | string | Yes | Machine-readable check |
| type | string | Yes | `type_constraint/range_constraint/state_constraint` |
| confidence | float | Yes | 0.0-1.0 confidence score |
| source_url | string | Yes | 该约束来源的文档 URL |
| source_status | string | No | `reachable/unreachable/degraded` — source_url 可达性状态 |

## 断言字段

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| assertion_id | string | Yes | Unique ID |
| endpoint | string | Yes | Referenced endpoint path |
| description | string | Yes | Human-readable |
| category | string | Yes | `type_check/range_check/state_check/behavioral` |
| expected_behavior | string | Yes | Expected outcome |
| confidence | float | Yes | 0.0-1.0 |
| defect_type_if_violated | string | No | `Type1_IllegalSuccess/Type2_PoorDiagnostics/Type3_RuntimeFailure/Type4_StateLogicViolation` |
| source_url | string | Yes | 该断言来源的文档 URL |
| doc_version | string | No | 该断言来源的文档版本 |

## 行为契约字段

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| contract_id | string | Yes | Unique ID |
| description | string | Yes | Human-readable description |
| scenario | string | Yes | 触发场景描述 |
| expected_behavior | string | Yes | 预期行为 |
| related_endpoints | array | No | 相关端点路径列表 |
| source_url | string | Yes | 该行为契约来源的文档 URL |

## 状态不变量字段

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| invariant_id | string | Yes | Unique ID |
| description | string | Yes | Human-readable description |
| assertion | string | Yes | Machine-readable invariant assertion |
| scope | string | No | `per_collection/per_table/global` |
| source_url | string | Yes | 该不变量来源的文档 URL |

## 置信度指南

| Score | Meaning |
|-------|---------|
| 1.0 | Explicitly stated in documentation |
| 0.8-0.9 | Strongly implied by examples |
| 0.6-0.7 | Inferred from related constraints |
| 0.4-0.5 | Industry convention |
| <0.4 | Do NOT include (too uncertain) |
