# Bug: Empty vector `[]` upsert with `wait=false` can trigger server panic (zero-length assertion failure)

## Summary

When upserting a point with an empty vector `[]` (zero dimensions) **without** the `wait=true` parameter, the API returns HTTP 200 with `"status": "acknowledged"`. While the point is eventually discarded during async processing, the zero-length vector can reach internal code paths that assert on non-zero length, causing **server panics** in distributed/sharded deployments.

This is distinct from the general "async upsert skips validation" issue (#2557, closed as not_planned) because empty vectors pose a **server stability risk** beyond mere poor diagnostics.

## Current Behavior

```python
import requests, time

BASE = "http://localhost:6333"

# Setup: create 4-dim collection
requests.put(f"{BASE}/collections/test", json={"vectors": {"size": 4, "distance": "Cosine"}})
time.sleep(0.5)

# With wait=true: correctly rejects empty vector
r_wait = requests.put(f"{BASE}/collections/test/points?wait=true", json={
    "points": [{"id": 1, "vector": []}]
})
print(f"wait=true:  status={r_wait.status_code}")  # 400 ✅

# Without wait: silently accepts empty vector
r_nowait = requests.put(f"{BASE}/collections/test/points", json={
    "points": [{"id": 2, "vector": []}]
})
print(f"wait=false: status={r_nowait.status_code}")  # 200 ❌
```

## Why This Is Different From #2557

I'm aware that #2557 (async upsert dimension validation) was closed as "not planned". This issue is filed separately because empty vectors are **qualitatively different** from wrong-dimension vectors:

| Aspect | Wrong dimension (e.g., 3→4) | Empty vector `[]` |
|--------|---------------------------|-------------------|
| Could be a typo? | Yes, off-by-one | No, zero dims is never intentional |
| Legitimate use case? | Arguably yes (migration) | No |
| Data stored? | No (discarded) | No (discarded) |
| **Can cause server panic?** | **No** | **Yes** — see #7967 |

The key difference: empty vectors can trigger **server crashes** in certain code paths, while wrong-dimension vectors are merely silently discarded.

## Evidence of Panic Risk

[Issue #7967](https://github.com/qdrant/qdrant/issues/7967) reports a production panic:

```
ERROR qdrant::startup: Panic occurred in file /qdrant/lib/common/common/src/fixed_length_priority_queue.rs at line 40:
length must be greater than zero
```

This panic occurs in `SearchResultAggregator::new` when a zero-length situation reaches the search path. While #7967's exact trigger was in a distributed context, the root cause is the same: zero-length data reaching code that asserts non-zero length.

### Source Code: Explicit Zero-Dimension Rejection

From [`lib/segment/src/data_types/vectors.rs`](https://github.com/qdrant/qdrant/blob/master/lib/segment/src/data_types/vectors.rs):

```rust
pub fn try_from_flatten(vectors: Vec<T>, dim: usize) -> Result<Self, OperationError> {
    if dim == 0 {
        return Err(OperationError::validation_error(
            "MultiDenseVector cannot have zero dimension",
        ));
    }
    // ...
}
```

The code **explicitly** treats `dim == 0` as a validation error. The problem is this check is only reached in the sync (`wait=true`) path.

### Source Code: Debug-Only Validation

[PR #8677](https://github.com/qdrant/qdrant/pull/8677) review noted that `validate_vector_parameters` is only called inside `debug_assert!`, meaning **release builds skip this validation entirely**, allowing malformed vectors to reach deeper code paths.

### Version Consistency

| Version | `vector=[], wait=false` | `vector=[], wait=true` |
|---------|------------------------|----------------------|
| v1.12.1 | 200 (accepted) | 400 (rejected) |
| v1.18.0 | 200 (accepted) | 400 (rejected) |

## Impact

- **Server stability**: Empty vectors reaching internal code paths can trigger panics (#7967), crashing the service
- **Silent data loss**: Users receive 200 OK but data is discarded
- **Inconsistent validation**: `wait=true` rejects, `wait=false` accepts

## Proposed Fix

Unlike the general dimension validation issue (#2557), fixing empty vector handling has a **minimal performance impact** because it's a simple `vector.is_empty()` check at the API boundary:

```rust
// In the upsert handler, before writing to WAL:
for point in &points {
    if point.vector.is_empty() {
        return Err(OperationError::validation_error(
            "Vector cannot be empty (zero dimensions)"
        ));
    }
}
```

This is an O(n) check over the number of points (not dimensions), and `is_empty()` is a length comparison against 0 — essentially free. It does not require reading collection metadata or comparing against the configured dimension, so it doesn't affect the async throughput argument.

## Environment

- Qdrant version: v1.12.1, v1.18.0
- API: REST
- Deployment: Docker

## Related

- [#7967](https://github.com/qdrant/qdrant/issues/7967) - Server panic: "length must be greater than zero" in search path (Open, bug label)
- [#2557](https://github.com/qdrant/qdrant/issues/2557) - Async upsert doesn't return error for wrong dimension (Closed, not_planned)
- [#9039](https://github.com/qdrant/qdrant/issues/9039) - Re-report of #2557 with comparative analysis (Open)
- [PR #8677](https://github.com/qdrant/qdrant/pull/8677) - `validate_vector_parameters` only in debug_assert
