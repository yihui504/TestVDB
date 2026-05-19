# Bug: `vectors.size=0` creates zero-dimension collection, may cause server panic

## Summary

The Qdrant collection creation API accepts `vectors.size=0`, creating a zero-dimension collection that serves no valid purpose and may trigger server panics on subsequent operations. The API should reject this with a 400/422 error.

## Steps to Reproduce

```bash
curl -X PUT 'http://localhost:6333/collections/test_zero_dim' \
  -H 'Content-Type: application/json' \
  -d '{"vectors": {"size": 0, "distance": "Cosine"}}'
```

## Expected Behavior

HTTP 400/422 with an error message like:

```
Invalid collection config: vectors.size must be >= 1, got 0
```

## Actual Behavior

HTTP 200 OK — the collection is created successfully. Subsequent operations (upsert, search) on this collection may cause the server to panic.

## Impact

- **Server stability**: Zero-dimension collections can cause panics during vector operations (see #7967, #9045)
- **No valid use case**: A zero-dimension vector is mathematically meaningless
- **Recovery difficulty**: A panic loop may require manual intervention to delete the collection

## Environment

- Qdrant version: v1.18.0
- API: REST
- Deployment: Docker

## Suggested Fix

Add `size >= 1` validation in the API layer before creating the collection:

```rust
if size == 0 {
    return Err(CollectionError::bad_request("vectors.size must be >= 1"));
}
```

This is a minimal O(1) check that prevents invalid state from entering the system.

## Related

- [#7967](https://github.com/qdrant/qdrant/issues/7967) - Server panic related to invalid collection configuration
- [#9045](https://github.com/qdrant/qdrant/issues/9045) - Panic on operations with zero-dimension vectors
