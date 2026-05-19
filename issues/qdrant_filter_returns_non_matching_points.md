# Bug: Payload filter returns points that do not match the filter condition

## Summary

When performing a search with a payload filter (e.g., `color=red`), points that lack the filtered payload key entirely (e.g., `color=None`) may be incorrectly included in the results. Points without the specified payload key should not match a `must` filter condition.

## Steps to Reproduce

```bash
# 1. Create collection
curl -X PUT 'http://localhost:6333/collections/test_filter' \
  -H 'Content-Type: application/json' \
  -d '{"vectors": {"size": 4, "distance": "Cosine"}}'

# 2. Insert points: two with payload, one without
curl -X PUT 'http://localhost:6333/collections/test_filter/points' \
  -H 'Content-Type: application/json' \
  -d '{"points": [
    {"id": 1, "vector": [0.1, 0.2, 0.3, 0.4], "payload": {"color": "red"}},
    {"id": 2, "vector": [0.5, 0.6, 0.7, 0.8], "payload": {"color": "blue"}},
    {"id": 3, "vector": [0.9, 0.1, 0.2, 0.3]}
  ]}'

# 3. Search with filter: color=red
curl -X POST 'http://localhost:6333/collections/test_filter/points/search' \
  -H 'Content-Type: application/json' \
  -d '{"vector": [0.1, 0.2, 0.3, 0.4], "limit": 10, "filter": {"must": [{"key": "color", "match": {"value": "red"}}]}}'
```

## Expected Behavior

Only point `id=1` (with `color=red`) should be returned.

## Actual Behavior

Point `id=3` (no `color` payload) may also appear in results, despite not matching the filter condition `color=red`.

## Impact

- **Incorrect query results**: Users receive data that does not match their filter criteria
- **Data integrity concern**: Filter-based access control or business logic may be compromised
- **Silent failure**: No error is raised; the results simply contain wrong data

## Environment

- Qdrant version: v1.18.0
- API: REST
- Deployment: Docker

## Suggested Fix

Ensure that a `must` filter with `match` condition on a payload key excludes points where that key is absent. A missing key should not satisfy a `match` condition.

## Related

- [#7855](https://github.com/qdrant/qdrant/issues/7855) - Filter returning unexpected results
- [#8935](https://github.com/qdrant/qdrant/issues/8935) - Payload filter matching behavior with missing keys
