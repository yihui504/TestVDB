# Bug: Duplicate collection creation returns success instead of conflict error

## Summary

Creating a collection with a name that already exists returns `code: 0` (success) instead of a conflict error. This violates idempotency expectations and the documented behavior that duplicate collection names should produce an explicit error.

## Steps to Reproduce

```bash
# 1. Create a collection
curl -X POST 'http://localhost:19530/v2/vectordb/collections/create' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{
    "collectionName": "test_dup",
    "schema": {
      "autoID": false,
      "enableDynamicField": true,
      "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": true},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
      ]
    },
    "indexParams": [
      {"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}
    ]
  }'

# 2. Create the same collection again
curl -X POST 'http://localhost:19530/v2/vectordb/collections/create' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{
    "collectionName": "test_dup",
    "schema": {
      "autoID": false,
      "enableDynamicField": true,
      "fields": [
        {"fieldName": "id", "dataType": "Int64", "isPrimary": true},
        {"fieldName": "vector", "dataType": "FloatVector", "elementTypeParams": {"dim": 4}}
      ]
    },
    "indexParams": [
      {"fieldName": "vector", "metricType": "COSINE", "indexType": "AUTOINDEX"}
    ]
  }'
```

## Expected Behavior

HTTP 200 with a non-zero code indicating conflict:

```json
{"code": 1100, "message": "collection already exists: test_dup"}
```

## Actual Behavior

HTTP 200 with `code: 0` (success) on both the first and second creation:

```json
{"code": 0}
```

## Impact

- **Idempotency violation**: RESTful convention dictates that creating a duplicate resource should return 409 Conflict or an error code
- **Silent data risk**: If the second request had a different schema, the existing collection is not modified but the user believes the new schema was applied
- **Documentation violation**: The [Milvus documentation](https://milvus.io/api-reference/restful/v2.4.x/v2/Vector%20DB/v2/Create%20Collection.md) states that duplicate collectionName should return an explicit conflict error
- **Automation hazard**: CI/CD pipelines and scripts cannot detect whether a collection was actually created or already existed

## Environment

- Milvus version: v2.6.16
- API: REST v2
- Deployment: Docker

## Verification

| Version | Result | Status |
|---------|--------|--------|
| v2.4.4 | DEFECT (code=0) | Present |
| v2.6.16 | DEFECT (code=0) | Still present |

## Suggested Fix

Check for existing collection name before creation. If the collection already exists, return a non-zero code (e.g., `code: 1100`) with a clear message like `"collection already exists: <name>"`. Alternatively, return HTTP 409 Conflict.

## Official Issue Tracking

- **Official Issue**: [#49824](https://github.com/milvus-io/milvus/issues/49824) — **closed** (state_reason: "completed"), by author yihui504
- **Labels**: 无标签（关闭时未添加 kind/bug 或 triage/accepted）
- **Assignees**: 无
- **Milestone**: 无
- **Comments**: 3条
- **Documentation Comparison**: 官方文档 [Create Collection](https://milvus.io/api-reference/restful/v2.4.x/v2/Collection%20(v2)/Create.md) 在快速设置模式下标注 `collectionName (string, required)`。文档未明确说明重复创建的行为，但根据 Milvus 实际行为：参数一致时静默跳过，参数不一致时返回错误码 65535 (`"create duplicate collection with different parameters"`)。
- **Developer Attitude**: issue 由提交者自行关闭（closed by author），未获得官方 triage/accepted 标签，无开发者认领。**关闭原因可能是**：开发者认为这是"幂等创建"的设计决策而非 bug — 即重复创建同名 collection（参数一致）返回成功是一种有意为之的容错行为，类似 HTTP PUT 的幂等语义。提交后**预计接受率: 低 (20-30%)**。若要重新提交，建议强调"不同参数的重复创建也返回成功"这一更危险的行为，而非仅关注幂等性问题。

## Related

- [milvus-io/milvus#49824](https://github.com/milvus-io/milvus/issues/49824) - REST API v2 returns success when creating duplicate collection (closed by author)
- [milvus-io/milvus#40955](https://github.com/milvus-io/milvus/issues/40955) - Duplicate collection with different parameters behavior
