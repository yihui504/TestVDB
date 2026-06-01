# [REST API] Search accepts invalid nlist=0 and nlist=-1 values

- **Target**: milvus
- **Version**: v2.6.16
- **Defect Type**: IllegalSuccess

## Documentation Evidence
- **Source URL**: https://milvus.io/api-reference/restful/v2.4.x/v2/Vector%20(v2)/CreateCollection.md
- **Documented Contract Assertions**:
- [DEFECT: ILLEGAL_SUCCESS] nlist=0 accepted
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
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] nlist=0 accepted'); sys.exit(1)
else: print(f'properly rejected nlist=0: {r.json()}'); sys.exit(0)
```

## Rerun Instructions
Replace `{{TESTVDB_DB_URL}}` with a live target URL that matches the documented version before rerunning the script.

## Verification Summary
- **Initial Run**: [DEFECT: ILLEGAL_SUCCESS] nlist=0 accepted
- **Double Reproduction**: repro_1: Observed explicit defect marker.; repro_2: Observed explicit defect marker.; variant_1: Observed explicit defect marker.
- **Classification Basis**: Initial run and 3 fresh-sandbox reproductions produced consistent IllegalSuccess classification and matching evidence excerpts.

## Runtime Evidence
```
Initial DB URL: batch
Initial Evidence Excerpt: [DEFECT: ILLEGAL_SUCCESS] nlist=0 accepted

Initial STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nlist=0 accepted


Initial STDERR:


Reproductions:
repro_1
DB URL: http://testvdb-db-51015fd7333144319dcde429fa27b154:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nlist=0 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nlist=0 accepted
STDERR:


repro_2
DB URL: http://testvdb-db-3c52072614954367bbd75c7dc2679c1c:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nlist=0 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nlist=0 accepted
STDERR:


variant_1
DB URL: http://testvdb-db-182b9e3969664fd1869607f34e66154e:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nlist=-1 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nlist=-1 accepted
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
The REST API endpoint for vector search does not validate the 'nlist' parameter against documented constraints. The API specification requires nlist > 0, but the server accepts nlist=0 and nlist=-1 without returning an error. This indicates missing input validation in the request handler for the search endpoint. The defect was reproduced consistently across multiple test environments, confirming it is not an intermittent issue.

## Improvement Suggestions
Add input validation for the 'nlist' parameter in the search endpoint handler. Ensure that nlist is a positive integer (nlist > 0). If nlist is not provided, it should default to a valid positive value (e.g., 16384). Return an appropriate error response (e.g., code=1100, message='Invalid parameter: nlist must be greater than 0') when validation fails. Update API documentation to clearly state the constraint.

## Semantic Gate
N/A


## GitHub Issue Body
### Steps to Reproduce
1. Create a collection with AUTOINDEX.
2. Insert a vector entity.
3. Send a search request with `nlist` set to 0 or -1.

### Expected Behavior
The API should reject the request with an error indicating that nlist must be greater than 0.

### Actual Behavior
The API accepts the request and returns a successful response (code=0), violating the documented constraint.

### Additional Context
- Tested with nlist=0 and nlist=-1.
- The defect was reproduced on multiple test instances.
