# GitHub Issue Draft: Negative TTL Silently Dropped on Collection Create

---

### Is there an existing issue for this?

- [x] I have searched the existing issues

### Environment

```markdown
- Milvus version: v2.6.16
- Deployment mode(standalone or cluster): standalone
- MQ type(rocksmq, pulsar or kafka): rocksmq
- SDK version(e.g. pymilvus v2.0.0rc2): REST API v2 (via curl)
- OS(Ubuntu or CentOS): Docker on Windows
- CPU/Memory: N/A
- GPU: N/A
- Others: Reproduced via Milvus REST API v2 (`/v2/vectordb/collections/create` and `/v2/vectordb/collections/alter_properties`)
```

### Current Behavior

When creating a collection with a negative `collection.ttl.seconds` property via the REST API v2, Milvus returns `code=0` (success) but **silently drops the TTL property** — the created collection has no TTL set at all.

This is inconsistent with the `alter_properties` endpoint, which correctly rejects negative TTL values with a clear error message:

```
{"code":1100,"message":"collection TTL is out of range, expect [-1, 3155760000], got -100: invalid parameter"}
```

**Create with negative TTL (BUG — returns success but TTL is silently dropped):**
```json
// Request: POST /v2/vectordb/collections/create
{"collectionName":"test_ttl_neg","dimension":4,"metricType":"L2","properties":{"collection.ttl.seconds":-100}}

// Response:
{"code":0,"data":{}}

// But describe shows NO TTL property:
// "properties": [{"key": "timezone", "value": "UTC"}]
// Missing: "collection.ttl.seconds"
```

**Alter with negative TTL (correctly rejects):**
```json
// Request: POST /v2/vectordb/collections/alter_properties
{"collectionName":"test_ttl_alter","properties":{"collection.ttl.seconds":-100}}

// Response:
{"code":1100,"message":"collection TTL is out of range, expect [-1, 3155760000], got -100: invalid parameter"}
```

### Expected Behavior

The `collections/create` endpoint should validate the `collection.ttl.seconds` property the same way `alter_properties` does — rejecting values outside the range `[-1, 3155760000]` with a clear error message, rather than silently dropping the property.

Expected response when creating with `collection.ttl.seconds=-100`:
```json
{"code":1100,"message":"collection TTL is out of range, expect [-1, 3155760000], got -100: invalid parameter"}
```

This is consistent with the fix in PR #46717 which added TTL validation to `alterCollectionTask.PreExecute`. The same validation should apply to `createCollectionTask.PreExecute`.

### Steps To Reproduce

1. Start Milvus v2.6.16 standalone (Docker):
   ```bash
   docker run -d --name milvus-standalone \
     -p 19530:19530 \
     -e ETCD_ENDPOINTS=etcd:2379 \
     -e MINIO_ADDRESS=minio:9000 \
     milvusdb/milvus:v2.6.16 milvus run standalone
   ```

2. Create a collection with negative TTL (BUG: returns success but TTL is dropped):
   ```bash
   curl -X POST http://localhost:19530/v2/vectordb/collections/create \
     -H "Content-Type: application/json" \
     -d '{"collectionName":"test_ttl_neg","dimension":4,"metricType":"L2","properties":{"collection.ttl.seconds":-100}}'
   # Response: {"code":0,"data":{}}  <-- Should return error, not success
   ```

3. Verify the TTL was silently dropped:
   ```bash
   curl -X POST http://localhost:19530/v2/vectordb/collections/describe \
     -H "Content-Type: application/json" \
     -d '{"collectionName":"test_ttl_neg"}'
   # Response shows properties WITHOUT "collection.ttl.seconds"
   # Only has: "properties": [{"key": "timezone", "value": "UTC"}]
   ```

4. For comparison, alter_properties correctly rejects negative TTL:
   ```bash
   curl -X POST http://localhost:19530/v2/vectordb/collections/alter_properties \
     -H "Content-Type: application/json" \
     -d '{"collectionName":"test_ttl_neg","properties":{"collection.ttl.seconds":-100}}'
   # Response: {"code":1100,"message":"collection TTL is out of range, expect [-1, 3155760000], got -100: invalid parameter"}
   ```

5. Cleanup:
   ```bash
   curl -X POST http://localhost:19530/v2/vectordb/collections/drop \
     -H "Content-Type: application/json" \
     -d '{"collectionName":"test_ttl_neg"}'
   ```

### Milvus Log

No error or warning is logged when creating a collection with negative TTL. The request is processed silently with HTTP 200.

### Anything else?

- This bug was discovered through automated parameter validation testing (VDBFuzz oracle).
- **This is a regression of PR #46717** — that PR added TTL validation to `alterCollectionTask.PreExecute` and `createCollectionTask.PreExecute` in the gRPC path, but the REST API v2's `collections/create` handler appears to not apply the same validation before forwarding the request.
- The valid TTL range is `[-1, 3155760000]` where `-1` means TTL is disabled (the default).
- **Impact**: Users who set a negative TTL at collection creation time will receive a success response and may assume the TTL was set, when in fact it was silently ignored. This can lead to unexpected data retention behavior — data that should have expired remains indefinitely.
- The same behavior exists for `ttl.seconds` (without the `collection.` prefix) — it is also silently dropped on create.
- Related: #46716, #46717
