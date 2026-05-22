# Candidate Defect: IllegalSuccess

- **Target**: milvus
- **Version**: v2.4.17
- **Status**: Rejected
- **Downgrade Reason**: repro_1 failed verification: Generated script failed due to Python/runtime authoring errors or test-infrastructure uncertainty.

## Documentation Evidence
- **Source URL**: https://milvus.io/docs/v2.6.x/
- **Contract Assertions**:
- [SEARCH] limit must be > 0
- [SEARCH] offset must be >= 0
- [SEARCH] data vector dimension must match collection dim
- [SEARCH] filter expression must be valid SQL-like syntax
- [SEARCH] searchParams.params.nprobe must be > 0
- [SEARCH] vector elements must be valid finite numbers (no NaN, no Infinity)
- [SEARCH] search against non-existent collectionName must return error (code != 0)
- [CREATE] collectionName must be non-empty
- [CREATE] dim must be > 0 and <= 32768
- [CREATE] dim must not be 0 or negative
- [CREATE] metricType must be one of: L2, IP, COSINE, HAMMING, JACCARD
- [CREATE] invalid metricType must be rejected with clear error
- [CREATE] indexType must be a valid index type
- [CREATE] invalid indexType must be rejected with clear error
- [CREATE] duplicate collectionName must return a clear conflict error
- [BEHAVIOR:STATE] insert N entities → rowCount must equal N
- [BEHAVIOR:STATE] delete M of N entities → rowCount must equal N-M
- [BEHAVIOR:STATE] upsert same entity ID twice → rowCount must NOT increase
- [BEHAVIOR:SEMANTIC] search results must have distance in descending order (COSINE/IP) or ascending (L2)
- [BEHAVIOR:SEMANTIC] limit=1 must return exactly 1 result
- [BEHAVIOR:SEMANTIC] offset beyond total entities must return empty results
- [BEHAVIOR:STATE] delete non-existent entity IDs → rowCount must NOT change
- [BEHAVIOR:STATE] drop collection then recreate with same name → must succeed
- [BEHAVIOR:DIAGNOSTIC] when limit=0 is rejected, error message must mention 'limit'
- [INDEX:CREATE] invalid indexType must be rejected with clear error
- [INDEX:CREATE] nlist must be > 0 for IVF_FLAT index
- [INDEX:CREATE] M must be > 0 for HNSW index
- [INDEX:CREATE] efConstruction must be > 0 for HNSW index
- [INDEX:CREATE] metricType must be compatible with vector field type (HAMMING/JACCARD require BinaryVector)
- [INDEX:CREATE] indexType must be one of: AUTOINDEX, HNSW, IVF_FLAT, IVF_SQ8, DISKANN
- [HYBRID:SEARCH] searchParams must be a non-empty array
- [HYBRID:SEARCH] rerank strategy must be one of: rrf, weighted
- [HYBRID:SEARCH] rerank weights must be non-negative
- [INDEX:DROP] dropping nonexistent index must return error (code != 0)
- [INDEX:DROP] collectionName must be non-empty
- [INDEX:DESCRIBE] describing nonexistent index must return error (code != 0)
- [INDEX:DESCRIBE] collectionName must be non-empty for index describe
- [PARTITION:CREATE] partitionName must be non-empty
- [PARTITION:CREATE] partitionName must not contain special characters or SQL injection
- [PARTITION:CREATE] duplicate partitionName must be rejected
- [PARTITION:DROP] partitionName must be non-empty
- [PARTITION:DROP] dropping nonexistent partition must return error (code != 0)
- [COLLECTION:RENAME] newCollectionName must be non-empty
- [COLLECTION:RENAME] collectionName must be non-empty for rename
- [COLLECTION:RENAME] renaming to existing collection name must be rejected
- [COLLECTION:ALTER_PROPERTIES] properties must be non-empty
- [COLLECTION:ALTER_PROPERTIES] collection.ttl.seconds must be non-negative
- [COLLECTION:ALTER_PROPERTIES] invalid property keys must be rejected
- [COLLECTION:FIELDS:ADD] fieldName must be non-empty
- [COLLECTION:FIELDS:ADD] adding field with duplicate fieldName must be rejected
- [COLLECTION:FIELDS:ADD] adding vector field to collection that already has a vector field must be rejected
- [ENTITIES:GET] id array must be non-empty
- [ENTITIES:GET] nonexistent entity IDs must return empty or partial results, not error
- [ALIAS:CREATE] aliasName must be non-empty
- [ALIAS:CREATE] aliasName must not conflict with existing collection name
- [ALIAS:ALTER] alias alter to nonexistent collection must return error (code != 0)
- [ALIAS:ALTER] aliasName must be non-empty for alter
- [ALIAS:DROP] aliasName must be non-empty for drop
- [ALIAS:DROP] dropping nonexistent alias must return error (code != 0)
- [DATABASE:CREATE] dbName must be non-empty
- [DATABASE:CREATE] duplicate dbName must be rejected
- [DATABASE:CREATE] dbName must not contain special characters
- [DATABASE:DROP] dropping default database must be rejected
- [DATABASE:DROP] dropping nonexistent database must return error (code != 0)
- [DATABASE:DROP] dbName must be non-empty
- [DATABASE:LIST] invalid parameters must be rejected
- [COLLECTION:LIST] dbName must be valid if provided
- [COLLECTION:HAS] checking nonexistent collection must return has=false, not error
- [COLLECTION:GET_STATS] collectionName must be non-empty for get_stats
- [COLLECTION:LOAD] loading nonexistent collection must return error (code != 0)
- [COLLECTION:LOAD] collectionName must be non-empty for load
- [COLLECTION:RELEASE] releasing unloaded collection must return error or be idempotent
- [COLLECTION:RELEASE] collectionName must be non-empty for release
- [COLLECTION:FLUSH] collectionName must be non-empty for flush
- [COLLECTION:COMPACT] collectionName must be non-empty for compact
- [INDEX:LIST] collectionName must be non-empty for index list
- [PARTITION:LIST] collectionName must be non-empty for partition list
- [PARTITION:HAS] checking nonexistent partition must return has=false, not error
- [ALIAS:LIST] collectionName must be valid for alias list
- [COLLECTION:LIST] empty dbName must be handled gracefully
- [CREATE:MUTATION] type confusion on dim field must be rejected
- [CREATE:MUTATION] null collectionName must be rejected
- [CREATE:MUTATION] missing collectionName must be rejected
- [CREATE:MUTATION] boundary float dim must be rejected
- [INSERT:MUTATION] type confusion on data field must be rejected
- [INSERT:MUTATION] null data must be rejected
- [INSERT:MUTATION] oversized data payload must be rejected
- [INSERT:MUTATION] boundary float in vector data must be rejected
- [SEARCH:MUTATION] type confusion on limit must be rejected
- [SEARCH:MUTATION] null data must be rejected
- [SEARCH:MUTATION] oversized limit must be rejected
- [SEARCH:MUTATION] boundary float in vector data must be rejected
- [QUERY:MUTATION] type confusion on limit must be rejected
- [QUERY:MUTATION] null filter must be rejected
- [QUERY:MUTATION] oversized limit must be rejected
- [QUERY:MUTATION] boundary float limit must be rejected
- [UPSERT:MUTATION] type confusion on data field must be rejected
- [UPSERT:MUTATION] null data must be rejected
- [UPSERT:MUTATION] oversized data payload must be rejected
- [UPSERT:MUTATION] boundary float in vector data must be rejected
- [IMPLICIT:REQUIRED] userName is required
- [IMPLICIT:REQUIRED] partitionNames is required
- [IMPLICIT:REQUIRED] collectionName is required
- [IMPLICIT:REQUIRED] id is required
- [IMPLICIT:REQUIRED] Request-Timeout is required
- [IMPLICIT:REQUIRED] dbName is required
- [IMPLICIT:REQUIRED] indexName is required
- [IMPLICIT:REQUIRED] autoID is required
- [IMPLICIT:REQUIRED] autoId is required
- [IMPLICIT:REQUIRED] enableDynamicField is required
- [IMPLICIT:REQUIRED] fields is required
- [IMPLICIT:REQUIRED] vector is required
- [IMPLICIT:REQUIRED] searchParams is required
- [IMPLICIT:REQUIRED] objectType is required
- [IMPLICIT:REQUIRED] privilege is required
- [IMPLICIT:REQUIRED] objectName is required
- [IMPLICIT:REQUIRED] roleName is required
- [IMPLICIT:REQUIRED] data is required
- [IMPLICIT:REQUIRED] indexParams is required
- [IMPLICIT:REQUIRED] Request-Header is required
- [IMPLICIT:REQUIRED] filter is required
- [IMPLICIT:REQUIRED] newCollectionName is required
- [IMPLICIT:REQUIRED] password is required
- [IMPLICIT:REQUIRED] aliasName is required
- [IMPLICIT:REQUIRED] Authorization is required
- [IMPLICIT:REQUIRED] newPassword is required

## MRE
```
import requests, json, uuid, sys, time

BASE_URL = "{{TESTVDB_DB_URL}}"
HEADERS = {"Authorization": "Bearer root:Milvus", "Content-Type": "application/json"}

# Generate unique collection name
suffix = str(uuid.uuid4())[:8]
collection_name = f"test_search_params_null_{suffix}"

# SETUP: Create collection
r = requests.post(f"{BASE_URL}/v2/vectordb/collections/create",
    json={"collectionName": collection_name, "dimension": 8, "metricType": "L2",
          "idType": "int64", "primaryFieldName": "id", "vectorFieldName": "vector"},
    headers=HEADERS)
assert r.json().get('code') == 0, f"Create failed: {r.json()}"
time.sleep(0.5)

# Insert some data
r = requests.post(f"{BASE_URL}/v2/vectordb/entities/insert",
    json={"collectionName": collection_name, "data": [
        {"id": 1, "vector": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]},
        {"id": 2, "vector": [0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.2, 0.1]}
    ]},
    headers=HEADERS)
assert r.json().get('code') == 0, f"Insert failed: {r.json()}"
time.sleep(0.3)

# TEST 1: Search with searchParams.radius=null (TYPE_CONFUSION - should be a number)
r = requests.post(f"{BASE_URL}/v2/vectordb/entities/search",
    json={"collectionName": collection_name, "data": [[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]],
          "limit": 2, "searchParams": {"radius": None}},
    headers=HEADERS)
result = r.json()
print(f"Search with radius=null: {result}")

if result.get('code') == 0:
    print("[DEFECT: ILLEGAL_SUCCESS] Server accepted searchParams.radius=null (should be rejected)")
    sys.exit(1)

# TEST 2: Search with searchParams.range_filter=null (TYPE_CONFUSION - should be a number)
r = requests.post(f"{BASE_URL}/v2/vectordb/entities/search",
    json={"collectionName": collection_name, "data": [[0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8]],
          "limit": 2, "searchParams": {"range_filter": None, "radius": 0.5}},
    headers=HEADERS)
result = r.json()
print(f"Search with range_filter=null: {result}")

if result.get('code') == 0:
    print("[DEFECT: ILLEGAL_SUCCESS] Server accepted searchParams.range_filter=null (should be rejected)")
    sys.exit(1)

print("All tests passed - server correctly rejected null values")
sys.exit(0)

```

## Initial Run
- **Reason**: Observed explicit defect marker.
- **DB URL**: http://testvdb-db-0fab15af556b4f0c88db9b5a3db0dae5:19530

- **Evidence Excerpt**: search with radius=null: {'code': 0, 'cost': 0, 'data': [{'distance': 0, 'id': 1}, {'distance': 1.68, 'id': 2}]}
[defect: illegal_success] server accepted searchparams.radius=null (should be rejected)


### STDOUT
```
Search with radius=null: {'code': 0, 'cost': 0, 'data': [{'distance': 0, 'id': 1}, {'distance': 1.68, 'id': 2}]}
[DEFECT: ILLEGAL_SUCCESS] Server accepted searchParams.radius=null (should be rejected)
```

### STDERR
```

```

## Reproduction Attempts
- repro_1: Generated script failed due to Python/runtime authoring errors or test-infrastructure uncertainty.
