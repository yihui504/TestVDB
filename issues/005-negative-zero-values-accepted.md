# Milvus REST API v2: Negative/Zero Values Accepted for Index and Collection Parameters

## Bug Report

**Severity**: P2 (Input Validation)  
**Version**: Milvus v2.6.16  
**Component**: REST API / Proxy  
**Status**: Partially reported — #49930 covers searchParams (ef/nprobe), but the following parameters are NOT covered

## Description

Several Milvus REST API v2 endpoints accept negative or zero values for parameters that require positive integers. While #49930 reports this for searchParams (ef=0/-1, nprobe=0/-1), the following parameters have the same validation deficiency.

## Affected Parameters (Not Covered by #49930)

### 1. `efconstruction=0` and `efconstruction=-1` (HNSW index parameter)

```python
import requests

BASE = "http://localhost:19530"
HEADERS = {"Authorization": "Bearer root:Milvus", "Content-Type": "application/json"}

# Create collection
requests.post(f"{BASE}/v2/vectordb/collections/create", headers=HEADERS,
              json={"collectionName": "test_efconstruction", "dimension": 4})

# Create index with efconstruction=0
r = requests.post(f"{BASE}/v2/vectordb/indexes/create", headers=HEADERS,
                  json={
                      "collectionName": "test_efconstruction",
                      "indexParams": [{"fieldName": "vector", "indexName": "vector_idx",
                                       "metricType": "COSINE", "params": {"efConstruction": 0, "M": 16}}]
                  })
print(f"efconstruction=0: {r.json()}")
# Expected: {"code": 400, "message": "efConstruction must be >= 2"}
# Actual: {"code": 0} (success)

# Create index with efconstruction=-1
r = requests.post(f"{BASE}/v2/vectordb/indexes/create", headers=HEADERS,
                  json={
                      "collectionName": "test_efconstruction",
                      "indexParams": [{"fieldName": "vector", "indexName": "vector_idx2",
                                       "metricType": "COSINE", "params": {"efConstruction": -1, "M": 16}}]
                  })
print(f"efconstruction=-1: {r.json()}")
# Expected: {"code": 400, "message": "efConstruction must be >= 2"}
# Actual: {"code": 0} (success)

# Cleanup
requests.post(f"{BASE}/v2/vectordb/collections/drop", headers=HEADERS,
              json={"collectionName": "test_efconstruction"})
```

### 2. `collection.ttl.seconds=-1` (Collection TTL)

```python
r = requests.post(f"{BASE}/v2/vectordb/collections/create", headers=HEADERS,
                  json={"collectionName": "test_ttl", "dimension": 4,
                        "params": {"ttlSeconds": -1}})
print(f"ttlSeconds=-1: {r.json()}")
# Expected: {"code": 400, "message": "ttlSeconds must be non-negative"}
# Actual: {"code": 0} (success)

# Cleanup
requests.post(f"{BASE}/v2/vectordb/collections/drop", headers=HEADERS,
              json={"collectionName": "test_ttl"})
```

### 3. `rerank=-1` (Search rerank parameter)

```python
# Setup
requests.post(f"{BASE}/v2/vectordb/collections/create", headers=HEADERS,
              json={"collectionName": "test_rerank", "dimension": 4})

r = requests.post(f"{BASE}/v2/vectordb/entities/search", headers=HEADERS,
                  json={"collectionName": "test_rerank", "data": [[0.1, 0.2, 0.3, 0.4]],
                        "rerank": -1, "limit": 1})
print(f"rerank=-1: {r.json()}")
# Expected: {"code": 400, "message": "rerank must be non-negative"}
# Actual: {"code": 0} (success)

# Cleanup
requests.post(f"{BASE}/v2/vectordb/collections/drop", headers=HEADERS,
              json={"collectionName": "test_rerank"})
```

### 4. Negative offset in search

```python
# Setup
requests.post(f"{BASE}/v2/vectordb/collections/create", headers=HEADERS,
              json={"collectionName": "test_offset", "dimension": 4})

r = requests.post(f"{BASE}/v2/vectordb/entities/search", headers=HEADERS,
                  json={"collectionName": "test_offset", "data": [[0.1, 0.2, 0.3, 0.4]],
                        "offset": -1, "limit": 1})
print(f"offset=-1: {r.json()}")
# Expected: {"code": 400, "message": "offset must be non-negative"}
# Actual: {"code": 0} (success)

# Cleanup
requests.post(f"{BASE}/v2/vectordb/collections/drop", headers=HEADERS,
              json={"collectionName": "test_offset"})
```

## Expected Behavior

All parameters should reject negative values (and zero where minimum is > 0) with a 400 error.

## Actual Behavior

All parameters accept negative and zero values and return success (`code: 0`).

## Environment

- Milvus: v2.6.16 (standalone, Docker)
- API: REST API v2

## Related Issues

- #49930: REST API v2 accepts invalid searchParams (ef=0/-1, nprobe=0/-1) — same category, accepted by developers (milestone 2.6.18)
- #49823: REST API v2 accepts nprobe=0 — predecessor to #49930

## Developer Attitude

The Milvus team has accepted this category of bugs (marked `triage/accepted`, assigned to MrPresent-Han, milestone 2.6.18). The above parameters are additional instances of the same systemic validation gap.
