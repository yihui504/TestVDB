# Bug: Async upsert silently discards dimension-mismatched vectors (Poor Diagnostics)·

> **Note**: This issue was previously reported as [#2557](https://github.com/qdrant/qdrant/issues/2557). This report provides additional evidence with a comparative analysis of `wait=true` vs `wait=fal·se` behavior, and reclassifies the defect from `IllegalSuccess` to `PoorDiagnostics` based on deeper investigation.

## Summary

When upserting a point with a wrong-dimension vector (e.g., 3-dim vector into a 4-dim collection) **without** the `wait=true` parameter, the API returns HTTP 200 with `"status": "acknowledged"`, but the point is **silently discarded** during async processing. In contrast, the same operation **with** `wait=true` correctly returns HTTP 400 with a clear error message.

This creates a diagnostic gap: users who rely on the default async behavior receive no indication that their data was rejected, potentially leading to data loss without any error feedback.

## Current Behavior

```python
import requests, time

BASE = "http://localhost:6333"

# Setup: create 4-dim collection
requests.put(f"{BASE}/collections/test", json={"vectors": {"size": 4, "distance": "Cosine"}})
time.sleep(0.5)

# With wait=true: correctly rejects
r_wait = requests.put(f"{BASE}/collections/test/points?wait=true", json={
    "points": [{"id": 1, "vector": [0.1, 0.2, 0.3]}]  # 3-dim into 4-dim
})
print(f"wait=true:  status={r_wait.status_code}")  # 400 ✅
print(f"  error: {r_wait.json()}")  # "Vector dimension error: expected dim: 4, got 3"

# Without wait: silently discards
r_nowait = requests.put(f"{BASE}/collections/test/points", json={
    "points": [{"id": 2, "vector": [0.1, 0.2, 0.3]}]  # 3-dim into 4-dim
})
print(f"wait=false: status={r_nowait.status_code}")  # 200 ❌
print(f"  response: {r_nowait.json()}")  # {"result": {"status": "acknowledged"}}

# Verify: point was never stored
time.sleep(0.5)
info = requests.get(f"{BASE}/collections/test").json()
print(f"points_count: {info['result']['points_count']}")  # 0 (data was discarded)
```

## Expected Behavior

One of the following:

1. **Preferred**: Return HTTP 400/422 even for async upserts when dimension mismatch is detected at write time
2. **Alternative**: Return HTTP 202 (Accepted) instead of 200 to indicate the operation is queued but not yet validated, distinguishing it from confirmed success
3. **Minimum**: Include a warning in the response that validation has not been performed and the user should use `wait=true` for guaranteed validation

## Evidence

### Comparative Analysis

| Parameter | `wait=true` | `wait=false` (default) |
|-----------|-------------|------------------------|
| HTTP Status | **400** | **200** |
| Response | `"Vector dimension error: expected dim: 4, got 3"` | `"status": "acknowledged"` |
| Point stored | No | No |
| User informed | Yes | **No** |

### Why This Is `PoorDiagnostics`, Not `IllegalSuccess`

The data is **not** actually stored — the point is discarded during async processing. The issue is not that invalid data is accepted, but that the server provides **no diagnostic feedback** about the rejection when using the default async path. The user receives a 200 OK response suggesting success, while the data is silently lost.

### Reproduction (3 independent runs)

All 3 runs confirm:
- `wait=true`: 400, point not stored
- `wait=false`: 200 + acknowledged, point not stored, count=0

### Independent Review

An independent reviewer confirmed this defect with the following surviving assertion:

> `upsert with wrong dimension: wait=true correctly rejects (400) but wait=false returns 200+acknowledged while silently discarding data`

## Impact

- **Silent data loss**: Users may believe their data was successfully stored when it was actually discarded
- **Inconsistent behavior**: Same operation produces different HTTP status codes depending on `wait` parameter
- **No error feedback**: The `acknowledged` status only means "received by WAL", not "validated and stored"
- **Production risk**: Applications using async upserts (the default and recommended for throughput) have no way to detect dimension errors without explicitly using `wait=true`

## Environment

- Qdrant version: v1.18.0
- API: REST
- Deployment: Docker

## Related

- Original report: [#2557](https://github.com/qdrant/qdrant/issues/2557) - "Upserting point with wrong vector size does not return an error"
- Comment by @timvisee confirming the behavior and suggesting `wait=false` should return 202 instead of 200
- Comment by @agourlay noting that changing to 202 would be a breaking change

## Suggested Fix

1. **Validate at write time**: Even for async operations, perform basic validation (dimension check, required fields) before writing to WAL. This is a cheap O(1) check.
2. **Return 202 for async operations**: Distinguish between "validated and committed" (200 + completed) and "received but not yet validated" (202 + acknowledged)
3. **Add validation result endpoint**: Allow users to query the status of an acknowledged operation to check if it was eventually applied or rejected
