# [Schema Validation] Negative `ef` value (-1) accepted in vectorIndexConfig without validation error

## How to reproduce this bug?

1. Start Weaviate v1.37.4 with anonymous access:
   ```bash
   docker run -p 8080:8080 -e AUTHENTICATION_ANONYMOUS_ACCESS_ENABLED=true -e DEFAULT_VECTORIZER_MODULE=none semitechnologies/weaviate:1.37.4
   ```

2. Create a collection with `ef=-1` in vectorIndexConfig:
   ```bash
   curl -X POST http://localhost:8080/v1/schema \
     -H 'Content-Type: application/json' \
     -d '{
       "class": "TestEfneg",
       "vectorizer": "none",
       "vectorIndexConfig": {
         "distance": "cosine",
         "ef": -1
       },
       "properties": [{"name": "text", "dataType": ["text"]}]
     }'
   ```

3. Observe that the request succeeds with HTTP 200 and the collection is created.

## What is the expected behavior?

The server should reject `ef=-1` with a 422 Unprocessable Entity response and a clear validation error message. The `ef` parameter in HNSW controls the size of the dynamic list for the nearest neighbors during search — it must be a positive integer. Negative values have no valid semantic interpretation in the HNSW algorithm and should be caught during schema validation.

## What is the actual behavior?

The server accepts `ef=-1` without error, creating a collection with an invalid configuration. This could lead to undefined behavior during search operations.

## Supporting information

Similar validation gaps may exist for other HNSW parameters:
- `maxConnections=0` is also accepted (though `0` is at least non-negative)
- `bq.rescoreLimit=-1` is silently discarded from config without error

## Server Version

1.37.4

## Weaviate Setup

Single node (Docker)

## Nodes count

1

## Code of Conduct

I have read and agree to the Weaviate's Contributor Covenant Code of Conduct.
