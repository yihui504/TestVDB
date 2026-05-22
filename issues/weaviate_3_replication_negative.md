### How to reproduce this bug?

1. Start Weaviate v1.37.4 with `AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED=true` and `DEFAULT_VECTORIZER_MODULE=none`
2. Create a collection with a negative `replicationFactor`:

```python
import requests, time
BASE = "http://localhost:8080"
r = requests.post(f"{BASE}/v1/schema", json={
    "class": "TestRep", "vectorizer": "none",
    "replicationConfig": {"factor": -1},
    "vectorIndexConfig": {"distance": "cosine"},
    "properties": [{"name": "text", "dataType": ["text"]}]
})
print(r.status_code)  # 200 — accepted!
```

3. Retrieve the collection config to see what was actually stored:

```python
time.sleep(0.5)
r2 = requests.get(f"{BASE}/v1/schema/TestRep")
factor = r2.json().get("replicationConfig", {}).get("factor")
print(f"Stored replicationFactor: {factor}")  # 1 (silently changed from -1!)
```

### What is the expected behavior?

The server should return HTTP 422 with an error message like "replicationFactor must be >= 1". A negative replication factor is semantically invalid and should be rejected.

### What is the actual behavior?

The server returns HTTP 200 and silently normalizes the replication factor from -1 to 1. The user receives no indication that their requested configuration was not applied. An HTTP 200 response implies the operation succeeded exactly as requested, which is false in this case.

### Supporting information

Reproduced on a fresh Weaviate v1.37.4 Docker container with default configuration. All 3 independent runs produced identical results.

### Server Version

v1.37.4

### Weaviate Setup

Single Node

### Nodes count

1
