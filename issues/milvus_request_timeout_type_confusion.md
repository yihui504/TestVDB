# Bug: `Request-Timeout` header accepts non-integer types (float, string)

## Summary

The Milvus v2 REST API accepts `Request-Timeout` header values that are not integers, violating the documented type constraint. Both float values (e.g., `3.5`) and string values (e.g., `"abc"`) are accepted without validation, returning `code: 0` (success) instead of a type error.

The `Request-Timeout` header is documented as `integer` type in the [v2.3.x API reference](https://milvus.io/api-reference/restful/v2.3.x/v2/Vector%20(v2)/Query.md) and is used in official cURL examples across the Milvus documentation (e.g., `"Request-Timeout: 10"`).

## Steps to Reproduce

```bash
# 1. Float value for Request-Timeout (should be integer)
curl -s -X POST 'http://localhost:19530/v2/vectordb/collections/list' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -H 'Request-Timeout: 3.5' \
  -d '{}'

# 2. String value for Request-Timeout (should be integer)
curl -s -X POST 'http://localhost:19530/v2/vectordb/collections/list' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -H 'Request-Timeout: abc' \
  -d '{}'

# 3. For comparison: valid integer value (works correctly)
curl -s -X POST 'http://localhost:19530/v2/vectordb/collections/list' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -H 'Request-Timeout: 10' \
  -d '{}'
```

Both case 1 and case 2 return `{"code": 0, ...}` — success with no validation error.

## Expected Behavior

Return a non-zero code with an error message indicating the type constraint, consistent with how Milvus validates other parameter types (e.g., `limit=0` returns code=65535):

```json
{"code": 65535, "message": "invalid parameter: Request-Timeout must be an integer, got 3.5"}
```

## Actual Behavior

HTTP 200 with `code: 0` and results returned normally. The invalid timeout value is silently ignored — the request proceeds with the default timeout behavior:

```json
{"code": 0, "data": ["..."]}
```

**Behavior details**:
- `Request-Timeout: 3.5` (float): The fractional part is silently discarded. The timeout is effectively set to 3 seconds, not 3.5. This silent truncation can cause the timeout to be shorter than intended.
- `Request-Timeout: abc` (non-numeric string): The value is completely ignored. The request uses the default timeout (no limit), meaning a timeout setting that the user explicitly configured is silently not applied.

## Impact

- **Type safety violation**: The [v2.3.x API reference](https://milvus.io/api-reference/restful/v2.3.x/v2/Vector%20(v2)/Query.md) specifies `Request-Timeout` as `integer` type. Accepting non-integer values violates this contract.
- **Silent truncation of float values**: Float values like `3.5` are silently truncated to `3`, causing the timeout to be shorter than the user intended. In production, this could cause premature timeouts on slow queries.
- **Silent ignoring of invalid values**: String values like `"abc"` are silently ignored, meaning the user's explicit timeout configuration is not applied. This could lead to hung requests that the user expected to time out.
- **Inconsistency with other parameter validation**: Milvus already validates `limit=0` (returns `"topk [0] is invalid, it should be in range [1, 16384]"`), demonstrating that the framework has parameter validation infrastructure. The `Request-Timeout` header lacks equivalent validation.

## Environment

- Milvus version: v2.6.16
- API: REST v2
- Deployment: Docker (standalone)

## Verification

| Variant | v2.4.4 | v2.6.16 | Status |
|---------|--------|---------|--------|
| `Request-Timeout: 3.5` (float) | DEFECT (code=0) | DEFECT (code=0) | Still present |
| `Request-Timeout: abc` (string) | DEFECT (code=0) | DEFECT (code=0) | Still present |

## Suggested Fix

Add server-side type validation for the `Request-Timeout` header. Parse the header value as an integer and reject non-integer values with a non-zero code and a clear error message indicating the expected type. This is consistent with the existing parameter validation pattern used for `limit`/`topk`.

## Official Issue Tracking

- **Official Issue**: [#49890](https://github.com/milvus-io/milvus/issues/49890) — **open**, submitted 2026-05-18
- **Related Issues**: [#49823](https://github.com/milvus-io/milvus/issues/49823) (nprobe=0 accepted), [#49844](https://github.com/milvus-io/milvus/issues/49844) (filter null/missing accepted) — same category of REST API v2 input validation gaps
- **Documentation Comparison**:
  - The [v2.3.x API reference](https://milvus.io/api-reference/restful/v2.3.x/v2/Vector%20(v2)/Query.md) explicitly lists `Request-Timeout` as `integer` type header, with example value `0` and description "The timeout duration for this operation."
  - The v2.4.x and [v2.6.x](https://milvus.io/api-reference/restful/v2.6.x/v2/Collection%20(v2)/List.md) API reference pages removed the `Request-Timeout` header from the parameter list, but the header is still functional and accepted by the server.
  - This documentation inconsistency (header works but is not documented in v2.4.x) should also be addressed.
- **Developer Attitude**: Not yet reported. Similar parameter validation issues (#49823 nprobe=0, #49844 filter null) have both been `triage/accepted` and assigned to MrPresent-Han, demonstrating active acceptance of REST API v2 validation fixes. **Estimated acceptance rate: medium-high (60-70%)**. Risk factors:
  1. Developers may consider HTTP header type validation to be the responsibility of the HTTP framework layer rather than the business logic layer
  2. The v2.4.x documentation no longer lists `Request-Timeout`, which weakens the "documentation violation" argument
  3. However, the header is still functionally supported and used in official examples, so type validation remains a valid concern
- **Additional Evidence**: Milvus already validates `limit=0` with the error `"topk [0] is invalid, it should be in range [1, 16384]"`, proving the framework has parameter validation infrastructure. Extending this to header type validation is a natural progression.

## Related

- [milvus-io/milvus#49823](https://github.com/milvus-io/milvus/issues/49823) - REST API v2 accepts nprobe=0 (open, triage/accepted)
- [milvus-io/milvus#49844](https://github.com/milvus-io/milvus/issues/49844) - REST API v2 query accepts null/missing filter (open, triage/accepted)
