# Bug: REST API v2 accepts empty string for `dbName` parameter

## Summary

The Milvus v2 REST API accepts empty strings (`""`) for the `dbName` parameter across multiple endpoints. The official documentation defines `dbName` as "The name of an **existing** database" — an empty string is not a valid database name, yet the API returns `code: 0` (success) with no validation error.

This is the same category of bug as #49844 (filter null/missing accepted), which has been `triage/accepted` and assigned to a developer.

> **Note**: The `filter=""` variant (on the query endpoint) has been **fixed** in v2.6.16 (returns `code: 100`). The `dbName=""` variant remains unfixed across all endpoints.

## Steps to Reproduce

```bash
# 1. Empty dbName on collections/list
curl -s -X POST 'http://localhost:19530/v2/vectordb/collections/list' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{"dbName": ""}'

# 2. Empty dbName on collections/describe
curl -s -X POST 'http://localhost:19530/v2/vectordb/collections/describe' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{"collectionName": "<any_collection>", "dbName": ""}'

# 3. Empty dbName on collections/drop
curl -s -X POST 'http://localhost:19530/v2/vectordb/collections/drop' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{"collectionName": "<any_collection>", "dbName": ""}'
```

All three return `{"code": 0, ...}` — success with no validation error.

## Expected Behavior

Return a non-zero code with an error message indicating that `dbName` must be a valid database name, consistent with how `filter=""` is now rejected (code=100) and `limit=0` is rejected (code=65535):

```json
{"code": 65535, "message": "invalid parameter: dbName must be a valid database name, got empty string"}
```

## Actual Behavior

HTTP 200 with `code: 0` and results returned as if the empty string was a valid database name (silently defaults to the default database):

```json
{"code": 0, "data": ["..."]}
```

## Impact

- **Ambiguous database targeting**: Empty `dbName` silently defaults to the default database, which may not be the user's intent. A typo like `dbName: ""` (from a variable that was never set) silently operates on the wrong database.
- **Documentation violation**: The official [List Collections](https://milvus.io/api-reference/restful/v2.4.x/v2/Collection%20(v2)/List.md) documentation defines `dbName` as "The name of an existing database." An empty string is not an existing database name.
- **Inconsistency with filter validation**: `filter=""` is now correctly rejected (code=100 in v2.6.16), but `dbName=""` is still accepted. Both are empty string values for parameters that should require non-empty values.
- **No error feedback**: Users receive no indication that their input is invalid.

## Environment

- Milvus version: v2.6.16
- API: REST v2
- Deployment: Docker (standalone)

## Verification

| Variant | v2.4.4 | v2.6.16 | Status |
|---------|--------|---------|--------|
| `dbName=""` on collections/list | DEFECT (code=0) | DEFECT (code=0) | Still present |
| `dbName=""` on collections/describe | DEFECT (code=0) | DEFECT (code=0) | Still present |
| `filter=""` on entities/query | DEFECT (code=0) | FIXED (code=100) | Fixed in v2.6.16 |

## Suggested Fix

Add server-side validation to reject empty strings for `dbName` parameter, consistent with the fix applied to `filter=""`. Return a non-zero code with a clear error message indicating the parameter must be a non-empty string that matches an existing database name.

## Official Issue Tracking

### dbName="" variant
- **Official Issue**: [#49889](https://github.com/milvus-io/milvus/issues/49889) — **open**, submitted 2026-05-18
- **Related Issue**: [#49844](https://github.com/milvus-io/milvus/issues/49844) (filter null/missing — same category of empty/null string parameter validation)
- **Documentation Comparison**: Official [List Collections v2.4.x](https://milvus.io/api-reference/restful/v2.4.x/v2/Collection%20(v2)/List.md) and [List Collections v2.6.x](https://milvus.io/api-reference/restful/v2.6.x/v2/Collection%20(v2)/List.md) both define `dbName` as `string` (optional), described as "The name of an existing database." An empty string `""` is not a valid database name, so accepting it violates the documented constraint. The same `dbName` parameter appears in [Create Collection](https://milvus.io/api-reference/restful/v2.4.x/v2/Collection%20(v2)/Create.md), [Describe Collection](https://milvus.io/api-reference/restful/v2.4.x/v2/Collection%20(v2)/Describe.md), and other endpoints — all with the same definition.
- **Developer Attitude**: Not yet reported. #49844 has been `triage/accepted` and assigned to MrPresent-Han (milestone 3.0), demonstrating that the development team is actively accepting REST API v2 parameter validation fixes. `dbName=""` is the same category of issue (empty string accepted where a meaningful value is required), so the **estimated acceptance rate is high (70-80%)**.

### filter="" variant
- **Official Issue**: [#49844](https://github.com/milvus-io/milvus/issues/49844) — **open**, `kind/bug` + `triage/accepted`, assigned to MrPresent-Han, milestone 3.0
- **Status**: v2.6.16 has fixed `filter=""` (returns code=100), but `filter=null` and missing `filter` still return all data
- **Documentation Comparison**: Official [Query](https://milvus.io/api-reference/restful/v2.4.x/v2/Vector%20(v2)/Query.md) explicitly marks `filter (string, required)`

## Related

- [milvus-io/milvus#49844](https://github.com/milvus-io/milvus/issues/49844) - REST API v2 query accepts null/missing filter (open, triage/accepted)
- [milvus-io/milvus#49823](https://github.com/milvus-io/milvus/issues/49823) - REST API v2 accepts nprobe=0 (open, triage/accepted)
