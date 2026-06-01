# WITHDRAWN: Empty String Accepted for Name-type Required Fields

## ⚠️ VERIFICATION RESULT: ALREADY FIXED IN v2.6.16

**Original Severity**: P2 (Input Validation)  
**Actual Status**: **Milvus v2.6.16 already rejects empty strings for name-type fields**

## Verification Evidence (2026-05-26)

All tested endpoints correctly reject empty string name fields with `code=1802`:

| Endpoint | Field | Response |
|----------|-------|----------|
| collections/create | collectionName="" | `{"code": 1802, "message": "missing required parameters...CollectionName...failed on the 'required' tag"}` |
| collections/describe | collectionName="" | `{"code": 1802, "message": "missing required parameters...CollectionName...failed on the 'required' tag"}` |
| collections/drop | collectionName="" | `{"code": 1802, "message": "missing required parameters...CollectionName...failed on the 'required' tag"}` |
| roles/create | roleName="" | `{"code": 1802, "message": "missing required parameters...RoleName...failed on the 'required' tag"}` |
| users/drop | userName="" | `{"code": 1802, "message": "missing required parameters...UserName...failed on the 'required' tag"}` |
| partitions/create | partitionName="" | `{"code": 1802, "message": "missing required parameters...PartitionName...failed on the 'required' tag"}` |
| collections/rename | newCollectionName="" | `{"code": 1802, "message": "missing required parameters...NewCollectionName...failed on the 'required' tag"}` |

### Exception: dbName=""

```python
r = requests.post(f"{BASE}/v2/vectordb/partitions/list", headers=H,
                  json={"collectionName": "nonexistent", "dbName": ""})
# Response: {"code": 100, "message": "collection not found[database=default][collection=nonexistent]"}
# dbName="" is treated as dbName="default" — this is already reported in #49889
```

## Why This Was a False Positive

The TestVDB tool's Mine run reported empty strings as accepted, but this was likely due to:
1. The tool testing empty strings on **composite endpoints** (`search+create_collection`) where the test script may have targeted the wrong endpoint
2. The tool's boundary test generating scripts that pass empty strings in a way that bypasses the validation (e.g., as part of a larger JSON body where the field is optional)

The actual Milvus v2.6.16 REST API uses Go's `binding:"required"` tag validation, which correctly rejects empty strings for required fields.

## Remaining Valid Issue

- `dbName=""` is treated as `dbName="default"` on some endpoints — already reported in #49889
- `aliases/list` accepts empty `collectionName` — already reported in #50018
