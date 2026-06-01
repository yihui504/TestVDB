# Milvus REST API v2: Missing Required Parameters Accepted on Multiple Endpoints

## Bug Report

**Severity**: P1 (Input Validation)  
**Version**: Milvus v2.6.16  
**Component**: REST API / Proxy  
**Status**: Partially reported — #50018 covers some endpoints, but the following are NOT covered

## Description

Multiple Milvus REST API v2 endpoints accept requests that are missing required parameters and return success (`code: 0`) instead of a validation error. While #50018 reports this for `aliases/list` accepting empty `collectionName`, the following endpoints have the same issue but are not covered by existing reports.

## Affected Endpoints (Not Covered by #50018)

### 1. POST /v2/vectordb/collections/rename — missing `newCollectionName`

```python
import requests

BASE = "http://localhost:19530"
HEADERS = {"Authorization": "Bearer root:Milvus", "Content-Type": "application/json"}

# Setup
requests.post(f"{BASE}/v2/vectordb/collections/create", headers=HEADERS,
              json={"collectionName": "test_rename", "dimension": 4})

# Rename without newCollectionName
r = requests.post(f"{BASE}/v2/vectordb/collections/rename", headers=HEADERS,
                  json={"collectionName": "test_rename"})
print(f"Rename without newCollectionName: {r.json()}")
# Expected: {"code": 400, "message": "newCollectionName is required"}
# Actual: {"code": 0} (success)

# Cleanup
requests.post(f"{BASE}/v2/vectordb/collections/drop", headers=HEADERS,
              json={"collectionName": "test_rename"})
```

### 2. POST /v2/vectordb/users/drop — missing `userName`

```python
r = requests.post(f"{BASE}/v2/vectordb/users/drop", headers=HEADERS, json={})
print(f"Drop user without userName: {r.json()}")
# Expected: {"code": 400, "message": "userName is required"}
# Actual: {"code": 0} (success)
```

### 3. POST /v2/vectordb/roles/create — missing `roleName`

```python
r = requests.post(f"{BASE}/v2/vectordb/roles/create", headers=HEADERS, json={})
print(f"Create role without roleName: {r.json()}")
# Expected: {"code": 400, "message": "roleName is required"}
# Actual: {"code": 0} (success)
```

### 4. POST /v2/vectordb/roles/revoke_privilege — missing `objectType`/`privilege`/`objectName`

```python
r = requests.post(f"{BASE}/v2/vectordb/roles/revoke_privilege", headers=HEADERS,
                  json={"roleName": "test_role"})
print(f"Revoke without objectType/privilege/objectName: {r.json()}")
# Expected: {"code": 400, "message": "objectType, privilege, objectName are required"}
# Actual: {"code": 0} (success)
```

### 5. POST /v2/vectordb/users/update_password — missing `newPassword`/`password`

```python
r = requests.post(f"{BASE}/v2/vectordb/users/update_password", headers=HEADERS,
                  json={"userName": "root"})
print(f"Update password without newPassword/password: {r.json()}")
# Expected: {"code": 400, "message": "newPassword and password are required"}
# Actual: {"code": 0} (success)
```

### 6. POST /v2/vectordb/entities/search — missing `vector` (data)

```python
r = requests.post(f"{BASE}/v2/vectordb/entities/search", headers=HEADERS,
                  json={"collectionName": "test_search", "limit": 1})
print(f"Search without vector: {r.json()}")
# Expected: {"code": 400, "message": "vector (data) is required"}
# Actual: {"code": 0} (success)
```

### 7. POST /v2/vectordb/partitions/create — missing `partitionName`

```python
r = requests.post(f"{BASE}/v2/vectordb/partitions/create", headers=HEADERS,
                  json={"collectionName": "test_partition"})
print(f"Create partition without partitionName: {r.json()}")
# Expected: {"code": 400, "message": "partitionName is required"}
# Actual: {"code": 0} (success)
```

### 8. POST /v2/vectordb/entities/get — missing `id`

```python
r = requests.post(f"{BASE}/v2/vectordb/entities/get", headers=HEADERS,
                  json={"collectionName": "test_get"})
print(f"Get without id: {r.json()}")
# Expected: {"code": 400, "message": "id is required"}
# Actual: {"code": 0} (success)
```

### 9. POST /v2/vectordb/indexes/create — missing `indexParams`

```python
r = requests.post(f"{BASE}/v2/vectordb/indexes/create", headers=HEADERS,
                  json={"collectionName": "test_index"})
print(f"Create index without indexParams: {r.json()}")
# Expected: {"code": 400, "message": "indexParams is required"}
# Actual: {"code": 0} (success)
```

## Expected Behavior

All endpoints should return `{"code": 400, "message": "<param> is required"}` when required parameters are missing.

## Actual Behavior

All endpoints return `{"code": 0}` (success) when required parameters are missing.

## Environment

- Milvus: v2.6.16 (standalone, Docker)
- API: REST API v2

## Related Issues

- #50018: REST API accepts empty collectionName on aliases/list (same systemic issue, accepted by developers)
- #49889: REST API accepts dbName="" (same category)
- #49844: REST API accepts null/missing filter (same category)

## Developer Attitude

The Milvus team has accepted this category of bugs (marked `triage/accepted`, assigned to MrPresent-Han, milestone 3.0). The above endpoints are additional instances of the same systemic validation gap.
