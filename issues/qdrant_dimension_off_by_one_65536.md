# Bug: Collection creation accepts `size=65536` despite FAQ stating maximum is 65,535 (off-by-one)

## Summary

The Qdrant collection creation API accepts `vectors.size=65536`, but the [official FAQ](https://qdrant.tech/documentation/faq/qdrant-fundamentals/) states the maximum is **65,535**. Meanwhile, `size=65537` is correctly rejected with 422. This indicates an off-by-one error in the validation logic: the code uses `<=65536` where it should use `<=65535` (or `<65536`).

## Current Behavior

```python
import requests

BASE = "http://localhost:6333"

# size=65536: ACCEPTED (but FAQ says max is 65535)
r1 = requests.put(f"{BASE}/collections/test_65536", json={
    "vectors": {"size": 65536, "distance": "Cosine"}
})
print(f"size=65536: status={r1.status_code}")  # 200 ❌ (expected: 422)

# size=65537: REJECTED (correct)
r2 = requests.put(f"{BASE}/collections/test_65537", json={
    "vectors": {"size": 65537, "distance": "Cosine"}
})
print(f"size=65537: status={r2.status_code}")  # 422 ✅
```

## Expected Behavior

`size=65536` should be rejected with 422, consistent with the FAQ's stated maximum of 65,535 dimensions.

Alternatively, if 65,536 is the intended maximum, the FAQ should be updated to say "up to 65,536 dimensions".

## Evidence

### Documentation Reference

The [Qdrant FAQ: Vectors](https://qdrant.tech/documentation/faq/qdrant-fundamentals/) explicitly states:

> In dense vectors, Qdrant supports up to **65,535** dimensions.

### Boundary Test Results

| `size` value | HTTP Status | Result |
|-------------|-------------|--------|
| 65534 | 200 | Accepted ✅ |
| 65535 | 200 | Accepted ✅ (FAQ max) |
| **65536** | **200** | **Accepted ❌** (exceeds FAQ max by 1) |
| 65537 | 422 | Rejected ✅ |

### Version Consistency

| Version | `size=65536` | `size=65537` |
|---------|-------------|-------------|
| v1.12.1 | 200 (accepted) | 422 (rejected) |
| v1.18.0 | 200 (accepted) | 422 (rejected) |

Bug is present in both versions.

### Source Code Reference

Core maintainer @timvisee stated in [Discussion #4519](https://github.com/orgs/qdrant/discussions/4519):

> "The current limit for dense vectors is 65536"

This confirms the code uses 65536 as the upper bound, which is inconsistent with the FAQ's stated 65,535.

## Impact

- **Documentation inconsistency**: FAQ says 65,535, code allows 65,536
- **User confusion**: Users reading the FAQ may expect 65,536 to be rejected
- **Off-by-one risk**: If the limit is meant to align with `u16::MAX` (65,535), then 65,536 is a fencepost error
- **No functional harm**: A 65,536-dimension collection works correctly if the hardware supports it, but it violates the documented contract

## Environment

- Qdrant version: v1.12.1, v1.18.0
- API: REST
- Deployment: Docker

## Suggested Fix

**Option A (code fix)**: Change the validation from `<=65536` to `<=65535` to match the FAQ:

```rust
// In lib/collection/src/operations/types.rs
const MAX_DENSE_VECTOR_DIMENSION: usize = 65535; // was 65536
```

**Option B (docs fix)**: Update the FAQ to say "up to 65,536 dimensions" if 65,536 is the intended maximum.

Option A is preferred because 65,535 aligns with `u16::MAX` and is consistent with other vector databases (Weaviate also uses 65,535).

## Related

- [Discussion #4519](https://github.com/orgs/qdrant/discussions/4519) - @timvisee confirms "current limit for dense vectors is 65536"
- [Issue #7529](https://github.com/qdrant/qdrant/issues/7529) - Lists "vector dimensions: 65535" as a noteworthy limit
