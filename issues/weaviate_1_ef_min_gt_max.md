### How to reproduce this bug?

1. Start Weaviate v1.37.4 with `AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED=true` and `DEFAULT_VECTORIZER_MODULE=none`
2. Create a collection with `dynamicEfMin` greater than `dynamicEfMax`:

```python
import requests
BASE = "http://localhost:8080"
r = requests.post(f"{BASE}/v1/schema", json={
    "class": "TestEfBad", "vectorizer": "none",
    "vectorIndexConfig": {
        "distance": "cosine",
        "dynamicEfMin": 500,
        "dynamicEfMax": 10,
        "dynamicEfFactor": 8
    },
    "properties": [{"name": "text", "dataType": ["text"]}]
})
print(r.status_code)  # 200
```

3. Retrieve the collection config to verify:

```python
r2 = requests.get(f"{BASE}/v1/schema/TestEfBad")
config = r2.json().get("vectorIndexConfig", {})
print(f"efMin={config.get('dynamicEfMin')}, efMax={config.get('dynamicEfMax')}")
# Output: efMin=500, efMax=10  (efMin > efMax!)
```

### What is the expected behavior?

The server should return HTTP 422 (Unprocessable Entity) with an error message like "dynamicEfMin must be less than or equal to dynamicEfMax". A configuration where the minimum dynamic ef is larger than the maximum is logically impossible and should be rejected.

This is consistent with how Weaviate already validates other vectorIndexConfig parameters (e.g., `maxConnections=0` correctly returns 422).

### What is the actual behavior?

The server returns HTTP 200 and creates the collection successfully. The illogical configuration (efMin=500 > efMax=10) is stored without any warning or error. The collection appears functional but operates with a broken HNSW dynamic ef range.

### Supporting information

Reproduced on a fresh Weaviate v1.37.4 Docker container with default configuration. All 3 independent runs produced identical results.

### Server Version

v1.37.4

### Weaviate Setup

Single Node

### Nodes count

1
