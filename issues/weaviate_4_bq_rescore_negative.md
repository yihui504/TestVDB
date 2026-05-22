### How to reproduce this bug?

1. Start Weaviate v1.37.4 with `AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED=true` and `DEFAULT_VECTORIZER_MODULE=none`
2. Create a collection with binary quantization enabled and a negative `rescoreLimit`:

```python
import requests, time
BASE = "http://localhost:8080"
r = requests.post(f"{BASE}/v1/schema", json={
    "class": "TestBQ", "vectorizer": "none",
    "vectorIndexConfig": {
        "distance": "cosine",
        "bq": {"enabled": True, "rescoreLimit": -1}
    },
    "properties": [{"name": "text", "dataType": ["text"]}]
})
print(r.status_code)  # 200 — accepted!
```

3. Retrieve the collection config to see what was actually stored:

```python
time.sleep(0.5)
r2 = requests.get(f"{BASE}/v1/schema/TestBQ")
bq_config = r2.json().get("vectorIndexConfig", {}).get("bq", {})
print(f"Stored bq config: {bq_config}")
# Output: {'enabled': True}  — rescoreLimit is gone!
```

### What is the expected behavior?

The server should return HTTP 422 with an error message like "bq.rescoreLimit must be >= 0 when bq is enabled". An invalid rescore limit should be caught early at schema validation time.

### What is the actual behavior?

The server returns HTTP 200 and silently discards the invalid `rescoreLimit` value from the configuration. The binary quantization is enabled but operates without the user-intended rescore limit. The user receives no indication that part of their configuration was invalid or ignored.

### Supporting information

Reproduced on a fresh Weaviate v1.37.4 Docker container with default configuration. All 3 independent runs produced identical results.

### Server Version

v1.37.4

### Weaviate Setup

Single Node

### Nodes count

1
