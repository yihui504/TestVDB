## Current Behavior

When creating a collection via the REST API, `shard_number=0` and negative values (e.g. `-1`) are accepted without returning an error. The collection is created successfully with HTTP 200.

## Steps to Reproduce

**Test `shard_number=0`:**

1. Start a Qdrant instance (v1.18.1)
2. Run the following script:

```python
import requests, sys, uuid
BASE = 'http://localhost:6333'
c = 'test_shard_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={
    "vectors": {"size": 4, "distance": "Cosine"},
    "shard_number": 0
})
if r.status_code == 200:
    print(f'[DEFECT: ILLEGAL_SUCCESS] shard_number=0 accepted ({r.status_code})')
    sys.exit(1)
else:
    print(f'properly rejected shard_number=0: {r.status_code}')
    sys.exit(0)
```

**Test `shard_number=-1`:**

```python
r = requests.put(f'{BASE}/collections/{c}', json={
    "vectors": {"size": 4, "distance": "Cosine"},
    "shard_number": -1
})
if r.status_code == 200:
    print(f'[DEFECT: ILLEGAL_SUCCESS] shard_number=-1 accepted ({r.status_code})')
    sys.exit(1)
else:
    print(f'properly rejected shard_number=-1: {r.status_code}')
    sys.exit(0)
```

## Expected Behavior

The API should reject `shard_number` values <= 0 with a 400 Bad Request and a descriptive error message indicating that `shard_number` must be a positive integer.

## Possible Solution

Add input validation for `shard_number` in the collection creation endpoint. At minimum, check that the value is a positive integer (>= 1) at the API boundary before passing it to internal collection creation logic. This could be implemented as a validation attribute on the request model (similar to how `ef_construct` already has `#[validate(range(min = 4))]` in the codebase).

## Context (Environment)

- **Qdrant version**: v1.18.1 (Docker: `qdrant/qdrant:v1.18.1`)
- **API**: REST (port 6333)
- **OS**: Linux (Docker container)

### Why this matters

- `shard_number=0` creates a collection with zero shards — no data can be distributed or stored. In a distributed setup this would break silently.
- A negative shard number has no semantic meaning and may cause undefined behavior in internal shard routing logic (e.g. modulo operations, array indexing).
- Other parameters in the same `PUT /collections/{name}` endpoint already have validation (e.g. `vectors.size` has an upper-bound check), so this is an inconsistency rather than a deliberate design choice.
