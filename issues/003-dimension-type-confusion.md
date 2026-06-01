# WITHDRAWN: Dimension Parameter Accepts Non-Integer Values (float/string)

## ⚠️ VERIFICATION RESULT: ALREADY FIXED IN v2.6.16

**Original Severity**: P1 (Data Integrity / Input Validation)  
**Actual Status**: **Milvus v2.6.16 already rejects non-integer dimension values**

## Verification Evidence (2026-05-26)

### Case 1: Float dimension (3.5)

```python
r = requests.post(f"{BASE}/v2/vectordb/collections/create", headers=H,
                  json={"collectionName": "test", "dimension": 3.5})
# Response: {"code": 1801, "message": "can only accept json format request, error: json: cannot unmarshal number 3.5 into Go struct field CollectionReq.dimension of type int32"}
# VERDICT: CORRECTLY REJECTED
```

### Case 2: String dimension ("abc")

```python
r = requests.post(f"{BASE}/v2/vectordb/collections/create", headers=H,
                  json={"collectionName": "test", "dimension": "abc"})
# Response: {"code": 1801, "message": "can only accept json format request, error: json: cannot unmarshal string into Go struct field CollectionReq.dimension of type int32"}
# VERDICT: CORRECTLY REJECTED
```

### Case 3: dimension=0

```python
r = requests.post(f"{BASE}/v2/vectordb/collections/create", headers=H,
                  json={"collectionName": "test", "dimension": 0})
# Response: {"code": 1100, "message": "dimension is required for quickly create collection..."}
# VERDICT: CORRECTLY REJECTED
```

### Case 4: dimension=-1

```python
r = requests.post(f"{BASE}/v2/vectordb/collections/create", headers=H,
                  json={"collectionName": "test", "dimension": -1})
# Response: {"code": 65535, "message": "invalid dimension: -1. should be in range 2 ~ 32768"}
# VERDICT: CORRECTLY REJECTED
```

## Why This Was a False Positive

The TestVDB tool's Mine run reported `dimension=3.5` and `dimension="abc"` as accepted, but this was likely due to:
1. The tool generating the test script incorrectly (e.g., passing dimension as a string in the URL rather than in JSON body)
2. Or the tool's defect detection logic misinterpreting the response

The actual Milvus v2.6.16 REST API correctly validates dimension type via Go's JSON unmarshaling (`CollectionReq.dimension` is `int32`), which naturally rejects float and string values.

## Remaining Valid Issues

- `dim=32768` (above reasonable max) is still accepted — see Issue 007
- Other parameter validation gaps (efconstruction, ttlSeconds, etc.) — see Issue 005
