---
name: contract-schema
description: TestVDB 结构化契约 JSON Schema 参考。当 Contract Formalizer Agent 或相关 Agent 需要了解契约格式时自动加载。
version: 1.0.0
---

# Contract JSON Schema Reference

## 触发条件

Contract Formalizer Agent 生成契约 JSON 时自动加载。非用户手动触发。

## Schema Version: 1.0

### _passport 字段 (v2.0 新增)

```json
{
  "_passport": {
    "schema_version": "2.0",
    "contract_hash": "sha256:<hex_digest>",
    "contract_hash_algorithm": "sha256",
    "source": {
      "doc_urls": ["<url>", ...],
      "doc_version": "<version>",
      "crawl_method": "crawl4ai|webfetch|manual",
      "crawled_at": "<ISO 8601>"
    },
    "generation": {
      "knowledge_extractor_agent": "testvdb:knowledge-extractor",
      "contract_formalizer_agent": "testvdb:contract-formalizer",
      "generated_at": "<ISO 8601>",
      "cache_ttl_hours": 168
    },
    "integrity": {
      "verified": true,
      "verified_at": "<ISO 8601>",
      "core_crud_coverage_pct": 95.0,
      "endpoint_count": 12,
      "constraint_count": 85
    }
  }
}
```

**Hash 计算规则**：
- 输入 = 排除 `_passport` 字段后的完整 JSON（按 key 排序，无空格）
- 算法 = sha256
- 格式 = `sha256:<hex_digest>`

**验证方法**：
```bash
python scripts/passport_verify.py <path/to/structured_contract.json>
```

## 顶层结构

```json
{
  "target": "<string> - milvus/qdrant/weaviate/pgvector",
  "version": "<string> - 目标版本",
  "cache_ttl_hours": "<integer> - 契约缓存有效期（小时），默认 168（7天）",
  "cached_at": "<string> - 契约生成时间（ISO 8601），用于计算缓存是否过期",
  "sdk": { ... },
  "docker": { ... },
  "api_endpoints": [ ... ],
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
| parameters | array | No | Parameter definitions |

## 约束字段

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| constraint_id | string | Yes | Unique ID: `{target}_{type}_{endpoint}_{counter}` |
| endpoint | string | Yes | Referenced endpoint path |
| description | string | Yes | Human-readable constraint |
| assertion | string | Yes | Machine-readable check |
| type | string | Yes | `type_constraint/range_constraint/state_constraint` |
| level | string | Yes | `endpoint` / `system`（v3.4 规则 2.7）：单请求可观测 → endpoint；跨端点/跨请求序列 → system |
| bound_strategies | array | No | 预绑定 strategy_id 清单——`scripts/bind_strategies.py` 确定性写入（v3.4 D2），formalizer 不填 |
| evidence_tier | string | Yes | `explicit` / `inferred`（ADR-0008 两档） |

## 断言字段

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| assertion_id | string | Yes | Unique ID |
| endpoint | string | Yes | Referenced endpoint path |
| description | string | Yes | Human-readable |
| category | string | Yes | `type_check/range_check/state_check/behavioral` |
| expected_behavior | string | Yes | Expected outcome |
| evidence_tier | string | Yes | `explicit` / `inferred`（ADR-0008 两档） |
| defect_type_if_violated | string | No | Type1-4 classification |

## 置信度指南

| Score | Meaning |
|-------|---------|
| 1.0 | Explicitly stated in documentation |
| 0.8-0.9 | Strongly implied by examples |
| 0.6-0.7 | Inferred from related constraints |
| 0.4-0.5 | Industry convention |
| <0.4 | Do NOT include (too uncertain) |
