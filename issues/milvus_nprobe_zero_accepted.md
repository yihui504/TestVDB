# Bug: Search accepts `nprobe=0` despite being an invalid search parameter

## Summary

The Milvus v2 REST API accepts `nprobe=0` in search parameters, which is semantically invalid. An `nprobe` value of 0 means "probe zero clusters" during ANN search, which cannot produce meaningful results. The API should reject this value and return a validation error.

## Steps to Reproduce

```bash
# 1. Create collection and insert data
curl -X POST 'http://localhost:19530/v2/vectordb/collections/create' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{
    "collectionName": "test_nprobe",
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

# 2. Insert a point
curl -X POST 'http://localhost:19530/v2/vectordb/entities/insert' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{"collectionName": "test_nprobe", "data": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4]}]}'

# 3. Search with nprobe=0
curl -X POST 'http://localhost:19530/v2/vectordb/entities/search' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{"collectionName": "test_nprobe", "data": [[0.1, 0.2, 0.3, 0.4]], "limit": 1, "searchParams": {"params": {"nprobe": 0}}}'
```

## Expected Behavior

HTTP 400 with an error message like:

```json
{"code": 1100, "message": "invalid search parameter: nprobe must be >= 1, got 0"}
```

## Actual Behavior

HTTP 200 with `code: 0` and search results (possibly empty) returned. No validation error.

```json
{"code": 0, "data": []}
```

## Impact

- **Undefined search behavior**: `nprobe=0` produces empty or incorrect search results without any error
- **Silent degradation**: Users receive no indication that the search parameter is invalid
- **Debugging difficulty**: Empty results from `nprobe=0` may be misinterpreted as "no matching data" rather than "invalid parameter"
- **Documentation violation**: The [Milvus search documentation](https://milvus.io/docs/search.md) implies nprobe should be a positive integer

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

Add server-side validation for `nprobe >= 1` before executing search. Return a 400-level response with a clear error message indicating the minimum valid value.

## Official Issue Tracking

- **Official Issue**: [#49823](https://github.com/milvus-io/milvus/issues/49823) — **open**
- **Labels**: `kind/bug`, `triage/accepted`
- **Assignee**: MrPresent-Han
- **Milestone**: 2.6.18
- **Comments**: 1条
- **Created**: 2026-05-15
- **Documentation Comparison**: REST API v2 官方文档 [Search](https://milvus.io/api-reference/restful/v2.4.x/v2/Vector%20(v2)/Search.md) 的 `searchParams.params` 未显式列出 nprobe，但 Milvus 索引文档明确标注 nprobe 取值范围为 `[1, nlist]`，默认值 8。`nprobe=0` 违反此约束。此外，Milvus 已正确验证 `limit=0`（返回 `"topk [0] is invalid, it should be in range [1, 16384]"`），nprobe 缺少同等验证是明显的参数验证缺口。
- **Developer Attitude**: **积极接受**。已获得 `triage/accepted` 标签，分配给核心开发者 MrPresent-Han，纳入 milestone 2.6.18（近期版本）。**预计接受率: 高 (90%+)**，issue 已被正式接受且计划在 2.6.18 修复。

## Related

- [milvus-io/milvus#49823](https://github.com/milvus-io/milvus/issues/49823) - REST API v2 accepts nprobe=0 (open, triage/accepted, milestone 2.6.18)
- [milvus-io/milvus#49844](https://github.com/milvus-io/milvus/issues/49844) - REST API v2 query accepts null/missing filter (open, triage/accepted, milestone 3.0)
