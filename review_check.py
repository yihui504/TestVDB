import requests, json, time

BASE = 'http://localhost:6335'

# Defect 4 deeper analysis: Is empty vectors config really a defect?
print('=== DEFECT 4 Deep Analysis: Empty vectors config ===')

# Create collection with empty json - what does Qdrant do?
r = requests.put(f'{BASE}/collections/test_empty_deep', json={})
print(f'Create with empty json: status={r.status_code}')
time.sleep(0.5)

# Check what the collection looks like
r = requests.get(f'{BASE}/collections/test_empty_deep')
if r.status_code == 200:
    info = r.json().get('result', {})
    config = info.get('config', {})
    params = config.get('params', {})
    vectors = params.get('vectors', {})
    print(f'  vectors config: {vectors}')
    print(f'  vector_count: {info.get("vectors_count", 0)}')
    print(f'  points_count: {info.get("points_count", 0)}')

# Can we insert points into this collection?
r = requests.put(f'{BASE}/collections/test_empty_deep/points', json={'points': [{'id': 1, 'vector': [0.1, 0.2, 0.3, 0.4]}]})
print(f'  Insert point: status={r.status_code}, body={r.json() if r.status_code != 200 else "OK"}')

# Can we search this collection?
r = requests.post(f'{BASE}/collections/test_empty_deep/points/search', json={'vector': [0.1, 0.2, 0.3, 0.4], 'limit': 3})
print(f'  Search: status={r.status_code}, body={r.json() if r.status_code != 200 else "OK"}')

# Now check: Is this actually a "named vectors" collection?
# Qdrant supports named vectors - empty config might create a collection with no default vector
print('\n=== Named vectors analysis ===')
r = requests.get(f'{BASE}/collections/test_empty_deep')
if r.status_code == 200:
    info = r.json().get('result', {})
    config = info.get('config', {})
    print(f'  Full config: {json.dumps(config, indent=2)[:800]}')

# Check score_threshold behavior more carefully
print('\n=== DEFECT 2/3 Deep Analysis: score_threshold ===')
# score_threshold=2.0 returns 0 results - is this really a defect?
# The server accepts the value but returns empty results, which is arguably correct behavior
# (no score can be > 2.0 for cosine similarity)
# But the contract says it should be rejected at the boundary

# Check what Qdrant documentation says about score_threshold
# score_threshold is a FILTER - it filters results by minimum score
# If score_threshold=2.0, all results are filtered out (no score > 2.0)
# If score_threshold=-0.5, no results are filtered (all scores > -0.5)
# The question is: should the server validate the range, or is it the client's responsibility?

# Let's check if Qdrant's OpenAPI spec has any constraints
r = requests.get(f'{BASE}/collections/mre_defect_collection')
if r.status_code == 200:
    info = r.json().get('result', {})
    print(f'Collection info: {json.dumps(info, indent=2)[:300]}')
