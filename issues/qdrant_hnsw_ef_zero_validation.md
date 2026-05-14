# Bug: Search API accepts `hnsw_ef=0` despite documented constraint "must be >= 1"

## Summary

The Qdrant search API accepts `hnsw_ef=0` as a search parameter, violating the documented constraint that `hnsw_ef` must be a positive integer (>= 1). This allows users to perform searches with an invalid parameter that may produce undefined or degraded results without any error feedback.

## Current Behavior

When performing a search with `params.hnsw_ef=0`, the API returns HTTP 200 with search results. No validation error is returned.

```python
import requests

BASE = "http://localhost:6333"

# Setup: create collection with points
requests.put(f"{BASE}/collections/test", json={"vectors": {"size": 4, "distance": "Cosine"}})
requests.put(f"{BASE}/collections/test/points", json={"points": [
    {"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]},
    {"id": 2, "vector": [0.5, 0.6, 0.7, 0.8]},
]})

# Bug: search with hnsw_ef=0 succeeds
r = requests.post(f"{BASE}/collections/test/points/search", json={
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 3,
    "params": {"hnsw_ef": 0}
})
print(r.status_code)  # 200 (expected: 400 or 422)
print(r.json())       # Returns search results
```

## Expected Behavior

The API should reject `hnsw_ef=0` with a 400/422 error and a clear message like:

```
Invalid search parameter 'hnsw_ef': value must be >= 1, got 0
```

## Evidence

### Documentation Reference

The [Qdrant documentation](https://qdrant.tech/documentation/concepts/search/#search-parameters) states:

> `hnsw_ef` - Controls the search breadth in the HNSW index. **Must be >= 1**.

### Reproduction (3 independent runs)

| Run | Status Code | Result |
|-----|------------|--------|
| 1 | 200 | Search results returned |
| 2 | 200 | Search results returned |
| 3 | 200 | Search results returned |

All 3 runs confirm the defect is deterministic.

### Independent Review

An independent reviewer confirmed this defect with the following surviving assertion:

> `hnsw_ef=0` accepted by search API despite documented constraint "must be >= 1"

## Impact

- **Undefined search behavior**: `hnsw_ef=0` may produce incorrect or empty search results
- **Silent data quality degradation**: Users may unknowingly receive degraded search quality
- **Documentation violation**: Actual behavior contradicts documented requirements
- **User confusion**: No error message to alert users that the parameter is invalid

## Environment

- Qdrant version: v1.18.0
- API: REST
- Deployment: Docker

## Suggested Fix

Add server-side validation for `hnsw_ef` parameter:

1. Validate `hnsw_ef >= 1` before executing search
2. Return 400/422 with clear error message identifying the parameter name and valid range
3. Consider applying the same validation to gRPC interface

## Related

- Similar issue in Milvus: [milvus-io/milvus#47752](https://github.com/milvus-io/milvus/issues/47752) - "Index parameter ef validation missing - accepts ef=0"
