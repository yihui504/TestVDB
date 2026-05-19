# Bug: `wait=false` silently discards dimension-mismatched vectors (data loss)

## Summary

When upserting a point with a wrong-dimension vector (e.g., 3-dim into a 4-dim collection) using `wait=false`, the API returns HTTP 200 with `"status": "acknowledged"`, but the point is **silently discarded**. The same operation with `wait=true` correctly returns HTTP 400. This creates a data loss risk for applications using the default async path.

## Steps to Reproduce

```bash
# 1. Create a 4-dim collection
curl -X PUT 'http://localhost:6333/collections/test_async' \
  -H 'Content-Type: application/json' \
  -d '{"vectors": {"size": 4, "distance": "Cosine"}}'

# 2. wait=true with wrong dimension → correctly rejected
curl -X PUT 'http://localhost:6333/collections/test_async/points?wait=true' \
  -H 'Content-Type: application/json' \
  -d '{"points": [{"id": 1, "vector": [0.1, 0.2, 0.3]}]}'
# Returns 400 Bad Request ✅

# 3. wait=false with wrong dimension → silently discarded
curl -X PUT 'http://localhost:6333/collections/test_async/points?wait=false' \
  -H 'Content-Type: application/json' \
  -d '{"points": [{"id": 2, "vector": [0.1, 0.2, 0.3]}]}'
# Returns 200 {"result":{"operation_id":0,"status":"acknowledged"}} ❌

# 4. Verify: point was never stored
curl 'http://localhost:6333/collections/test_async/points/count'
# Returns {"result":{"count":0}} — data silently lost
```

## Expected Behavior

`wait=false` should also reject dimension-mismatched vectors with HTTP 400/422, or at minimum return HTTP 202 (Accepted) to indicate the operation is queued but not yet validated.

## Actual Behavior

| Parameter | HTTP Status | Point Stored | User Informed |
|-----------|-------------|--------------|---------------|
| `wait=true` | 400 | No | Yes ✅ |
| `wait=false` | 200 | No | **No** ❌ |

The 200 + `acknowledged` response misleads users into believing their data was stored.

## Impact

- **Silent data loss**: Users believe data was stored when it was actually discarded
- **Inconsistent behavior**: Same invalid operation produces different HTTP codes depending on `wait`
- **Production risk**: Applications using async upserts (default, recommended for throughput) have no way to detect dimension errors

## Environment

- Qdrant version: v1.18.0
- API: REST
- Deployment: Docker

## Suggested Fix

Add a dimension check at the API layer before writing to the WAL. This is an O(1) operation per vector that does not affect async write throughput:

```rust
for point in &points {
    if point.vector.len() != collection_config.vector_size {
        return Err(Error::bad_request(format!(
            "Vector dimension error: expected {}, got {}",
            collection_config.vector_size, point.vector.len()
        )));
    }
}
```

This minimal check prevents invalid data from entering the WAL, ensuring consistent behavior between `wait=true` and `wait=false`.

## Related

- [#2557](https://github.com/qdrant/qdrant/issues/2557) - Original report (closed as not_planned)
- [#9039](https://github.com/qdrant/qdrant/issues/9039) - Related discussion on async validation
