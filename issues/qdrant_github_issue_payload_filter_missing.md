Payload filter returns points with missing payload field (payload=None)

<!--- Provide a general summary of the issue in the Title above -->

## Current Behavior
<!--- Tell us what happens instead of the expected behavior -->

When performing a search with a payload filter (e.g., `{"must":[{"key":"color","match":{"value":"red"}}]}`), the response includes points where the `payload` field is `None` or the filtered key is entirely absent. These points do not satisfy the filter condition, yet they are returned in the search results alongside points that genuinely match.

Observed output: `filter color=red returned point with color=None`

## Steps to Reproduce
<!--- Provide a link to a live example, or an unambiguous set of steps to -->
<!--- reproduce this bug. Include code to reproduce, if relevant -->

1. Start Qdrant v1.18.1:

```
docker run -d --name qdrant-payload-bug -p 6333:6333 -p 6334:6334 qdrant/qdrant:v1.18.1
```

2. Create a collection:

```
curl -X PUT 'http://localhost:6333/collections/test_payload_filter' -H 'Content-Type: application/json' -d '{"vectors": {"size": 4, "distance": "Cosine"}}'
```

3. Insert 10 points — even IDs have `color=red`, odd IDs have `color=blue`, and point id=3 and id=6 have no `color` payload at all:

```
curl -X PUT 'http://localhost:6333/collections/test_payload_filter/points?wait=true' -H 'Content-Type: application/json' -d '{"points": [{"id": 1, "vector": [0.1, 0.2, 0.3, 0.4], "payload": {"color": "red"}}, {"id": 2, "vector": [0.5, 0.6, 0.7, 0.8], "payload": {"color": "blue"}}, {"id": 3, "vector": [0.9, 0.1, 0.2, 0.3]}, {"id": 4, "vector": [0.2, 0.3, 0.4, 0.5], "payload": {"color": "red"}}, {"id": 5, "vector": [0.6, 0.7, 0.8, 0.9], "payload": {"color": "blue"}}, {"id": 6, "vector": [0.3, 0.4, 0.5, 0.6]}, {"id": 7, "vector": [0.7, 0.8, 0.9, 0.1], "payload": {"color": "red"}}, {"id": 8, "vector": [0.4, 0.5, 0.6, 0.7], "payload": {"color": "blue"}}, {"id": 9, "vector": [0.8, 0.9, 0.1, 0.2], "payload": {"color": "red"}}, {"id": 10, "vector": [0.5, 0.6, 0.7, 0.8], "payload": {"color": "blue"}}]}'
```

4. Search with a payload filter that should only match points with `color=red`:

```
curl -X POST 'http://localhost:6333/collections/test_payload_filter/points/search' -H 'Content-Type: application/json' -d '{"vector": [0.1, 0.2, 0.3, 0.4], "limit": 10, "with_payload": true, "filter": {"must": [{"key": "color", "match": {"value": "red"}}]}}'
```

5. Observe that points with `payload: None` (id=3, id=6) appear in the response alongside points that genuinely have `color=red`.

<!--- Please make sure to include the data which could be used to reproduce the problem -->


## Expected Behavior
<!--- Tell us what should happen -->

All returned points must have `payload.color` equal to `"red"`. Points where the `color` key is missing from the payload should be excluded from the results, since they do not satisfy the `must` filter condition.

A `match` condition on a payload key should require the key to exist and its value to match. A missing key cannot satisfy a `match` condition.

## Possible Solution
<!--- Not obligatory, but suggest a fix/reason for the bug, -->

In the payload filtering logic (likely in `lib/segment/src/payload_storage/query_checker.rs` or the condition checker), when evaluating a `match` condition on a payload key, the implementation should explicitly check whether the key exists in the point's payload. If the key is absent, the point should be excluded from the match (return `false`), rather than being treated as a match or falling through to a default that includes it.

A missing key should not satisfy any `match` condition — it should be treated the same as a key whose value does not match.

## Context (Environment)
<!--- How has this issue affected you? What are you trying to accomplish? -->
<!--- Providing context helps us come up with a solution that is most useful in the real world -->

- **Qdrant version**: v1.18.1 (official Docker image `qdrant/qdrant:v1.18.1`)
- **Deployment**: Docker (single node, default configuration)
- **API**: REST
- **Reproduction**: Confirmed in 3 independent fresh Docker sandbox environments, plus a variant test with a different filter value
- **Discovery**: Found via automated contract-driven fuzzing (TestVDB framework)

## Detailed Description
<!--- Provide a detailed description of the change or addition you are proposing -->

This is an **INCORRECT_RESULT** bug. The search API returns HTTP 200 with results that violate the fundamental semantics of payload filtering. The `must` filter with a `match` condition on a payload key should act as a positive filter — only points that have the key and whose value matches should be returned. Instead, points with missing payload keys are incorrectly included.

**Impact**:
- Users receive data that does not match their filter criteria
- Filter-based access control or business logic may be compromised
- No error is raised; the results silently contain wrong data
- The `must` filter contract is violated — points that do not satisfy the condition are returned

## Possible Implementation
<!--- Not obligatory, but suggest an idea for implementing addition or change -->

In `lib/segment/src/payload_storage/query_checker.rs`, the condition evaluation path for payload fields that do not exist on a point should return `false` for `match` conditions rather than defaulting to `true` or allowing the point to pass through unfiltered. This is consistent with how `values_count` and `is_empty` checks handle missing keys — a missing key should not satisfy a positive match.

A regression test covering mixed payload scenarios (some points with the key, some without, some with different values) should be added to the existing test suite in `lib/segment/tests/`.