## Current Behavior

Qdrant accepts `score_threshold` values outside the [0.0, 1.0] range in search requests and returns HTTP 200 OK, despite such values being semantically invalid for similarity scores. Both positive overflow (e.g., `score_threshold=2.0`) and negative values (e.g., `score_threshold=-0.5`) bypass server-side validation and succeed silently.

Confirmed on **v1.18.0**. Independently verified against a standalone `qdrant/qdrant:v1.18.0` Docker container on `localhost:6335`.

## Steps to Reproduce

1. Start a Qdrant instance (e.g., `docker run -p 6333:6333 qdrant/qdrant:v1.18.0`)

2. Create a collection with a 4-dimensional Cosine vector config:

```python
import requests

BASE = "http://localhost:6333"
COLLECTION = "test_score_threshold_bug"

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

4. Search with `score_threshold=2.0` (should be rejected but isn't):

```python
r = requests.post(f"{BASE}/collections/{COLLECTION}/points/search", json={
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 5,
    "score_threshold": 2.0
})
print(r.status_code, r.json())
# Expected: 400 or 422
# Actual: 200 OK with empty results
```

5. Search with `score_threshold=-0.5` (should be rejected but isn't):

```python
r = requests.post(f"{BASE}/collections/{COLLECTION}/points/search", json={
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 5,
    "score_threshold": -0.5
})
print(r.status_code, r.json())
# Expected: 400 or 422
# Actual: 200 OK with all results (negative threshold passes everything)
```

**Observed output for score_threshold=2.0**:
```
Search score_threshold=2.0: status=200
Response: {
  "result": [],
  "status": "ok",
  "time": 0.005683838
}
```

**Observed output for score_threshold=-0.5**:
```
Search score_threshold=-0.5: status=200
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

The server should reject the request with a 400 or 422 error, since `score_threshold` values outside [0.0, 1.0] are semantically invalid. The `score_threshold` parameter defines the minimum similarity score for search results — for Cosine similarity, valid scores are in [0.0, 1.0]; for Dot product, scores can vary but negative thresholds still lack meaningful use.

This expectation is supported by:
1. **Semantic consistency**: `score_threshold` is a filter on similarity scores. Accepting values outside the valid score range is meaningless — `score_threshold=2.0` will never match any result (wastes a query), and `score_threshold=-0.5` passes all results (equivalent to no threshold).
2. **API consistency**: The `limit` parameter is validated as `>=1` and `offset` as `>=0` in the OpenAPI schema, but `score_threshold` has no range constraint despite having a well-defined valid range.
3. **Contract documentation**: The documented contract assertion states "score_threshold must be between 0.0 and 1.0", which the server does not enforce.
4. **User experience**: A user who accidentally passes `score_threshold=2.0` (e.g., from a configuration error) will receive an empty result set with no indication that the parameter value is invalid, leading to confusing debugging sessions.

## Possible Solution

Add server-side validation for the `score_threshold` parameter in the search endpoint to reject values outside [0.0, 1.0]. The validation should return a clear error message (e.g., 400 Bad Request with a descriptive body) when `score_threshold` is out of range.

Example validation logic:
```rust
if let Some(score_threshold) = search_request.score_threshold {
    if score_threshold < 0.0 || score_threshold > 1.0 {
        return Err(Status::bad_request("score_threshold must be between 0.0 and 1.0"));
    }
}
```

Additionally, add a regression test that asserts out-of-range `score_threshold` values produce a rejection rather than 200 OK.

## Context (Environment)

- **Qdrant version**: v1.18.0 (affected)
- **Deployment**: Docker container (`qdrant/qdrant:v1.18.0`)
- **API**: REST API, POST `/collections/{collection_name}/points/search`
- **Discovery method**: Automated contract-based fuzzing with Oracle invariant derivation
- **Independent verification**: Manually confirmed against a standalone Docker container on `localhost:6335`

## Detailed Description

The `score_threshold` parameter defines the minimum similarity score for search results to be returned. Setting `score_threshold=2.0` is semantically meaningless for Cosine similarity — no score can exceed 1.0, so the filter will always exclude all results. Setting `score_threshold=-0.5` is equally meaningless — all scores exceed -0.5, so the filter has no effect.

Currently, Qdrant accepts both out-of-range values without error and returns search results (empty for positive overflow, all for negative). This suggests the server performs no range validation on `score_threshold`.

**Source code evidence**:
- `score_threshold` is `optional float` in the gRPC proto with no range constraint
- No validation exists for `score_threshold` range anywhere in the Qdrant codebase
- The OpenAPI schema defines `score_threshold` as `number` type with no `minimum` or `maximum` property
- The `limit` parameter has `minimum: 1` in the OpenAPI schema, demonstrating that Qdrant does validate other search parameters

**Impact assessment**:
- `score_threshold=2.0`: Returns empty results — logically harmless but wastes a query and provides no feedback about the invalid parameter
- `score_threshold=-0.5`: Returns all results — effectively disables the threshold filter, which could be a security concern if the threshold was intended to limit results to high-confidence matches

The defect was independently verified through:
- **Initial observation**: Both out-of-range `score_threshold` values returned 200 OK
- **Double reproduction**: Two fresh sandbox reproductions both confirmed `[DEFECT: ILLEGAL_SUCCESS] score_threshold=2.0 accepted` and `[DEFECT: ILLEGAL_SUCCESS] score_threshold=-0.5 accepted`
- **Independent review**: Extended QdrantIndependentReviewer probe now covers `score_threshold_high` and `score_threshold_neg` checks
- **Manual verification**: Standalone Docker container on `localhost:6335` confirmed both values return 200 OK

## Possible Implementation

1. In the search request handler, validate `score_threshold` before processing:
   - If `score_threshold` is present and < 0.0 or > 1.0, return 400 Bad Request with message: `"score_threshold must be between 0.0 and 1.0, got {value}"`
2. Add a unit/integration test:
   ```python
   def test_score_threshold_out_of_range_rejected():
       r = requests.post(f"{BASE}/collections/{COLLECTION}/points/search", json={
           "vector": [0.1, 0.2, 0.3, 0.4],
           "limit": 5,
           "score_threshold": 2.0
       })
       assert r.status_code in (400, 422), f"Expected rejection, got {r.status_code}"

       r = requests.post(f"{BASE}/collections/{COLLECTION}/points/search", json={
           "vector": [0.1, 0.2, 0.3, 0.4],
           "limit": 5,
           "score_threshold": -0.5
       })
       assert r.status_code in (400, 422), f"Expected rejection, got {r.status_code}"
   ```
3. Consider adding `minimum: 0.0` and `maximum: 1.0` to the `score_threshold` field in the OpenAPI schema, consistent with how `limit` has `minimum: 1`.
