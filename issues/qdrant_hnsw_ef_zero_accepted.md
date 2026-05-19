# Bug: Search API accepts `hnsw_ef=0` despite documented constraint "must be >= 1"

## Summary

The Qdrant search API accepts `params.hnsw_ef=0`, violating the documented constraint that `hnsw_ef` must be >= 1. This may produce undefined search behavior or server panics without any error feedback.

## Steps to Reproduce

```bash
# 1. Create collection and insert a point
curl -X PUT 'http://localhost:6333/collections/test_hnsw' \
  -H 'Content-Type: application/json' \
  -d '{"vectors": {"size": 4, "distance": "Cosine"}}'

curl -X PUT 'http://localhost:6333/collections/test_hnsw/points' \
  -H 'Content-Type: application/json' \
  -d '{"points": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]}'

# 2. Search with hnsw_ef=0
curl -X POST 'http://localhost:6333/collections/test_hnsw/points/search' \
  -H 'Content-Type: application/json' \
  -d '{"vector": [0.1, 0.2, 0.3, 0.4], "limit": 10, "params": {"hnsw_ef": 0}}'
```

## Expected Behavior

HTTP 400/422 with an error message like:

```
Invalid search parameter 'hnsw_ef': value must be >= 1, got 0
```

## Actual Behavior

HTTP 200 with search results returned. No validation error.

## Impact

- **Undefined search behavior**: `hnsw_ef=0` may produce incorrect or empty results
- **Potential panic**: Related to #7967 where invalid HNSW parameters caused server panics
- **Documentation violation**: The [Qdrant documentation](https://qdrant.tech/documentation/concepts/search/#search-parameters) states `hnsw_ef` must be >= 1
- **Silent degradation**: Users receive no indication that the parameter is invalid

## Environment

- Qdrant version: v1.18.0
- API: REST
- Deployment: Docker

## Suggested Fix

Add server-side validation for `hnsw_ef >= 1` before executing search. Return 400/422 with a clear error message.

## Related

- [#9017](https://github.com/qdrant/qdrant/issues/9017) - HNSW parameter validation
- [#7502](https://github.com/qdrant/qdrant/issues/7502) - Search parameter constraints
- [#7967](https://github.com/qdrant/qdrant/issues/7967) - Server panic with invalid HNSW parameters
