# WITHDRAWN: Duplicate ID Insert Returns Invalid Entity Count (-1)

## ⚠️ VERIFICATION RESULT: FALSE POSITIVE (Tool Bug + Misunderstood Semantics)

**Original Severity**: P0 (Data Integrity)  
**Actual Status**: **TestVDB tool bug + Milvus insert is NOT upsert**

## Verification Evidence (2026-05-26)

### Root Cause 1: Tool Bug (same as Issue 001)

The `-1` count was caused by TestVDB using `collections/describe` to get `rowCount`, but `describe` does not return `rowCount` in v2.6.16. See Issue 001 for details.

### Root Cause 2: Milvus Insert Semantics

Milvus `insert` is **NOT upsert**. Duplicate PK inserts create additional rows (row_count increases), and search deduplicates by PK at query time.

**Evidence from Issue #49509** (Milvus collaborator comment):
> "insert is not upsert in Milvus — Batch 2 stores 3000 additional vectors under the same PKs (row_count = 6000)"
> "search retrieves top-L candidates by distance per segment, then deduplicates by PK during reduce, keeping the latest entity"
> "The behavior matches the documented Milvus insert/search semantics — not a server bug"

### Actual Milvus Behavior

```python
import requests, time

BASE = "http://localhost:19530"
H = {"Authorization": "Bearer root:Milvus", "Content-Type": "application/json"}

# Insert same ID twice
requests.post(f"{BASE}/v2/vectordb/collections/create", headers=H,
              json={"collectionName": "test", "dimension": 4})
requests.post(f"{BASE}/v2/vectordb/entities/insert", headers=H,
              json={"collectionName": "test", "data": [{"id": 1, "vector": [0.1,0.2,0.3,0.4]}]})
requests.post(f"{BASE}/v2/vectordb/entities/insert", headers=H,
              json={"collectionName": "test", "data": [{"id": 1, "vector": [0.5,0.6,0.7,0.8]}]})
time.sleep(5)

# get_stats: rowCount=0 (stale, but not -1)
r = requests.post(f"{BASE}/v2/vectordb/collections/get_stats", headers=H,
                  json={"collectionName": "test"})
print(r.json())  # {"data": {"rowCount": 0}}

# query: returns 1 entity (deduplicated by PK)
r = requests.post(f"{BASE}/v2/vectordb/entities/query", headers=H,
                  json={"collectionName": "test", "filter": "id >= 0"})
print(len(r.json().get("data", [])))  # 1 (deduplicated)
```

### Conclusion

1. `-1` is a TestVDB tool bug (wrong endpoint for rowCount)
2. Duplicate ID insert behavior is **by design** in Milvus (insert ≠ upsert)
3. `get_stats.rowCount=0` when data exists is a known stale count issue (PR #45147, Issue #48897)

## Related Issues

- #49509: Documents Milvus insert/search semantics (insert is NOT upsert)
- #48897: count(*) returns inaccurate results (known stale count issue)
- #45147/#45981: Fixes for rowCount staleness
