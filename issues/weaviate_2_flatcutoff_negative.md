### How to reproduce this bug?

1. Start Weaviate v1.37.4 with `AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED=true` and `DEFAULT_VECTORIZER_MODULE=none`
2. Create a collection with a negative `flatSearchCutoff`:

```python
import requests
BASE = "http://localhost:8080"
r = requests.post(f"{BASE}/v1/schema", json={
    "class": "TestFC", "vectorizer": "none",
    "vectorIndexConfig": {
        "distance": "cosine",
        "flatSearchCutoff": -100
    },
    "properties": [{"name": "text", "dataType": ["text"]}]
})
print(r.status_code)  # 200
```

3. Verify the invalid value was stored:

```python
r2 = requests.get(f"{BASE}/v1/schema/TestFC")
cutoff = r2.json().get("vectorIndexConfig", {}).get("flatSearchCutoff")
print(f"flatSearchCutoff={cutoff}")  # -100 (negative!)
```

### What is the expected behavior?

The server should return HTTP 422 with an error message like "flatSearchCutoff must be >= 0". A negative flat search cutoff value has no meaningful interpretation — it cannot represent a valid threshold for switching between flat and approximate search.

### What is the actual behavior?

The server returns HTTP 200 and stores the negative `flatSearchCutoff` value of -100 in the collection configuration. No validation error is returned.

### Supporting information

Reproduced on a fresh Weaviate v1.37.4 Docker container with default configuration. All 3 independent runs produced identical results.

### Server Version

v1.37.4

### Weaviate Setup

Single Node

### Nodes count

1
