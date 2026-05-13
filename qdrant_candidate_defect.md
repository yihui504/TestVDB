# Candidate Defect: IllegalSuccess

- **Target**: qdrant
- **Version**: 1.18.0
- **Status**: Rejected
- **Downgrade Reason**: repro_1 failed verification: Execution completed without defect markers.

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

## MRE
```
import requests
import json

BASE_URL = "{{TESTVDB_DB_URL}}"

# Create a test collection
collection_name = "mre_test_collection"
resp = requests.put(f"{BASE_URL}/collections/{collection_name}", json={
    "vectors": {
        "size": 4,
        "distance": "Cosine"
    }
})
print(f"CREATE collection: {resp.status_code}")

# DEFECT 1: hnsw_ef=0 is silently accepted (should be >= 1)
print("\n--- DEFECT 1: hnsw_ef=0 ---")
resp = requests.post(f"{BASE_URL}/collections/{collection_name}/points/search", json={
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 5,
    "params": {
        "hnsw_ef": 0
    }
})
print(f"Status: {resp.status_code}")
if resp.status_code == 200:
    print("[DEFECT: RANGE_VIOLATION] hnsw_ef=0 accepted (should be >= 1)")
else:
    print("Correctly rejected")

# DEFECT 2: score_threshold values outside [0.0, 1.0] are silently accepted
print("\n--- DEFECT 2: score_threshold=1.5 ---")
resp = requests.post(f"{BASE_URL}/collections/{collection_name}/points/search", json={
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 5,
    "score_threshold": 1.5
})
print(f"Status: {resp.status_code}")
if resp.status_code == 200:
    print("[DEFECT: RANGE_VIOLATION] score_threshold=1.5 accepted (should be 0.0-1.0)")
else:
    print("Correctly rejected")

print("\n--- DEFECT 3: score_threshold=-0.5 ---")
resp = requests.post(f"{BASE_URL}/collections/{collection_name}/points/search", json={
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 5,
    "score_threshold": -0.5
})
print(f"Status: {resp.status_code}")
if resp.status_code == 200:
    print("[DEFECT: RANGE_VIOLATION] score_threshold=-0.5 accepted (should be 0.0-1.0)")
else:
    print("Correctly rejected")

```

## Initial Run
- **Reason**: Oracle detected: [DEFECT: ILLEGAL_SUCCESS] hnsw_ef=0 accepted
- **DB URL**: http://testvdb-db-544e450bc65c4a2382de4a4b2d7cd65d:6333

- **Evidence Excerpt**: [DEFECT: ILLEGAL_SUCCESS] hnsw_ef=0 accepted

### STDOUT
```

```

### STDERR
```

```

## Reproduction Attempts
- repro_1: Execution completed without defect markers.
