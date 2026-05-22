# Bug: Search API accepts `score_threshold=-1` and `score_threshold=2.0` — no range validation

## Summary

The Qdrant search API accepts `score_threshold` values outside the documented `[0.0, 1.0]` range. Both `-1` and `2.0` return HTTP 200 with results instead of a validation error. This is the same class of parameter validation gap as `hnsw_ef=0` being accepted.

## Steps to Reproduce

```bash
# 1. Create collection and insert points
curl -X PUT 'http://localhost:6333/collections/test_st' \
  -H 'Content-Type: application/json' \
  -d '{"vectors": {"size": 4, "distance": "Cosine"}}'

curl -X PUT 'http://localhost:6333/collections/test_st/points' \
  -H 'Content-Type: application/json' \
  -d '{"points": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]}'

# 2. Search with score_threshold=-1 (BUG: accepted)
curl -X POST 'http://localhost:6333/collections/test_st/points/search' \
  -H 'Content-Type: application/json' \
  -d '{"vector": [0.1, 0.2, 0.3, 0.4], "limit": 10, "score_threshold": -1}'

# 3. Search with score_threshold=2.0 (BUG: accepted)
curl -X POST 'http://localhost:6333/collections/test_st/points/search' \
  -H 'Content-Type: application/json' \
  -d '{"vector": [0.1, 0.2, 0.3, 0.4], "limit": 10, "score_threshold": 2.0}'
```

## Expected Behavior

HTTP 400/422 with an error message like:

```
Invalid search parameter 'score_threshold': value must be in range [0.0, 1.0], got -1
```

## Actual Behavior

Both requests return HTTP 200. The server appears to accept out-of-range values without validation.

## Impact

- **Semantic violation**: `score_threshold` is defined as a [0.0, 1.0] similarity threshold. Values outside this range have no defined meaning
- **Undefined filtering behavior**: A negative threshold or threshold > 1.0 may produce inconsistent or empty result sets
- **API contract breach**: The documented API surface does not define behavior for out-of-range values

## Environment

- Qdrant version: v1.13.4
- API: REST
- Deployment: Docker standalone

## Suggested Fix

Add server-side validation for `score_threshold ∈ [0.0, 1.0]` before executing search. Return 400/422 with a clear error message indicating the valid range.

## Discovered By

Automated parameter validation testing via TestVDB (contract-driven boundary value generator).
