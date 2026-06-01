# WITHDRAWN: Concurrent Insert Returns Invalid Count (-1)

## ⚠️ VERIFICATION RESULT: FALSE POSITIVE (Tool Bug)

**Original Severity**: P0 (Data Integrity)  
**Actual Status**: **TestVDB tool bug, NOT a Milvus bug**

## Verification Evidence (2026-05-26)

### Root Cause Analysis

The `-1` count was caused by TestVDB using the **wrong API endpoint** to retrieve `rowCount`:

```python
# TestVDB's code (semantic.rs:278, sequence_gen.rs:354):
r2 = requests.post(f'{BASE}/v2/vectordb/collections/describe', ...)
count = r2.json().get('data', {}).get('rowCount', -1)
#                                                          ^^ default value used!
```

The `describe` endpoint **does not return `rowCount`** in v2.6.16. The `.get('rowCount', -1)` returns the default value `-1` because the key doesn't exist in the response.

### Actual Milvus Behavior

| Endpoint | Returns rowCount? | Value |
|----------|-------------------|-------|
| `collections/describe` | **No** | Key not in response |
| `collections/get_stats` | Yes | `0` (inaccurate but not -1) |
| `entities/query` + count | Yes | `40` (correct) |

### Reproduction

```python
import requests, threading, time

BASE = "http://localhost:19530"
H = {"Authorization": "Bearer root:Milvus", "Content-Type": "application/json"}

# Create + concurrent insert (4 threads x 10 entities)
requests.post(f"{BASE}/v2/vectordb/collections/create", headers=H,
              json={"collectionName": "test", "dimension": 4})
# ... insert 40 entities concurrently ...
time.sleep(5)

# describe: rowCount NOT in response
r = requests.post(f"{BASE}/v2/vectordb/collections/describe", headers=H,
                  json={"collectionName": "test"})
print(r.json().get("data", {}).get("rowCount", -1))  # prints -1 (default!)

# get_stats: rowCount=0 (inaccurate)
r = requests.post(f"{BASE}/v2/vectordb/collections/get_stats", headers=H,
                  json={"collectionName": "test"})
print(r.json().get("data", {}).get("rowCount"))  # prints 0

# query: correct count
r = requests.post(f"{BASE}/v2/vectordb/entities/query", headers=H,
                  json={"collectionName": "test", "filter": "id >= 0"})
print(len(r.json().get("data", [])))  # prints 40
```

### Remaining Issue

While `-1` is a tool bug, `get_stats.rowCount=0` when data exists (query returns 40) **may be a real Milvus issue** — rowCount is stale/inaccurate after concurrent inserts. However, this is:
1. A known issue (PR #45147, #45981, Issue #48897)
2. Severity P2 (stale count) rather than P0 (corruption)
3. The count eventually converges after flush

### Tool Fix Required

TestVDB should use `get_stats` endpoint instead of `describe` for `rowCount`:
```python
# Fix: use get_stats instead of describe
r2 = requests.post(f'{BASE}/v2/vectordb/collections/get_stats', headers=HEADERS, json={"collectionName": c})
count = r2.json().get('data', {}).get('rowCount', -1)
```
