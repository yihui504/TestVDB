# [REST API] Search API accepts invalid nprobe values (negative and zero) despite documented constraint

- **Target**: milvus
- **Version**: v2.6.16
- **Defect Type**: IllegalSuccess

## Documentation Evidence
- **Source URL**: https://milvus.io/api-reference/restful/v2.4.x/v2/Vector%20(v2)/CreateCollection.md
- **Documented Contract Assertions**:
- [DEFECT: ILLEGAL_SUCCESS] nprobe=-1 accepted
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
body["nprobe"] = -1
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] nprobe=-1 accepted'); sys.exit(1)
else: print(f'properly rejected nprobe=-1: {r.json()}'); sys.exit(0)
```

## Rerun Instructions
Replace `{{TESTVDB_DB_URL}}` with a live target URL that matches the documented version before rerunning the script.

## Verification Summary
- **Initial Run**: [DEFECT: ILLEGAL_SUCCESS] nprobe=-1 accepted
- **Double Reproduction**: repro_1: Observed explicit defect marker.; repro_2: Observed explicit defect marker.; variant_1: Observed explicit defect marker.
- **Classification Basis**: Initial run and 3 fresh-sandbox reproductions produced consistent IllegalSuccess classification and matching evidence excerpts.

## Runtime Evidence
```
Initial DB URL: batch
Initial Evidence Excerpt: [DEFECT: ILLEGAL_SUCCESS] nprobe=-1 accepted

Initial STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=-1 accepted


Initial STDERR:


Reproductions:
repro_1
DB URL: http://testvdb-db-2da5503773d645aaada2c537ad79a3d9:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nprobe=-1 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=-1 accepted
STDERR:


repro_2
DB URL: http://testvdb-db-1273a8c695f94c76a0ec582194505aeb:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nprobe=-1 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=-1 accepted
STDERR:


variant_1
DB URL: http://testvdb-db-050174cb2d07467b9b1a0c0c735e3684:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nprobe=0 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted
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
The REST API endpoint for vector search does not validate the `nprobe` parameter against the documented constraint that it must be greater than 0. The server accepts negative values (e.g., -1) and zero without returning an error code, leading to an illegal success state. This indicates a missing input validation check in the request handling layer, likely in the parameter parsing or validation middleware.

## Improvement Suggestions
Add input validation for the `nprobe` parameter in the search endpoint to reject values <= 0. The validation should check that `nprobe` is an integer greater than 0 and return an appropriate error code (e.g., 1100 for invalid parameter) with a descriptive message. This can be implemented in the REST API handler or a shared validation utility.

## Semantic Gate
N/A


## GitHub Issue Body
## Steps to Reproduce
1. Create a collection with AUTOINDEX.
2. Insert a vector entity.
3. Send a search request with `nprobe` set to -1 or 0.

Example request:
```
POST /v2/vectordb/entities/search
{
  "collectionName": "test_collection",
  "data": [[0.1, 0.2, 0.3, 0.4]],
  "limit": 3,
  "nprobe": -1
}
```

## Expected Behavior
The API should reject the request with an error code (e.g., 1100) and a message indicating that `nprobe` must be greater than 0.

## Actual Behavior
The API returns success (code=0) even when `nprobe` is -1 or 0, violating the documented constraint.
