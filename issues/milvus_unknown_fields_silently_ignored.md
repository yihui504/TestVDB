# Bug: Unknown/extra fields in request body are silently ignored

## Summary

The Milvus v2 REST API silently ignores unknown or extra fields in the request body instead of rejecting them. This permissive parsing behavior means typos in parameter names (e.g., `collectonName` instead of `collectionName`) are silently ignored, potentially causing the request to operate on incorrect defaults without any error feedback.

## Steps to Reproduce

```bash
# 1. Valid request with an extra unknown field
curl -X POST 'http://localhost:19530/v2/vectordb/collections/describe' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{"collectionName": "<any_collection>", "extraField": 999}'

# 2. Typo in parameter name (silent no-op)
curl -X POST 'http://localhost:19530/v2/vectordb/collections/describe' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer root:Milvus' \
  -d '{"collectonName": "<any_collection>"}'
```

## Expected Behavior

HTTP 400 with an error message like:

```json
{"code": 1100, "message": "unknown parameter: extraField"}
```

Or at minimum, a warning that unrecognized fields were present.

## Actual Behavior

HTTP 200 with `code: 0` and results returned as if the extra field did not exist. The unknown field is silently dropped.

```json
{"code": 0, "data": {"collectionName": "...", ...}}
```

## Impact

- **Typo blindness**: Misspelled parameter names are silently ignored, making debugging extremely difficult
- **API contract violation**: Strict API contracts should reject unexpected fields to catch client errors early
- **Security risk**: Extra fields could be used for parameter injection or confusion attacks in proxy/gateway scenarios
- **Documentation erosion**: If the API silently ignores unknowns, there is no pressure to keep client code in sync with API changes

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

Implement strict request body parsing that rejects unknown fields. Return a 400-level response listing the unrecognized parameter names. This can be implemented as a strict deserialization mode (e.g., `deny_unknown_fields` in serde) or a middleware validation layer.

## Official Issue Tracking

- **Official Issue**: 无直接对应issue
- **Related Issue**: [#49823](https://github.com/milvus-io/milvus/issues/49823), [#49844](https://github.com/milvus-io/milvus/issues/49844) — 同属 REST API v2 参数验证缺口类别
- **Documentation Comparison**: 官方文档对每个 API 端点都列出了明确的参数列表，未列出的参数不应被接受。静默忽略未知字段违反了 API 契约的严格性原则。然而，Milvus REST API v2 使用 Go 的标准 JSON 解析，默认行为是忽略未知字段（与 Rust serde 的默认行为一致），要启用严格模式需要显式配置 `DisallowUnknownFields()`。
- **Developer Attitude**: 未被单独报告。**预计接受率: 低 (20-30%)**。原因：
  1. **行业惯例**: 大多数 REST API 框架默认忽略未知字段，这是广泛接受的容错行为
  2. **兼容性风险**: 启用严格模式可能破坏现有客户端（客户端发送了服务端新版本才支持的字段时，旧版本会拒绝）
  3. **优先级低**: 相比 nprobe=0（语义错误）和 filter=null（安全风险），未知字段静默忽略的影响较小
  4. **可能的折中方案**: 开发者更可能接受"在响应中添加 warning 字段"而非"拒绝未知字段"
- **Suggested Submission Strategy**: 若提交，建议强调 typo 导致的静默错误（如 `collectonName` 被忽略导致操作了默认 collection），而非强调严格 API 契约。安全角度（参数注入）也可增加说服力。

## Related

- [milvus-io/milvus#49823](https://github.com/milvus-io/milvus/issues/49823) - REST API v2 accepts nprobe=0 (open, triage/accepted)
- [milvus-io/milvus#49844](https://github.com/milvus-io/milvus/issues/49844) - REST API v2 query accepts null/missing filter (open, triage/accepted)
