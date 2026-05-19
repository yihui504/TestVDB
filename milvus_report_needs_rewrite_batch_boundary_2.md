# Verified IllegalSuccess: milvus 2.6.16

- **Target**: milvus
- **Version**: 2.6.16
- **Defect Type**: IllegalSuccess

## Documentation Evidence
- **Source URL**: https://milvus.io/api-reference/restful/v2.4.x/v2/Vector%20(v2)/CreateCollection.md
- **Documented Contract Assertions**:
- [DEFECT: ILLEGAL_SUCCESS] searchnprobe below min (0) accepted
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
body = {"collectionName":c,"data":[[0.1,0.2,0.3,0.4]],"limit":3}
body["searchnprobe"] = 0
r = requests.post(f'{BASE}/v2/vectordb/entities/search', headers=HEADERS, json=body)
if r.json().get('code') == 0: print(f'[DEFECT: ILLEGAL_SUCCESS] searchnprobe below min (0) accepted'); sys.exit(1)
else: print(f'properly rejected searchnprobe below min (0): {r.json()}'); sys.exit(0)
```

## Rerun Instructions
Replace `{{TESTVDB_DB_URL}}` with a live target URL that matches the documented version before rerunning the script.

## Verification Summary
- **Initial Run**: [DEFECT: ILLEGAL_SUCCESS] searchnprobe below min (0) accepted
- **Double Reproduction**: repro_1: Observed explicit defect marker.; repro_2: Observed explicit defect marker.
- **Classification Basis**: Initial run and 2 fresh-sandbox reproductions produced consistent IllegalSuccess classification and matching evidence excerpts.

## Runtime Evidence
```
Initial DB URL: batch
Initial Evidence Excerpt: [DEFECT: ILLEGAL_SUCCESS] searchnprobe below min (0) accepted

Initial STDOUT:
[DEFECT: ILLEGAL_SUCCESS] searchnprobe below min (0) accepted


Initial STDERR:


Reproductions:
repro_1
DB URL: http://testvdb-db-39671a3c583a43fbadc13f83f4bdc0b2:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] searchnprobe below min (0) accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] searchnprobe below min (0) accepted
STDERR:


repro_2
DB URL: http://testvdb-db-1fdd646b6caf4ec28a79b45b7b634dc6:19530
Reason: Observed explicit defect marker.
Evidence Excerpt: [defect: illegal_success] searchnprobe below min (0) accepted

STDOUT:
[DEFECT: ILLEGAL_SUCCESS] searchnprobe below min (0) accepted
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
The database accepted an operation that the cited contract treats as invalid (nprobe=0 search accepted (code=0) despite documented nprobe > 0 constraint). This indicates that the server-side request validation is either missing or too permissive for the affected parameter(s), allowing the operation to proceed to a success response instead of being rejected at the boundary.

## Improvement Suggestions
Add or tighten request validation for the affected parameter(s) so the invalid operation is rejected at the boundary before success is returned. Specifically: nprobe=0 search accepted (code=0) despite documented nprobe > 0 constraint. After the fix, add a regression test that asserts the documented constraint produces a 400/422 rejection rather than 200 OK.
