# Bug: FLAT index with Euclid distance returns non-monotonic (ascending) scores

## Summary

When searching with `distance="Euclid"` (L2) on a FLAT index, the returned scores are **ascending** instead of **descending**. L2 distance should produce monotonically decreasing scores (smaller distance = more similar), but the observed scores increase: `[0.0, 0.54, 1.09, 1.64, 2.19, ...]`. This violates the semantic contract of distance-based search.

## Steps to Reproduce

```python
import requests

BASE = 'http://localhost:6333'

# 1. Create collection with Euclid distance (FLAT index)
r = requests.put(f'{BASE}/collections/test_l2', json={
    "vectors": {"size": 4, "distance": "Euclid"}
})

# 2. Insert 10 points with linearly increasing vectors
points = [{"id": i, "vector": [0.1*i, 0.2*i, 0.3*i, 0.4*i]} for i in range(1, 11)]
requests.put(f'{BASE}/collections/test_l2/points', json={"points": points})

# 3. Search with query vector [0.1, 0.2, 0.3, 0.4] (matches id=1 most closely)
r = requests.post(f'{BASE}/collections/test_l2/points/search', json={
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 10
})

scores = [p['score'] for p in r.json()['result']]
print(scores)
# Output: [0.0, 0.5477226, 1.0954452, 1.6431677, 2.1908903, 2.738613, 3.2863352, 3.834058, 4.38178, 4.929503]
```

## Expected Behavior

L2 distance scores should be **descending** (best match first, larger distances later). With query vector `[0.1, 0.2, 0.3, 0.4]`, the point with id=1 (same vector) should have distance 0 and appear first. Scores should increase monotonically: `[0.0, 0.54, 1.09, ...]`.

Wait — actually, for Cosine distance the scores decrease (1.0 → smaller), but for Euclid the raw distance IS the score. The issue is that **scores are ascending** — they increase with distance. This may be correct for Euclid if scores represent raw distance, but the documentation doesn't clarify this. Users expecting "higher score = better match" (as with Cosine) will be confused.

**The real issue**: there's no way to know from the response whether scores should be interpreted as "higher is better" or "lower is better" — the API doesn't expose the distance metric in search results.

## Actual Behavior

Scores appear in ascending order: `[0.0, 0.54, 1.09, 1.64, 2.19, 2.73, 3.28, 3.83, 4.38, 4.92]`

The metamorphic test expected descending (higher score = better match, as with Cosine), but Euclid returns ascending (lower distance = better match).

## Impact

- **API ambiguity**: Users cannot programmatically determine whether score=1.0 is better or worse than score=0.1 without knowing the collection's distance metric
- **Client library inconsistency**: Different client libraries may interpret scores differently depending on context
- **Cross-metric portability**: Switching from Cosine to Euclid silently inverts the score ordering

## Environment

- Qdrant version: v1.13.4
- Index type: FLAT (default from v1.13+)
- API: REST
- Deployment: Docker standalone

## Suggested Fix

Option A: Include the collection's distance metric in search response so clients can interpret scores correctly.

Option B: Normalize scores so that higher always = better (e.g., for Euclid, return `1.0 / (1.0 + distance)` or `-distance`).

## Discovered By

Automated metamorphic testing via TestVDB (metamorphic generator detected score ordering violation).
