# [Bug]: REST API v2 `aliases/list` accepts empty string for collectionName while other endpoints properly reject it

### Is there an existing issue for this?

- [x] I have searched the existing issues (similar: #49889 for dbName; #49843 for TTL)

### Environment

```markdown
- Milvus version: v2.6.16
- Deployment mode(standalone or cluster): standalone
- MQ type(rocksmq, pulsar or kafka): rocksmq
- SDK version(e.g. pymilvus v2.0.0rc2): REST API v2 (via curl)
- OS(Ubuntu or CentOS): Docker on Windows
- CPU/Memory: N/A
- GPU: N/A
- Others: Reproduced via Milvus REST API v2 (`/v2/vectordb/aliases/list`)
```

### Current Behavior

The `POST /v2/vectordb/aliases/list` endpoint accepts an empty string for the `collectionName` parameter and returns `code=0` (success). This is inconsistent with other endpoints that accept `collectionName` — most of which **correctly reject** empty strings with `code=1802` and a validation error message.

**aliases/list with collectionName="" (BUG):**
```bash
curl -s -X POST 'http://localhost:19530/v2/vectordb/aliases/list' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{"collectionName":"", "dbName":"default"}'
# Response: {"code":0, "data":[]}
```

**For contrast — most other endpoints correctly reject empty collectionName:**

```bash
# collections/create — properly rejected
curl ... -d '{"collectionName":"","dimension":4,"metricType":"L2"}'
# {"code":1802,"message":"missing required parameters, error: Key: 'CollectionReq.CollectionName' Error:Field validation for 'CollectionName' failed on the 'required' tag"}

# collections/describe — properly rejected
curl ... -d '{"collectionName":""}'
# {"code":1802,"message":"missing required parameters... 'CollectionName' failed on the 'required' tag"}

# collections/drop — properly rejected
curl ... -d '{"collectionName":""}'
# {"code":1802,...}

# BUT aliases/list — silently accepted with code=0:
curl ... -d '{"collectionName":""}'
# {"code":0,"data":[]}
```

### Expected Behavior

Milvus should reject requests with an empty `collectionName` in the aliases/list endpoint, returning an error similar to:

```json
{
  "code": 65535,
  "message": "invalid parameter: collectionName must be a non-empty string"
}
```

This is consistent with how `filter=""` is now rejected (code=100) and how other parameters with documented constraints are validated.

### Steps To Reproduce

1. Start Milvus v2.6.16 standalone (Docker)
2. Create a collection with an alias (for context):
```bash
curl -s -X POST 'http://localhost:19530/v2/vectordb/aliases/create' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{"aliasName":"test_alias","collectionName":"test_coll"}'
```
3. List aliases with empty collection name:
```bash
curl -s -X POST 'http://localhost:19530/v2/vectordb/aliases/list' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{"collectionName":""}'
```
4. Observe `{"code":0,"data":[]}` — no validation error

### Milvus Log

No error or warning is logged when `collectionName=""` is used in aliases/list:
```
[GIN] [/v2/vectordb/aliases/list] [code=200] [latency=1.1ms] [method=POST] [error=]
```

### Anything else?

- This bug was discovered through automated parameter validation testing (VDBFuzz).
- The `collectionName` parameter in aliases/list specifies the collection whose aliases to list. An empty string is not a valid identifier for any collection.
- This is the same category of issue as #49889 (`dbName=""` accepted) and #49844 (`filter` null/missing accepted) — REST API v2 parameter validation gaps.
- Most endpoints that accept `collectionName` (create, describe, drop, load) now correctly reject empty strings with code=1802 as of v2.6.16. The `aliases/list` endpoint appears to have been missed in this validation pass.
- **Verified on live v2.6.16 standalone (Docker) on 2026-05-23**: aliases/list returns code=0 for collectionName=""; all other tested endpoints return code=1802.