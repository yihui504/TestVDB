**Comment for [qdrant/qdrant#9044](https://github.com/qdrant/qdrant/issues/9044)**

---

Related to the off-by-one upper-bound issue already reported in this thread: the REST API also accepts `vectors.size=0` and negative values (e.g. `-1`) without returning an error during collection creation.

## MRE for `vectors.size=0`

```python
import requests, sys, uuid
BASE = 'http://localhost:6333'
c = 'test_vecsize_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={
    "vectors": {"size": 0, "distance": "Cosine"}
})
if r.status_code == 200:
    print(f'[DEFECT: ILLEGAL_SUCCESS] vectors.size=0 accepted ({r.status_code})')
    sys.exit(1)
else:
    print(f'properly rejected vectors.size=0: {r.status_code}')
    sys.exit(0)
```

## MRE for `vectors.size=-1`

```python
r = requests.put(f'{BASE}/collections/{c}', json={
    "vectors": {"size": -1, "distance": "Cosine"}
})
if r.status_code == 200:
    print(f'[DEFECT: ILLEGAL_SUCCESS] vectors.size=-1 accepted ({r.status_code})')
    sys.exit(1)
else:
    print(f'properly rejected vectors.size=-1: {r.status_code}')
    sys.exit(0)
```

## Why this is different from the 65536 off-by-one

- The 65536 issue is a **documentation/code alignment** problem — 65536 is a valid `u16` but the FAQ says 65535. It's about intent clarity.
- `size=0` is a **runtime correctness** problem — a zero-dimensional vector collection has no valid use case. Any subsequent upsert or search will likely fail or produce undefined behavior.
- `size=-1` is a **semantic impossibility** — no dimensionality can be negative.

## Precedent

Both of these are more severe than the off-by-one FAQ issue. In particular, the empty vector case was recognized as a crash risk in #9045 (fixed in #9070). A collection created with `size=0` would accept empty vectors and could hit the same code path.

## Environment

- **Qdrant version**: v1.18.1 (Docker: `qdrant/qdrant:v1.18.1`)
- **API**: REST (port 6333)
