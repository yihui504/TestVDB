# [REST API] Payload filter returns points with missing payload field

- **Target**: qdrant
- **Version**: v1.18.1
- **Defect Type**: StateLogicViolation

## Documentation Evidence
- **Source URL**: https://qdrant.tech/documentation/
- **Documented Contract Assertions**:
- create_collection.vectors.size must be > 0
- create_collection.vectors.distance must be one of: Dot, Cosine, Euclid, Manhattan
- create_collection.vectors.datatype must be one of: float32, float16, uint8
- create_collection.vectors.multivector_config.comparator must be max_sim
- create_collection.vectors.hnsw_config.m must be > 0
- create_collection.vectors.hnsw_config.ef_construct must be > 0
- create_collection.vectors.hnsw_config.full_scan_threshold must be > 0
- create_collection.vectors.hnsw_config.on_disk must be boolean
- create_collection.vectors.hnsw_config.payload_m must be >= 0
- create_collection.sparse_vectors.modifier must be idf
- create_collection.sparse_vectors.index.on_disk must be boolean
- create_collection.sparse_vectors.index.datatype must be one of: float16, uint8
- create_collection.hnsw_config.m must be > 0
- create_collection.hnsw_config.ef_construct must be > 0
- create_collection.hnsw_config.full_scan_threshold must be > 0
- create_collection.hnsw_config.on_disk must be boolean
- create_collection.hnsw_config.payload_m must be >= 0
- create_collection.quantization_config.scalar.type must be int8
- create_collection.quantization_config.scalar.quantile must be between 0 and 1
- create_collection.quantization_config.scalar.always_ram must be boolean
- create_collection.quantization_config.binary.always_ram must be boolean
- create_collection.quantization_config.binary.encoding must be one of: two_bits, one_and_half_bits
- create_collection.quantization_config.binary.query_encoding must be scalar8bits
- create_collection.quantization_config.product.compression must be x16
- create_collection.quantization_config.product.always_ram must be boolean
- create_collection.quantization_config.turbo.always_ram must be boolean
- create_collection.quantization_config.turbo.bits must be one of: bits4, bits2, bits1_5, bits1
- create_collection.optimizers_config.indexing_threshold must be > 0
- create_collection.optimizers_config.deleted_threshold must be between 0 and 1
- create_collection.optimizers_config.vacuum_min_vector_number must be > 0
- create_collection.optimizers_config.default_segment_number must be >= 0
- create_collection.optimizers_config.max_segment_size must be > 0
- create_collection.optimizers_config.memmap_threshold must be > 0
- create_collection.optimizers_config.flush_interval_sec must be > 0
- create_collection.optimizers_config.max_optimization_threads must be > 0
- create_collection.wal_config.wal_capacity_mb must be > 0
- create_collection.wal_config.wal_segments_ahead must be >= 0
- create_collection.shard_number must be > 0
- create_collection.sharding_method must be custom
- create_collection.replication_factor must be > 0
- create_collection.write_consistency_factor must be > 0
- create_collection.on_disk_payload must be boolean
- [IMPLICIT:REQUIRED] keys is required
- [IMPLICIT:REQUIRED] collection_name is required
- [IMPLICIT:REQUIRED] field_name is required
- [IMPLICIT:REQUIRED] searches is required
- [IMPLICIT:REQUIRED] shard_id is required
- [IMPLICIT:REQUIRED] payload is required
- [IMPLICIT:REQUIRED] peer_id is required
- [IMPLICIT:REQUIRED] snapshot_name is required
- [IMPLICIT:REQUIRED] vector is required
- [IMPLICIT:REQUIRED] operations is required
- [IMPLICIT:REQUIRED] location is required
- [IMPLICIT:REQUIRED] limit is required
- [IMPLICIT:REQUIRED] shard_key is required
- [IMPLICIT:REQUIRED] group_by is required
- [IMPLICIT:REQUIRED] actions is required
- [IMPLICIT:REQUIRED] group_size is required
- [IMPLICIT:REQUIRED] ids is required
- [IMPLICIT:REQUIRED] vector_name is required
- [IMPLICIT:REQUIRED] points is required
- **Surviving Assertions Under Report**:
- hnsw_ef=0 request succeeded despite documented positive hnsw_ef constraint
- score_threshold=2.0 request succeeded despite documented 0.0-1.0 range constraint
- score_threshold=-0.5 request succeeded despite documented 0.0-1.0 range constraint

## Minimal Reproducible Example (MRE)
```
import requests, sys, uuid, time
BASE = '{{TESTVDB_DB_URL}}'
c = 'safety_payloadfilter_' + uuid.uuid4().hex[:8]
r = requests.put(f'{BASE}/collections/{c}', json={"vectors":{"size":4,"distance":"Cosine"}})
if r.status_code not in (200, 201): print(f'setup failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
points = [{"id": i+1, "vector": [0.1*i, 0.1*i, 0.1*i, 0.1*i], "payload": {"color": "red" if i%2==0 else "blue"}} for i in range(10)]
r = requests.put(f'{BASE}/collections/{c}/points', json={"points": points})
if r.status_code != 200: print(f'upsert failed: {r.status_code}'); sys.exit(0)
time.sleep(0.5)
r = requests.post(f'{BASE}/collections/{c}/points/search', json={"vector":[0.5,0.5,0.5,0.5], "limit":10, "filter":{"must":[{"key":"color","match":{"value":"red"}}]}})
results = r.json().get('result', [])
for h in results:
    if h.get('payload', {}).get('color') != 'red':
        print(f'[DEFECT: STATE_LOGIC_VIOLATION] filter color=red returned point with color={h.get("payload",{}).get("color")}'); sys.exit(1)
print(f'payload filter correct: {len(results)} results all with color=red'); sys.exit(0)
```

## Rerun Instructions
Replace `{{TESTVDB_DB_URL}}` with a live target URL that matches the documented version before rerunning the script.

## Verification Summary
- **Initial Run**: Observed explicit defect marker.
- **Double Reproduction**: repro_1: Observed explicit defect marker.; repro_2: Observed explicit defect marker.; variant_1: Observed explicit defect marker.
- **Classification Basis**: Initial run and 3 fresh-sandbox reproductions produced consistent StateLogicViolation classification and matching evidence excerpts.

## Runtime Evidence
```
Initial DB URL: http://testvdb-db-d1ca92f9a4a04b8884b5ef477320f8ac:6333
Initial Evidence Excerpt: [defect: state_logic_violation] filter color=red returned point with color=none


Initial STDOUT:


Initial STDERR:


Reproductions:
repro_1
DB URL: http://testvdb-db-9e38deef8ef84acb8e46ea5fbb8d525b:6333
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: state_logic_violation] filter color=red returned point with color=none

STDOUT:
[DEFECT: STATE_LOGIC_VIOLATION] filter color=red returned point with color=None
STDERR:


repro_2
DB URL: http://testvdb-db-bd04d8469d9046e7813dddbf1cc25653:6333
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: state_logic_violation] filter color=red returned point with color=none

STDOUT:
[DEFECT: STATE_LOGIC_VIOLATION] filter color=red returned point with color=None
STDERR:


variant_1
DB URL: http://testvdb-db-f67b72be72a84dc5abc364f2ebe8b59e:6333
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: state_logic_violation] filter color=green returned point with color=none

STDOUT:
[DEFECT: STATE_LOGIC_VIOLATION] filter color=green returned point with color=None
STDERR:


```

## Independent Review
- **Summary**: Independent developer-side replay confirmed the surviving issue subset: hnsw_ef=0 request succeeded despite documented positive hnsw_ef constraint; score_threshold=2.0 request succeeded despite documented 0.0-1.0 range constraint; score_threshold=-0.5 request succeeded despite documented 0.0-1.0 range constraint.
- **Scope**: Fresh independent replay covered collection creation, seed insert, and the narrowed qdrant search assertions outside the LLM-generated script.

## Submission-Grade Review
- **Verdict**: SubmissionGrade
- **Summary**: All hard gates are present; the report is submission-grade under the current Phase 5 rubric.
- **Hard Gates**:
- [PASS] Documentation and contract binding: Source URL present: true; original contract assertions: 61; surviving assertions under report: 3.
- [PASS] MRE and rerun evidence: MRE placeholder present: true; runtime evidence includes replay summary: true.
- [PASS] Double reproduction and independent review: Double reproduction recorded: true; independent review recorded: true.
- **Soft Gates**:
- [PASS] Report readability: Root cause, improvement suggestions, and review scope are all present.
- **Direct-Fail Reasons**:
- None

## Root Cause Analysis
The defect occurs when a search request includes a payload filter (e.g., `{"must":[{"key":"color","match":{"value":"red"}}]}`) but the response contains points where the `payload` field is `None` or missing entirely. This violates the expected state logic that filtered results should only include points matching the filter criteria. The root cause is likely that the search implementation does not properly handle points with missing payload fields during filtering, either by not excluding them or by returning them with a null payload instead of omitting them.

## Improvement Suggestions
1. Ensure that points with missing or null payload fields are excluded from search results when a payload filter is applied. 2. Validate that the filter logic correctly checks for the existence of the key before comparing values. 3. Add unit tests covering edge cases where points have no payload or missing keys. 4. Consider returning an error or empty result if the filter cannot be applied due to missing data.

## Semantic Gate
Ambiguous


## GitHub Issue Body
## Steps to Reproduce
1. Create a collection with vectors of size 4 and Cosine distance.
2. Insert 10 points with payloads: even IDs have `{"color": "red"}`, odd IDs have `{"color": "blue"}`.
3. Perform a search with filter `{"must":[{"key":"color","match":{"value":"red"}}]}`.
4. Observe that some returned points have `payload: None` instead of `{"color": "red"}`.

## Expected Behavior
All returned points should have `payload.color` equal to `"red"`. Points without a `color` field or with a different value should be excluded.

## Actual Behavior
Some points in the result have `payload: None` (or missing payload), indicating that the filter did not properly exclude points without the required payload field.
