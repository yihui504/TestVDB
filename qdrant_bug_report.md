# Verified IllegalSuccess: qdrant 1.18.0

- **Target**: qdrant
- **Version**: 1.18.0
- **Defect Type**: IllegalSuccess

## Documentation Evidence
- **Source URL**: https://qdrant.tech/documentation/concepts/
- **Documented Contract Assertions**:
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
- **Surviving Assertions Under Report**:
- hnsw_ef=0 request succeeded despite documented positive hnsw_ef constraint

## Minimal Reproducible Example (MRE)
```
import requests

BASE_URL = "{{TESTVDB_DB_URL}}"

# Create a collection to work with
collection_name = "mre_defect_collection"
create_payload = {
    "vectors": {
        "size": 4,
        "distance": "Cosine"
    }
}
r = requests.put(f"{BASE_URL}/collections/{collection_name}", json=create_payload)
assert r.status_code == 200, f"Setup failed: {r.text}"

# DEFECT 1: hnsw_ef=0 silently accepted (must be >=1 per contract)
search_payload = {
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 5,
    "params": {
        "hnsw_ef": 0
    }
}
r = requests.post(f"{BASE_URL}/collections/{collection_name}/points/search", json=search_payload)
if r.status_code == 200:
    print("[DEFECT: ILLEGAL_SUCCESS] hnsw_ef=0 accepted (contract says must be >= 1)")

# DEFECT 2: score_threshold=2.0 silently accepted (must be 0.0-1.0 per contract)
search_payload = {
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 5,
    "score_threshold": 2.0
}
r = requests.post(f"{BASE_URL}/collections/{collection_name}/points/search", json=search_payload)
if r.status_code == 200:
    print("[DEFECT: ILLEGAL_SUCCESS] score_threshold=2.0 accepted (contract says 0.0-1.0)")

# DEFECT 3: score_threshold=-0.5 silently accepted (must be 0.0-1.0 per contract)
search_payload = {
    "vector": [0.1, 0.2, 0.3, 0.4],
    "limit": 5,
    "score_threshold": -0.5
}
r = requests.post(f"{BASE_URL}/collections/{collection_name}/points/search", json=search_payload)
if r.status_code == 200:
    print("[DEFECT: ILLEGAL_SUCCESS] score_threshold=-0.5 accepted (contract says 0.0-1.0)")

# DEFECT 4: Create collection with missing vectors config accepted (contract says required)
r = requests.put(f"{BASE_URL}/collections/mre_no_vectors_defect", json={})
if r.status_code == 200:
    print("[DEFECT: STATE_VIOLATION] Missing vectors config accepted (contract says required)")

```

## Rerun Instructions
Replace `{{TESTVDB_DB_URL}}` with a live target URL that matches the documented version before rerunning the script.

## Verification Summary
- **Initial Run**: Observed explicit illegal success marker.
- **Double Reproduction**: repro_1: Observed explicit illegal success marker.; repro_2: Observed explicit illegal success marker.
- **Classification Basis**: Initial run and 2 fresh-sandbox reproductions produced consistent IllegalSuccess classification and matching evidence excerpts.

## Runtime Evidence
```
Initial DB URL: http://testvdb-db-ff93202e493849baa65aa64f246ef389:6333
Initial Evidence Excerpt: [defect: illegal_success] hnsw_ef=0 accepted (contract says must be >= 1)
[defect: illegal_success] score_threshold=2.0 accepted (contract says 0.0-1.0)
[defect: illegal_success] score_threshold=-0.5 accepted (contract says 0.0-1.0)
[defect: state_violation] missing vectors config accepted (contract

Initial STDOUT:


Initial STDERR:


Reproductions:
repro_1
DB URL: http://testvdb-db-d2680561fd124ae3a3ea724fa3754969:6333
Reason: Observed explicit illegal success marker.
Evidence Excerpt: [defect: illegal_success] hnsw_ef=0 accepted (contract says must be >= 1)
[defect: illegal_success] score_threshold=2.0 accepted (contract says 0.0-1.0)
[defect: illegal_success] score_threshold=-0.5 accepted (contract says 0.0-1.0)
[defect: state_violation] missing vectors config accepted (contract
STDOUT:
[DEFECT: ILLEGAL_SUCCESS] hnsw_ef=0 accepted (contract says must be >= 1)
[DEFECT: ILLEGAL_SUCCESS] score_threshold=2.0 accepted (contract says 0.0-1.0)
[DEFECT: ILLEGAL_SUCCESS] score_threshold=-0.5 accepted (contract says 0.0-1.0)
[DEFECT: STATE_VIOLATION] Missing vectors config accepted (contract says required)
STDERR:


repro_2
DB URL: http://testvdb-db-214dd85fe59a4e4b806a8556c9dfdc2b:6333
Reason: Observed explicit illegal success marker.
Evidence Excerpt: [defect: illegal_success] hnsw_ef=0 accepted (contract says must be >= 1)
[defect: illegal_success] score_threshold=2.0 accepted (contract says 0.0-1.0)
[defect: illegal_success] score_threshold=-0.5 accepted (contract says 0.0-1.0)
[defect: state_violation] missing vectors config accepted (contract
STDOUT:
[DEFECT: ILLEGAL_SUCCESS] hnsw_ef=0 accepted (contract says must be >= 1)
[DEFECT: ILLEGAL_SUCCESS] score_threshold=2.0 accepted (contract says 0.0-1.0)
[DEFECT: ILLEGAL_SUCCESS] score_threshold=-0.5 accepted (contract says 0.0-1.0)
[DEFECT: STATE_VIOLATION] Missing vectors config accepted (contract says required)
STDERR:


```

## Independent Review
- **Summary**: Independent developer-side replay confirmed the surviving issue subset: hnsw_ef=0 request succeeded despite documented positive hnsw_ef constraint.
- **Scope**: Fresh independent replay covered collection creation, seed insert, and the narrowed Qdrant search assertions outside the LLM-generated script.

## Submission-Grade Review
- **Verdict**: SubmissionGrade
- **Summary**: All hard gates are present; the report is submission-grade under the current Phase 5 rubric.
- **Hard Gates**:
- [PASS] Documentation and contract binding: Source URL present: true; original contract assertions: 18; surviving assertions under report: 1.
- [PASS] MRE and rerun evidence: MRE placeholder present: true; runtime evidence includes replay summary: true.
- [PASS] Double reproduction and independent review: Double reproduction recorded: true; independent review recorded: true.
- **Soft Gates**:
- [PASS] Report readability: Root cause, improvement suggestions, and review scope are all present.
- **Direct-Fail Reasons**:
- None

## Root Cause Analysis
The database accepted an operation that the cited contract treats as invalid (hnsw_ef=0 request succeeded despite documented positive hnsw_ef constraint). This indicates that the server-side request validation is either missing or too permissive for the affected parameter(s), allowing the operation to proceed to a success response instead of being rejected at the boundary.

## Improvement Suggestions
Add or tighten request validation for the affected parameter(s) so the invalid operation is rejected at the boundary before success is returned. Specifically: hnsw_ef=0 request succeeded despite documented positive hnsw_ef constraint. After the fix, add a regression test that asserts the documented constraint produces a 400/422 rejection rather than 200 OK.
