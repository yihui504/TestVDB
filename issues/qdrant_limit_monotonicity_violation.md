# Bug: Search top-1 result changes when `limit` parameter changes (limit monotonicity violation)

## Summary

When searching the same collection with different `limit` values, the top-1 result ID changes. Changing `limit=3` to `limit=10` produces a different first result: `top1=8` vs `top1=13`. This violates the expected property that the top-K results should be independent of the limit value — increasing the limit should only add more results, not change which results appear at the top.

**This is the most serious bug in this batch** — it means search results are non-deterministic relative to the limit parameter, which breaks fundamental metamorphic relations and can cause silent data quality issues in production applications.

## Steps to Reproduce

```python
import requests

BASE = 'http://localhost:6333'

# 1. Create collection
r = requests.put(f'{BASE}/collections/test_limit', json={
    "vectors": {"size": 4, "distance": "Cosine"}
})

# 2. Insert 20 points
points = [{"id": i, "vector": [0.1*i, 0.2*i, 0.3*i, 0.4*i]} for i in range(20)]
requests.put(f'{BASE}/collections/test_limit/points', json={"points": points})

# 3. Search with limit=3
r1 = requests.post(f'{BASE}/collections/test_limit/points/search', json={
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 3
})
top1_a = r1.json()['result'][0]['id']

# 4. Search with limit=10
r2 = requests.post(f'{BASE}/collections/test_limit/points/search', json={
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 10
})
top1_b = r2.json()['result'][0]['id']

print(f"limit=3 top-1: {top1_a}")   # 8
print(f"limit=10 top-1: {top1_b}")  # 13
# BUG: top1_a != top1_b
```

## Expected Behavior

The top-1 result should be the same regardless of `limit`. The search should find the globally closest point to the query vector and return it as the first result. Increasing `limit` should only add more results (2nd, 3rd, ..., Nth closest), never change the ordering of previously returned results.

Expected: `top-1(limit=3) == top-1(limit=10) == id_of_closest_point`

## Actual Behavior

- `limit=3`: top-1 = 8
- `limit=10`: top-1 = 13

The top result changes depending on how many results are requested.

## Impact

- **High severity**: Breaks the fundamental contract of search — results should be deterministic given the same query
- **Data quality**: Applications that paginate through search results (e.g., showing "page 1" with limit=10, then "page 2" with offset=10) will get inconsistent data
- **Metamorphic violation**: Changing a non-semantic parameter (limit) should not change the relative ordering of results
- **Reproducibility**: Debugging and testing becomes unreliable when search results are limit-dependent

## Analysis

This may be related to:
- FLAT index approximation: FLAT should be exact, so this should not happen
- Score ties: If multiple points have identical scores, tie-breaking may be non-deterministic
- Cosine distance normalization: The Cosine similarity of [0.1i, ...] vectors may produce ties for certain i values

The query vector `[0.1, 0.2, 0.3, 0.4]` is proportional to point id=1's vector `[0.1, 0.2, 0.3, 0.4]`. Cosine similarity between the query and point id=i is `cos([0.1,0.2,0.3,0.4], [0.1*i,0.2*i,0.3*i,0.4*i]) = 1.0` for ALL i (since the vectors are collinear). This means all points have identical Cosine similarity of 1.0, and the ordering is a tie-breaking problem.

**Root cause**: When all scores are identical (Cosine=1.0 for collinear vectors), Qdrant's tie-breaking is non-deterministic across different limit values. A stable sort or secondary sort key (e.g., by ID) should be applied.

## Environment

- Qdrant version: v1.13.4
- Index type: FLAT
- Distance: Cosine
- API: REST
- Deployment: Docker standalone

## Suggested Fix

Apply a stable secondary sort key (e.g., by point ID ascending) when scores are tied. This ensures deterministic ordering regardless of the limit parameter.

Alternatively: document that tie-breaking is implementation-defined and may vary between requests.

## Discovered By

Automated metamorphic testing via TestVDB (metamorphic limit monotonicity generator).
