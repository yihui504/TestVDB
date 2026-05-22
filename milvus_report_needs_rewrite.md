# [REST API] Search accepts invalid nprobe=0 despite documented constraint

- **Target**: milvus
- **Version**: v2.6.16
- **Defect Type**: IllegalSuccess

## Documentation Evidence
- **Source URL**: https://milvus.io/docs/v2.6.x/
- **Documented Contract Assertions**:
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
- [IMPLICIT:REQUIRED] partitionName is required
- [IMPLICIT:REQUIRED] collectionName is required
- [IMPLICIT:REQUIRED] userName is required
- [IMPLICIT:REQUIRED] dbName is required
- [IMPLICIT:REQUIRED] objectType is required
- [IMPLICIT:REQUIRED] privilege is required
- [IMPLICIT:REQUIRED] objectName is required
- [IMPLICIT:REQUIRED] roleName is required
- [IMPLICIT:REQUIRED] data is required
- [IMPLICIT:REQUIRED] vector is required
- [IMPLICIT:REQUIRED] searchParams is required
- [IMPLICIT:REQUIRED] aliasName is required
- [IMPLICIT:REQUIRED] indexName is required
- [IMPLICIT:REQUIRED] newCollectionName is required
- [IMPLICIT:REQUIRED] partitionNames is required
- [IMPLICIT:REQUIRED] Request-Header is required
- [IMPLICIT:REQUIRED] indexParams is required
- [IMPLICIT:REQUIRED] Authorization is required
- [IMPLICIT:REQUIRED] autoID is required
- [IMPLICIT:REQUIRED] autoId is required
- [IMPLICIT:REQUIRED] enableDynamicField is required
- [IMPLICIT:REQUIRED] fields is required
- [IMPLICIT:REQUIRED] Request-Timeout is required
- [IMPLICIT:REQUIRED] filter is required
- [IMPLICIT:REQUIRED] newPassword is required
- [IMPLICIT:REQUIRED] password is required
- [IMPLICIT:REQUIRED] id is required
- **Surviving Assertions Under Report**:
- nprobe=0 search accepted (code=0) despite documented nprobe > 0 constraint

## Minimal Reproducible Example (MRE)
```
import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers={'Authorization': 'Bearer root:Milvus', 'Content-Type': 'application/json'}, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
body = {"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3,"searchParams":{"params":{"nprobe":0}}}
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted'); sys.exit(1)
else: print(f'properly rejected nprobe=0: {r.json()}'); sys.exit(0)
```

## Rerun Instructions
Replace `{{TESTVDB_DB_URL}}` with a live target URL that matches the documented version before rerunning the script.

## Verification Summary
- **Initial Run**: Oracle detected: [DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted
- **Double Reproduction**: repro_1: Observed explicit defect marker.; repro_2: Observed explicit defect marker.; variant_1: Observed explicit defect marker.
- **Classification Basis**: Initial run and 3 fresh-sandbox reproductions produced consistent IllegalSuccess classification and matching evidence excerpts.

## Runtime Evidence
```
Initial DB URL: 
Initial Evidence Excerpt: [DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted

Initial STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted

Initial STDERR:


Reproductions:
repro_1
DB URL: http://testvdb-db-da0d5fbb2b384d2f93f022d40e865e9c:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nprobe=0 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted
STDERR:


repro_2
DB URL: http://testvdb-db-09e4c39617cc4f00831b19436e785b51:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nprobe=0 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted
STDERR:


variant_1
DB URL: http://testvdb-db-e0add5bf86c34e4c8e77d7760ecf0ec1:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nprobe=-1 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=-1 accepted
STDERR:


```

## Independent Review
- **Summary**: Independent developer-side replay confirmed the surviving issue subset: nprobe=0 search accepted (code=0) despite documented nprobe > 0 constraint.
- **Scope**: Fresh independent replay covered collection creation, seed insert, and the narrowed milvus search assertions outside the LLM-generated script.

## Submission-Grade Review
- **Verdict**: NeedsRewrite
- **Summary**: The report is not submission-grade yet because at least one hard gate or direct-fail condition is still open.
- **Hard Gates**:
- [PASS] Documentation and contract binding: Source URL present: true; original contract assertions: 127; surviving assertions under report: 1.
- [FAIL] MRE and rerun evidence: MRE placeholder present: false; runtime evidence includes replay summary: true.
- [PASS] Double reproduction and independent review: Double reproduction recorded: true; independent review recorded: true.
- **Soft Gates**:
- [PASS] Report readability: Root cause, improvement suggestions, and review scope are all present.
- **Direct-Fail Reasons**:
- Missing hard gate: MRE and rerun evidence.

## Root Cause Analysis
The REST API search endpoint does not validate the `nprobe` parameter in the search request. The `nprobe` parameter is documented to require a value greater than 0, but the server accepts `nprobe=0` and returns a success response (code=0) instead of rejecting the request with an error. This indicates a missing input validation check in the request handling logic, likely in the search request parser or the underlying search execution layer.

## Improvement Suggestions
Add input validation for the `nprobe` parameter in the search endpoint. Specifically, check that `nprobe` is an integer greater than 0. If validation fails, return an appropriate error response with a non-zero code and a descriptive message. This validation should be applied at the API layer before any search execution logic is invoked.


## GitHub Issue Body
### Steps to Reproduce
1. Create a collection with an index (e.g., AUTOINDEX).
2. Insert at least one entity.
3. Send a search request with `nprobe` set to 0 in the search parameters.
4. Observe that the API returns a success response (code=0) instead of an error.

### Expected Behavior
The API should reject the request with an error code and message indicating that `nprobe` must be greater than 0.

### Actual Behavior
The API accepts `nprobe=0` and returns a successful search response (code=0).
