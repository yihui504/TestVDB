## Current Behavior

Qdrant accepts `params.hnsw_ef: 0` in search requests and returns HTTP 200 OK with results, despite `hnsw_ef=0` being semantically invalid for the HNSW algorithm. This allows a meaningless parameter value to bypass server-side validation and succeed silently.

Confirmed on both **v1.17.1** and **v1.18.0**. Independently verified against a standalone `qdrant/qdrant:v1.18.0` Docker container on `localhost:6334`.

## Steps to Reproduce

1. Start a Qdrant instance (e.g., `docker run -p 6333:6333 qdrant/qdrant:v1.18.0`)
2. Create a collection with a 4-dimensional Cosine vector config:

```python
import requests

BASE = "http://localhost:6333"
COLLECTION = "test_hnsw_bug"

requests.put(f"{BASE}/collections/{COLLECTION}", json={
    "vectors": {"size": 4, "distance": "Cosine"}
})
```

3. Insert a point:

```python
import time
time.sleep(0.5)

requests.put(f"{BASE}/collections/{COLLECTION}/points", json={
    "points": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]
})
```

4. Search with `hnsw_ef=0` (should be rejected but isn't):

```python
r = requests.post(f"{BASE}/collections/{COLLECTION}/points/search", json={
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 3,
    "params": {"hnsw_ef": 0}
})
print(r.status_code, r.json())
# Expected: 400 or 422
# Actual: 200 OK with search results (score=1.0)
```

**Observed output**:
```
Search hnsw_ef=0: status=200
Response: {
  "result": [
    {
      "id": 1,
      "version": 1,
      "score": 1.0
    }
  ],
  "status": "ok",
  "time": 0.005683838
}
```

## Expected Behavior

The server should reject the request with a 400 or 422 error, since `hnsw_ef=0` is semantically invalid. The `hnsw_ef` parameter controls the size of the dynamic candidate list during HNSW search — a list of size 0 cannot perform any meaningful search traversal.

This expectation is supported by:
1. **Algorithm semantics**: The HNSW paper and all implementations treat `ef` as a positive integer. An `ef` of 0 means "consider zero candidates," which is equivalent to returning no results — yet Qdrant returns results as if the parameter were ignored.
2. **Internal consistency**: Qdrant's own `HnswConfigDiff` validates `ef_construct` with `#[validate(range(min = 4))]` (source: `lib/collection/src/operations/config_diff.rs`), demonstrating that the project considers small ef values invalid for index construction. The same validation is missing for the search-time `hnsw_ef`.
3. **Strict mode precedent**: The strict mode config includes `search_max_hnsw_ef` for upper-bound validation (source: `lib/collection/src/operations/verification/search.rs`), but no lower-bound check exists.
4. **API consistency**: The `limit` parameter is validated as `>=1` and `offset` as `>=0` in the OpenAPI schema, but `hnsw_ef` has no such constraint.

## Possible Solution

Add server-side validation for the `hnsw_ef` parameter in the search endpoint to reject values < 1. The validation should return a clear error message (e.g., 400 Bad Request with a descriptive body) when `hnsw_ef` is 0 or negative.

Example validation logic:
```rust
if let Some(hnsw_ef) = params.hnsw_ef {
    if hnsw_ef < 1 {
        return Err(Status::bad_request("hnsw_ef must be >= 1"));
    }
}
```

Additionally, add a regression test that asserts `hnsw_ef=0` produces a rejection rather than 200 OK.

## Context (Environment)

- **Qdrant version**: v1.17.1 and v1.18.0 (both affected)
- **Deployment**: Docker container (`qdrant/qdrant:v1.18.0`)
- **API**: REST API, POST `/collections/{collection_name}/points/search`
- **Discovery method**: Automated contract-based fuzzing with independent double-reproduction verification
- **Independent verification**: Manually confirmed against a standalone Docker container on `localhost:6334`

## Detailed Description

The `hnsw_ef` parameter controls the size of the dynamic list for the nearest neighbors during the search in the HNSW graph. Setting `hnsw_ef=0` is semantically meaningless — it would request a dynamic list of size zero, which cannot perform any meaningful search traversal.

Currently, Qdrant accepts `hnsw_ef=0` without error and returns search results. This suggests the server either:
1. Silently ignores the invalid value and falls back to a default, or
2. Treats 0 as a valid value internally, bypassing the constraint.

Either way, the behavior is inconsistent with the project's own validation patterns and could mislead users who accidentally pass `hnsw_ef=0` (e.g., from a configuration error or variable that defaults to 0).

**Source code evidence**:
- `ef_construct` has `#[validate(range(min = 4))]` in `lib/collection/src/operations/config_diff.rs`
- `SearchParams.hnsw_ef` is `optional uint64` in the gRPC proto with no minimum constraint
- Strict mode only checks `hnsw_ef` upper bound via `search_max_hnsw_ef` in `lib/collection/src/operations/verification/search.rs`
- No lower-bound validation exists for `hnsw_ef` anywhere in the codebase

The defect was independently verified through:
- **Initial observation**: `hnsw_ef=0` request returned 200 OK
- **Double reproduction**: Two fresh sandbox reproductions both confirmed `[DEFECT: ILLEGAL_SUCCESS] hnsw_ef=0 accepted`
- **Independent review**: Developer-side replay confirmed the issue survives
- **Manual verification**: Standalone Docker container on `localhost:6334` confirmed `hnsw_ef=0` returns 200 OK with valid results

## Possible Implementation

1. In the search request handler, validate `params.hnsw_ef` before processing:
   - If `hnsw_ef` is present and < 1, return 400 Bad Request with message: `"hnsw_ef must be >= 1, got 0"`
2. Add a unit/integration test:
   ```python
   def test_hnsw_ef_zero_rejected():
       r = requests.post(f"{BASE}/collections/{COLLECTION}/points/search", json={
           "vector": [0.1, 0.2, 0.3, 0.4],
           "limit": 3,
           "params": {"hnsw_ef": 0}
       })
       assert r.status_code in (400, 422), f"Expected rejection, got {r.status_code}"
   ```
3. Consider also validating negative values for `hnsw_ef` with the same check.
4. Consider adding `#[validate(range(min = 1))]` to the `hnsw_ef` field in the REST API schema, consistent with how `ef_construct` is validated.
