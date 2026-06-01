# ef values below -1 accepted in vectorIndexConfig without validation error

## Severity

P2 — Parameter validation gap (no crash, but semantically invalid configuration silently accepted)

## Summary

Weaviate v1.37.5 accepts `ef` values below -1 (e.g., -2, -3) in `vectorIndexConfig` when creating a collection via `POST /v1/schema`. While `ef=-1` is the documented sentinel value meaning "let Weaviate pick" (see `DefaultEF`), values like `ef=-2` have no valid semantic interpretation in the HNSW algorithm and should be rejected with a 422 validation error.

This is distinct from issue #11436, which reports that `ef=-1` is accepted. The value `-1` is actually a documented sentinel, so #11436 may be by-design. However, `ef=-2` and below are clearly invalid — they are not sentinels, not documented, and have no meaningful interpretation.

## Steps to Reproduce

1. Start Weaviate v1.37.5 with anonymous access:

```bash
docker run -p 8080:8080 \
  -e AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED=true \
  -e DEFAULT_VECTORIZER_MODULE=none \
  semitechnologies/weaviate:1.37.5
```

2. Create a collection with `ef=-2`:

```python
import requests

BASE = "http://localhost:8080"

r = requests.post(f"{BASE}/v1/schema", json={
    "class": "TestEfNeg2",
    "vectorizer": "none",
    "vectorIndexConfig": {
        "distance": "cosine",
        "ef": -2
    },
    "properties": [{"name": "text", "dataType": ["text"]}]
})
print(f"Status: {r.status_code}")  # Expected: 422, Actual: 200
```

3. Verify the invalid value was stored:

```python
import time
time.sleep(0.5)
r2 = requests.get(f"{BASE}/v1/schema/TestEfNeg2")
ef = r2.json().get("vectorIndexConfig", {}).get("ef")
print(f"Stored ef: {ef}")  # Output: -2 (invalid value stored!)
```

## Expected Behavior

The server should return HTTP 422 with an error message like "ef must be -1 or >= 0". The `ef` parameter in HNSW controls the size of the dynamic list for nearest neighbors during search. The only valid negative value is `-1`, which is the documented sentinel for "automatic" (see `DefaultEF` in the source code). Any value below -1 is semantically meaningless and should be caught during schema validation.

## Actual Behavior

The server returns HTTP 200 and creates the collection with `ef=-2` stored in the configuration. No validation error is returned. This is inconsistent with how Weaviate validates other `vectorIndexConfig` parameters — for example, `maxConnections=0` and `efConstruction=0` correctly return 422.

## Relationship to Existing Issues

- **#11436**: Reports `ef=-1` accepted. However, `ef=-1` is the documented sentinel value (see PR #11439 which explicitly states "ef is intentionally left untouched — -1 is the documented 'let Weaviate pick' sentinel"). This issue is about values **below** -1, which are clearly invalid.
- **PR #11439**: Fixes validation for `dynamicEfMin`, `dynamicEfMax`, `dynamicEfFactor`, `flatSearchCutoff`, `cleanupIntervalSeconds`, `vectorCacheMaxObjects`, but **intentionally does not add validation for `ef`** because `-1` is a valid sentinel. This PR should be extended to allow `-1` but reject values below -1.

## Suggested Fix

In `entities/vectorindex/hnsw/config.go`, `(*UserConfig).validate()`, add:

```go
if uc.EF < -1 {
    errMsgs = append(errMsgs, "ef must be -1 or >= 0")
}
```

This preserves the `-1` sentinel while rejecting clearly invalid negative values.

## Developer Attitude Assessment

### Related Issues and Developer Response

| Issue/PR | State | Labels | Developer Attitude | Relevance |
|----------|-------|--------|-------------------|-----------|
| [#10771](https://github.com/weaviate/weaviate/issues/10771) PQ segments negative → DoS | **Closed (fixed)** | bug, community | **Accepted** — closed by @trengrj (Weaviate team) | Same pattern: negative parameter accepted, team fixed it |
| [#11399](https://github.com/weaviate/weaviate/issues/11399) dynamicEfMin > dynamicEfMax | **Open** | bug, community | **Accepted** — PR #11439 submitted to fix | Same pattern: invalid vectorIndexConfig accepted |
| [#11400](https://github.com/weaviate/weaviate/issues/11400) flatSearchCutoff negative | **Open** | bug, community | **Accepted** — covered by PR #11439 | Same pattern: negative parameter accepted |
| [#11436](https://github.com/weaviate/weaviate/issues/11436) ef=-1 accepted | **Open** | bug, community | **Likely by-design** — ef=-1 is documented sentinel | Our issue is distinct: ef=-2 is NOT a sentinel |
| [PR #11439](https://github.com/weaviate/weaviate/pull/11439) validate hnsw numeric ranges | **Open** (pending review) | community | **Actively developed** — by @gaurav0107, intentionally skipped ef | Directly adjacent: same validate() function, same code path |

### Key Evidence from PR #11439

PR #11439 explicitly states:

> "ef is intentionally left untouched — -1 is the documented 'let Weaviate pick' sentinel"

This confirms:
1. The PR author **acknowledged** ef validation is missing
2. The PR author **chose not to fix** ef because `-1` is a valid sentinel
3. The PR author **did not consider** values below -1 (like -2, -3)
4. The correct fix (`ef < -1` instead of `ef < 0`) was not implemented because it was out of scope

### Submission Success Rate Assessment

**Estimated acceptance probability: HIGH (80-90%)**

Reasoning:
1. **Precedent**: All similar parameter validation issues (#10771, #11399, #11400) were accepted and labeled as `bug` + `community`
2. **Distinct from #11436**: Our issue is about ef=-2 (not -1), which even PR #11439's author implicitly agrees should be validated — they just didn't implement it
3. **Minimal fix**: The suggested `if uc.EF < -1` is a one-line change in the same function PR #11439 already modifies, with zero risk of breaking existing behavior
4. **No by-design defense**: Unlike ef=-1 (which has a documented sentinel purpose), ef=-2 has no defender — no documentation, no code path, no semantic meaning
5. **Risk**: The only risk is that maintainers may decide to handle ef validation together with #11436 in a single pass, potentially marking our issue as a duplicate. However, since #11436 argues ef=-1 should be rejected (which is by-design), and our issue argues ef=-2 should be rejected (which is clearly a bug), they are substantively different

### Potential Objections and Rebuttals

| Objection | Rebuttal |
|-----------|----------|
| "Duplicate of #11436" | #11436 is about ef=-1 (documented sentinel). Our issue is about ef<-1 (no valid semantics). Different root cause, different fix |
| "Low severity, no crash" | True, but same severity as #11399/#11400 which were accepted as bugs. Invalid config silently stored can lead to unpredictable search behavior |
| "PR #11439 already handles this" | PR #11439 explicitly does NOT handle ef. It says "ef is intentionally left untouched". Our issue fills the gap PR #11439 left open |

## Self-Assessment: Defect Validity Downgrade

**⚠️ This issue has been downgraded from a standalone defect to a supplementary test case for #11436.**

After deeper analysis of the Weaviate source code, the original argument that "ef=-2 is a new defect distinct from ef=-1" does not hold up:

### Runtime Behavior Evidence

In `adapters/repos/db/vector/hnsw/search.go`, the `searchTimeEF` function:

```go
func (h *hnsw) searchTimeEF(k int) int {
    ef := int(atomic.LoadInt64(&h.ef))
    if ef < 1 {
        return h.autoEfFromK(k)  // BOTH ef=-1 AND ef=-2 take this path
    }
    if ef < k {
        ef = k
    }
    return ef
}
```

The `if ef < 1` condition means **all negative integers** (-1, -2, -100...) take the exact same `autoEfFromK` code path. There is **zero runtime behavioral difference** between ef=-1 and ef=-2.

### Why the Original Argument Fails

The original argument was: "ef=-1 is a documented sentinel, ef=-2 is not, so ef=-2 is a new bug." While semantically true (ef=-2 is not a documented sentinel), this distinction has **no practical impact** because:

1. **Same runtime behavior**: Both ef=-1 and ef=-2 produce identical search results via `autoEfFromK`
2. **Same root cause**: Both are manifestations of the same missing validation in `validate()` — the absence of `if uc.EF < -1` check
3. **Same fix**: The fix for #11436 (add ef validation) would also cover ef=-2; they are not independently fixable
4. **No distinct user impact**: No user would observe different behavior between ef=-1 and ef=-2

### Conclusion

ef=-2 is not an independent new defect. It is a supplementary test case for the same root cause as #11436 (missing ef validation). The "new defect" count for the Weaviate fuzzing campaign should be **0**, not 1.

The remaining argument for filing this issue is purely about **input validation hygiene** — the server should reject values it doesn't explicitly document as valid, even if they happen to behave identically to documented values at runtime. This is a valid but low-priority concern, not a new defect discovery.

## Environment

- Weaviate version: v1.37.5
- Deployment: Docker (single node, anonymous access)
- Vectorizer: none (manual vectors)
- Discovered by: TestVDB automated contract-driven fuzzing framework
