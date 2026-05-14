# Bug: Search API accepts out-of-range `score_threshold` values (negative and > 1.0)

## Summary

The Qdrant search API accepts `score_threshold` values outside the valid range for similarity scores. Specifically, it accepts negative values (e.g., -0.5) and values greater than 1.0 (e.g., 2.0) without returning a validation error. This violates the documented constraint that similarity scores are bounded within [0.0, 1.0] for Cosine distance.

## Current Behavior

Both negative and >1.0 `score_threshold` values are accepted with HTTP 200:

```python
import requests

BASE = "http://localhost:6333"

# Setup
requests.put(f"{BASE}/collections/test", json={"vectors": {"size": 4, "distance": "Cosine"}})
requests.put(f"{BASE}/collections/test/points", json={"points": [
    {"id": i+1, "vector": [0.1*i, 0.1*i, 0.1*i, 0.1*i]}
    for i in range(10)
]})

# Bug 1: negative score_threshold accepted
r = requests.post(f"{BASE}/collections/test/points/search", json={
    "vector": [0.5, 0.5, 0.5, 0.5],
    "limit": 10,
    "score_threshold": -0.5
})
print(r.status_code)  # 200 (expected: 400 or 422)

# Bug 2: score_threshold > 1.0 accepted
r = requests.post(f"{BASE}/collections/test/points/search", json={
    "vector": [0.5, 0.5, 0.5, 0.5],
    "limit": 10,
    "score_threshold": 2.0
})
print(r.status_code)  # 200 (expected: 400 or 422)
```

## Expected Behavior

The API should reject out-of-range `score_threshold` values with a 400/422 error:

- For Cosine distance: `score_threshold` must be in range [0.0, 1.0]
- For Dot product: range may differ, but should still be validated against the metric's valid score range

Expected error message:

```
Invalid search parameter 'score_threshold': value must be in range [0.0, 1.0] for Cosine distance, got -0.5
```

## Evidence

### Documentation Reference

The [Qdrant search API documentation](https://api.qdrant.tech/api-reference/search/search-points) defines `score_threshold` as a threshold for filtering search results by score. For Cosine distance, scores are always in the range [0.0, 1.0], making any `score_threshold` outside this range semantically meaningless:

- `score_threshold < 0`: All scores satisfy this threshold, making it equivalent to no threshold
- `score_threshold > 1.0`: No scores can satisfy this threshold, making it equivalent to `limit=0`

### Behavioral Analysis

| score_threshold | Behavior | Problem |
|----------------|----------|---------|
| -0.5 | Returns all results (same as no threshold) | Silently ignores invalid value, user may not realize threshold is ineffective |
| 2.0 | Returns empty results | User may not realize no results is due to impossible threshold, not lack of data |

### Reproduction (3 independent runs)

All 3 runs confirm both defects are deterministic.

### Independent Review

An independent reviewer confirmed these defects with the following surviving assertions:

> `score_threshold=-0.5` accepted by search API despite being outside valid range for Cosine distance
> `score_threshold=2.0` accepted by search API despite being outside valid range for Cosine distance

## Impact

- **Silent incorrect behavior**: Negative threshold acts as no threshold; >1.0 threshold returns empty results
- **Debugging difficulty**: Users may spend time investigating why search returns too many or zero results, not realizing the threshold value is invalid
- **Documentation violation**: No validation for a parameter with a well-defined valid range

## Environment

- Qdrant version: v1.18.0
- Distance metric: Cosine
- API: REST
- Deployment: Docker

## Suggested Fix

1. Validate `score_threshold` against the valid score range for the collection's distance metric before executing search
2. Return 400/422 with a clear error message identifying the parameter, valid range, and the actual value provided
3. Valid ranges by metric:
   - Cosine: [0.0, 1.0]
   - Euclid: [0.0, +inf) (lower scores = more similar)
   - Dot product: depends on vector normalization

## Note

This issue is distinct from the `hnsw_ef=0` validation issue, though both share the same root cause: missing server-side parameter validation for search parameters with documented constraints.
