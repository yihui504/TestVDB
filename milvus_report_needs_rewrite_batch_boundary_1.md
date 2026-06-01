# [REST API] Search accepts invalid nprobe=0 despite documented constraint

- **Target**: milvus
- **Version**: v2.6.16
- **Defect Type**: IllegalSuccess

## Documentation Evidence
- **Source URL**: https://milvus.io/api-reference/restful/v2.4.x/v2/Vector%20(v2)/CreateCollection.md
- **Documented Contract Assertions**:
- [DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted
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
body["nprobe"] = 0
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted'); sys.exit(1)
else: print(f'properly rejected nprobe=0: {r.json()}'); sys.exit(0)
```

## Rerun Instructions
Replace `{{TESTVDB_DB_URL}}` with a live target URL that matches the documented version before rerunning the script.

## Verification Summary
- **Initial Run**: [DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted
- **Double Reproduction**: repro_1: Observed explicit defect marker.; repro_2: Observed explicit defect marker.; variant_1: Observed explicit defect marker.
- **Classification Basis**: Initial run and 3 fresh-sandbox reproductions produced consistent IllegalSuccess classification and matching evidence excerpts.

## Runtime Evidence
```
Initial DB URL: batch
Initial Evidence Excerpt: [DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted

Initial STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted


Initial STDERR:


Reproductions:
repro_1
DB URL: http://testvdb-db-f8c3b71977fc4795911f8bfaee0ab1ff:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nprobe=0 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted
STDERR:


repro_2
DB URL: http://testvdb-db-9798025b46544c68910c869552fad5ed:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] nprobe=0 accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] nprobe=0 accepted
STDERR:


variant_1
DB URL: http://testvdb-db-d4346e851aec4948850ae6310ea248f1:19530
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
- [PASS] Documentation and contract binding: Source URL present: true; original contract assertions: 1; surviving assertions under report: 1.
- [FAIL] MRE and rerun evidence: MRE placeholder present: false; runtime evidence includes replay summary: true.
- [PASS] Double reproduction and independent review: Double reproduction recorded: true; independent review recorded: true.
- **Soft Gates**:
- [PASS] Report readability: Root cause, improvement suggestions, and review scope are all present.
- **Direct-Fail Reasons**:
- Missing hard gate: MRE and rerun evidence.

## Root Cause Analysis
The REST API endpoint for vector search does not validate the `nprobe` parameter against the documented constraint that it must be greater than 0. The server accepts requests with `nprobe=0` and returns a success response (code=0), which contradicts the API specification. This indicates a missing input validation check in the request handling logic, likely in the search request parser or the underlying service layer. The same issue also allows negative values like `nprobe=-1`.

## Improvement Suggestions
Add input validation for the `nprobe` parameter in the search endpoint to reject values <= 0. The validation should be performed early in the request processing pipeline, returning an appropriate error code (e.g., code=1100 for invalid argument) and a descriptive message. Additionally, consider adding similar validation for other numeric parameters that have documented constraints.

## Semantic Gate
N/A


## GitHub Issue Body
### Steps to Reproduce
1. Create a collection with a vector field and insert a vector.
2. Send a search request with `nprobe` set to 0.
3. Observe that the API returns success (code=0) instead of rejecting the request.

### Expected Behavior
The API should reject requests with `nprobe=0` and return an error code (e.g., 1100) with a message indicating that `nprobe` must be greater than 0.

### Actual Behavior
The API accepts `nprobe=0` and returns a successful search response (code=0). This violates the documented constraint that `nprobe` must be > 0.
