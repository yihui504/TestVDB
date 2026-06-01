# [REST API] Search API accepts nlist=0 despite documented minimum constraint

- **Target**: milvus
- **Version**: v2.6.16
- **Defect Type**: IllegalSuccess

## Documentation Evidence
- **Source URL**: https://milvus.io/api-reference/restful/v2.4.x/v2/Vector%20(v2)/CreateCollection.md
- **Documented Contract Assertions**:
- [DEFECT: ILLEGAL_SUCCESS] nlist below min (0) accepted
- **Surviving Assertions Under Report**:
- nprobe=0 search accepted (code=0) despite documented nprobe > 0 constraint

## Minimal Reproducible Example (MRE)
```
import requests, sys, uuid, time
BASE = '{TESTVDB_DB_URL}'
HEADERS = {'Authorization': '{TESTVDB_AUTH_HEADER}', 'Content-Type': 'application/json'}
c = 'oracle_' + uuid.uuid4().hex[:8]
r = requests.post(f'{BASE}/v2/vectordb/collections/create', headers={'Authorization': '{{TESTVDB_AUTH_HEADER}}', 'Content-Type': 'application/json'}, json={"collectionName":c,"schema":{"autoID":False,"enableDynamicField":True,"fields":[{"fieldName":"id","dataType":"Int64","isPrimary":True},{"fieldName":"vector","dataType":"FloatVector","elementTypeParams":{"dim":4}}]},"indexParams":[{"fieldName":"vector","metricType":"COSINE","indexType":"AUTOINDEX"}]})
if r.json().get('code') != 0: print(f'setup failed: {r.text}'); sys.exit(0)
time.sleep(1)
r = requests.post(f'{BASE}/v2/vectordb/entities/insert', headers=HEADERS, json={"collectionName":c,"data":[{"id":1,"vector":[0.1,0.2,0.3,0.4]}]})
if r.json().get('code') != 0: print(f'insert failed: {r.text}'); sys.exit(0)
time.sleep(1)
body = {"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}
body["nlist"] = 0
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] nlist below min (0) accepted'); sys.exit(1)
else: print(f'properly rejected nlist below min (0): {r.json()}'); sys.exit(0)
```

## Rerun Instructions
Replace `{{TESTVDB_DB_URL}}` with a live target URL that matches the documented version before rerunning the script.

## Verification Summary
- **Initial Run**: [DEFECT: ILLEGAL_SUCCESS] nlist below min (0) accepted
- **Double Reproduction**: repro_1: Observed explicit defect marker.; repro_2: Observed explicit defect marker.; variant_1: Observed explicit defect marker.
- **Classification Basis**: Initial run and 3 fresh-sandbox reproductions produced consistent IllegalSuccess classification and matching evidence excerpts.

## Runtime Evidence
```
Initial DB URL: batch
Initial Evidence Excerpt: [DEFECT: ILLEGAL_SUCCESS] nlist below min (0) accepted

Initial STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nlist below min (0) accepted


Initial STDERR:


Reproductions:
repro_1
DB URL: http://testvdb-db-d30b8bf54e264c39a9783a9684b0ec58:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nlist below min (0) accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nlist below min (0) accepted
STDERR:


repro_2
DB URL: http://testvdb-db-cd1ce2e6cac147e9a295cb1d0cbc7d07:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nlist below min (0) accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nlist below min (0) accepted
STDERR:


variant_1
DB URL: http://testvdb-db-34a808d4cadb4cd38a7eeddbcabd832e:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nlist below min (-1) accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nlist below min (-1) accepted
STDERR:


```

## Independent Review
- **Summary**: Independent developer-side replay confirmed the surviving issue subset: nprobe=0 search accepted (code=0) despite documented nprobe > 0 constraint.
- **Scope**: Fresh independent replay covered collection creation, seed insert, and the narrowed milvus search assertions outside the LLM-generated script.

## Submission-Grade Review
- **Verdict**: NeedsRewrite
- **Summary**: The report is not submission-grade yet because at least one hard gate or direct-fail condition is still open.
- **Hard Gates**:
- [PASS] Documentation and contract binding: Source URL present: true; original contract assertions: 1; surviving assertions under report: 1.
- [FAIL] MRE and rerun evidence: MRE placeholder present: false; runtime evidence includes replay summary: true.
- [PASS] Double reproduction and independent review: Double reproduction recorded: true; independent review recorded: true.
- **Soft Gates**:
- [PASS] Report readability: Root cause, improvement suggestions, and review scope are all present.
- **Direct-Fail Reasons**:
- Missing hard gate: MRE and rerun evidence.

## Root Cause Analysis
The REST API endpoint for vector search does not validate the 'nlist' parameter against its documented minimum value. The API accepts nlist=0 (and even negative values like -1) without returning an error, allowing illegal success. This indicates a missing or insufficient input validation check in the request handling logic, likely in the parameter parsing or validation layer before the search operation is executed.

## Improvement Suggestions
Add explicit validation for the 'nlist' parameter in the search endpoint to reject values less than the documented minimum (e.g., 1). The validation should be performed early in the request processing pipeline, returning a clear error code and message (e.g., code=1100, message='nlist must be greater than 0'). Additionally, consider adding similar validation for other numeric parameters with documented constraints to prevent similar defects.

## Semantic Gate
N/A


## GitHub Issue Body
### Steps to Reproduce
1. Create a collection with AUTOINDEX.
2. Insert a vector entity.
3. Send a search request with `nlist` set to 0 (or a negative value).

Example request:
```
POST /v2/vectordb/entities/search
{
  "collectionName": "test_collection",
  "data": [[0.1, 0.2, 0.3, 0.4]],
  "limit": 3,
  "nlist": 0
}
```

### Expected Behavior
The API should reject the request with an appropriate error code (e.g., code=1100) and message indicating that `nlist` must be greater than 0.

### Actual Behavior
The API returns success (code=0) and performs the search, ignoring the documented constraint that `nlist` must be > 0.
