# Milvus REST API v2: 32768-dimension Collection Created Without Warning (OOM Risk)

## Bug Report

**Severity**: P1 (Resource Safety)  
**Version**: Milvus v2.6.16  
**Component**: REST API / Proxy  
**Status**: Not previously reported

## Description

The Milvus REST API v2 allows creating collections with dimension=32768 (the maximum supported value) without any warning or resource estimation. While 32768 is technically within the documented limit, creating such a high-dimension collection can easily cause OOM (Out of Memory) on the Milvus server, especially when combined with HNSW indexes or large data volumes.

The API should either:
1. Reject dimensions above a safe threshold (e.g., 8192) with a warning, or
2. Require explicit confirmation for high-dimension collections, or
3. At minimum, return a warning header when creating high-dimension collections

## Steps to Reproduce

```python
import requests

BASE = "http://localhost:19530"
HEADERS = {"Authorization": "Bearer root:Milvus", "Content-Type": "application/json"}

# Create collection with maximum dimension
r = requests.post(f"{BASE}/v2/vectordb/collections/create", headers=HEADERS,
                  json={"collectionName": "test_high_dim", "dimension": 32768})
print(f"32768-dim create: {r.json()}")
# Expected: At minimum a warning, or rejection for dimensions > safe threshold
# Actual: {"code": 0} (success, no warning)

# Verify collection was created
r = requests.post(f"{BASE}/v2/vectordb/collections/describe", headers=HEADERS,
                  json={"collectionName": "test_high_dim"})
print(f"Describe: {r.json()}")

# Cleanup
requests.post(f"{BASE}/v2/vectordb/collections/drop", headers=HEADERS,
              json={"collectionName": "test_high_dim"})
```

## Expected Behavior

The API should either:
1. Reject dimensions above a safe threshold with `{"code": 400, "message": "dimension 32768 exceeds safe threshold. Use at your own risk."}`, or
2. Return a warning alongside the success response

## Actual Behavior

The API accepts dimension=32768 without any validation or warning, potentially leading to OOM when data is inserted or indexed.

## Memory Estimation

For a 32768-dimension collection with HNSW index and 1M vectors:
- Raw vector data: 32768 * 4 bytes * 1M = ~128 GB
- HNSW index overhead: ~2-3x raw data
- Total: ~300-400 GB memory required

This far exceeds typical standalone Milvus deployments.

## Environment

- Milvus: v2.6.16 (standalone, Docker)
- API: REST API v2

## Impact

Creating high-dimension collections without resource estimation can:
1. Cause OOM crashes on the Milvus server
2. Affect other tenants in shared deployments
3. Lead to silent data loss if the server crashes during indexing
