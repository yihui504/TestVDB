# Candidate Defect: IllegalSuccess

- **Target**: qdrant
- **Version**: v1.18.0
- **Status**: Rejected
- **Downgrade Reason**: Independent developer-side review could not complete cleanly: Independent Qdrant probe failed.
STDOUT:

STDERR:
Traceback (most recent call last):
  File "<string>", line 86, in <module>
NameError: name 'time' is not defined


## Documentation Evidence
- **Source URL**: https://qdrant.tech/documentation/concepts/
- **Contract Assertions**:
- [SEARCH] vector length must match collection vector size
- [SEARCH] limit must be > 0
- [SEARCH] limit must be an integer (not float or string)
- [SEARCH] offset must be >= 0
- [SEARCH] offset must be an integer (not float or string)
- [SEARCH] params.hnsw_ef must be >= 1 (must NOT be 0 or negative)
- [SEARCH] score_threshold must be between 0.0 and 1.0
- [SEARCH] vector elements must be valid finite numbers (no NaN, no Infinity)
- [SEARCH] search against non-existent collection_name must return a clear error (not 200)
- [SEARCH] params.exact must be boolean true/false
- [CREATE] vectors.size must be > 0 (positive integer)
- [CREATE] vectors.size must not be 0 or negative
- [CREATE] vectors.distance must be one of: Dot, Cosine, Euclid, Manhattan
- [CREATE] invalid distance metric must be rejected with clear error
- [CREATE] shard_number must be >= 1 if specified
- [CREATE] shard_number=0 must be rejected
- [CREATE] missing required vectors config must be rejected
- [CREATE] duplicate collection name must return a clear conflict error (not 200)
- [BEHAVIOR:STATE] upsert N points → points_count must equal N
- [BEHAVIOR:STATE] delete M of N points → points_count must equal N-M
- [BEHAVIOR:STATE] upsert same point ID twice → points_count must NOT increase
- [BEHAVIOR:SEMANTIC] search results must have scores in descending order
- [BEHAVIOR:SEMANTIC] score_threshold must filter out results below threshold
- [BEHAVIOR:SEMANTIC] limit=1 must return exactly 1 result
- [BEHAVIOR:SEMANTIC] offset beyond total points must return empty results
- [BEHAVIOR:SEMANTIC] scroll with limit must paginate through all points without duplicates
- [BEHAVIOR:STATE] delete non-existent point IDs → points_count must NOT change
- [BEHAVIOR:STATE] delete collection then recreate with same name → must succeed
- [BEHAVIOR:SEMANTIC] exact=true search must return same results as approximate search
- [BEHAVIOR:DIAGNOSTIC] when limit=0 is rejected, error message must mention 'limit'

## MRE
```

import requests, sys, uuid, time

BASE = '{{TESTVDB_DB_URL}}'

# Create collection
c = 'defect_hnsw0_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={'vectors': {'size': 4, 'distance': 'Cosine'}})
time.sleep(0.5)

# Insert a point
r = requests.put(f'{BASE}/collections/{c}/points', json={'points': [{'id': 1, 'vector': [0.1, 0.2, 0.3, 0.4]}]})
time.sleep(0.3)

# Search with hnsw_ef=0 (INVALID: must be >= 1)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={
    'vector': [0.1, 0.2, 0.3, 0.4],
    'limit': 5,
    'params': {'hnsw_ef': 0}
})

if r.status_code == 200:
    print(f'[DEFECT: ILLEGAL_SUCCESS] hnsw_ef=0 accepted with status 200')
    print(f'Response: {r.json()}')
    sys.exit(1)
else:
    print(f'Correctly rejected: status={r.status_code}')
    sys.exit(0)

```

## Initial Run
- **Reason**: Observed explicit illegal success marker.
- **DB URL**: http://testvdb-db-797cb29fc6ba464aaf7d39255734d65e:6333

- **Evidence Excerpt**: [defect: illegal_success] hnsw_ef=0 accepted with status 200
response: {'result': [{'id': 1, 'version': 1, 'score': 1.0}], 'status': 'ok', 'time': 0.001299509}


### STDOUT
```

```

### STDERR
```

```

## Reproduction Attempts
- repro_1: Observed explicit illegal success marker.
- repro_2: Observed explicit illegal success marker.
