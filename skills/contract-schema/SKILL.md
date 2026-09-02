---
name: contract-schema
description: TestVDB structured-contract JSON Schema reference. Auto-loaded when the Contract Formalizer agent or related agents need the contract format.
version: 1.0.0
---

# Contract JSON Schema Reference

## Trigger conditions

Auto-loaded when the Contract Formalizer agent generates contract JSON. Not user-triggered.

## Schema Version: 1.0

### The _passport field (added v2.0)

```json
{
  "_passport": {
    "schema_version": "2.0",
    "contract_hash": "sha256:<hex_digest>",
    "contract_hash_algorithm": "sha256",
    "source": {
      "doc_urls": ["<url>", "..."],
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

**Hash computation rules**:
- input = the complete JSON excluding the `_passport` field (keys sorted, no whitespace)
- algorithm = sha256
- format = `sha256:<hex_digest>`

**Verification method**:
```bash
python scripts/passport_verify.py <path/to/structured_contract.json>
```

## Top-level structure

```json
{
  "target": "<string> - milvus/qdrant/weaviate/pgvector",
  "version": "<string> - target version",
  "cache_ttl_hours": "<integer> - contract cache lifetime (hours), default 168 (7 days)",
  "cached_at": "<string> - contract generation time (ISO 8601), used to compute cache expiry",
  "sdk": { "..." : "..." },
  "docker": { "..." : "..." },
  "api_endpoints": [ "..." ],
  "constraints": {
    "type_constraints": [ "..." ],
    "range_constraints": [ "..." ],
    "state_constraints": [ "..." ],
    "resource_bound_constraints": [ "..." ],
    "doc_consistency_constraints": [ "..." ],
    "other_constraints": [ "..." ]
  },
  "assertions": [ "..." ],
  "behavioral_contracts": [ "..." ],
  "state_invariants": [ "..." ],
  "data_types": [ "..." ]
}
```

## Endpoint fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| path | string | Yes | Endpoint path (e.g., `search+points`, `CREATE TABLE`) |
| method | string | Yes | HTTP method or `SQL` |
| category | string | Yes | `collections/points/search/index/management/ddl/dml/dql` |
| description | string | No | Human-readable description |
| parameters | array | No | Parameter definitions |

## Constraint fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| constraint_id | string | Yes | Unique ID: `{target}_{type}_{endpoint}_{counter}` |
| endpoint | string | Yes | Referenced endpoint path |
| description | string | Yes | Human-readable constraint |
| assertion | string | Yes | Machine-readable check |
| type | string | Yes | `type_constraint/range_constraint/state_constraint/resource_bound/doc_consistency/other` (the last three = Rule 2.9's new constraint classes; other = the fallback class for documentation promises fitting no known class) |
| no_fit_reason | string | required for the other class | one sentence stating why it fits no known class (treating other as a lazy outlet is forbidden; Rule 2.9) |
| level | string | Yes | `endpoint` / `system` (v3.4 Rule 2.7): observable in a single request → endpoint; spans endpoints / crosses requests → system (the other class is graded normally per this rule) |
| bound_strategies | array | No | pre-bound strategy_id list — written deterministically by `scripts/bind_strategies.py` (v3.4 D2); the formalizer does not fill it |
| evidence_tier | string | Yes | `explicit` / `inferred` (ADR-0008's two grades) |

## Assertion fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| assertion_id | string | Yes | Unique ID |
| endpoint | string | Yes | Referenced endpoint path |
| description | string | Yes | Human-readable |
| category | string | Yes | `type_check/range_check/state_check/behavioral` |
| expected_behavior | string | Yes | Expected outcome |
| evidence_tier | string | Yes | `explicit` / `inferred` (ADR-0008's two grades) |
| defect_type_if_violated | string | No | Type1-4 classification |

## Confidence guide

| Score | Meaning |
|-------|---------|
| 1.0 | Explicitly stated in documentation |
| 0.8-0.9 | Strongly implied by examples |
| 0.6-0.7 | Inferred from related constraints |
| 0.4-0.5 | Industry convention |
| <0.4 | Do NOT include (too uncertain) |
