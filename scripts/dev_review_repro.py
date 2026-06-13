#!/usr/bin/env python3
"""Reproduce DEF-004 and DEF-006 for dev review."""
import requests, json, uuid, threading, sys, time

BASE = 'http://localhost:8080/v1'

def repro_def004():
    """Concurrent batch delete + insert"""
    CLASS = 'DevReviewBatchDeleteRepro'
    requests.delete(BASE + '/schema/' + CLASS, timeout=5)

    r = requests.post(BASE + '/schema', json={
        'class': CLASS,
        'vectorizer': 'none',
        'properties': [
            {'name': 'content', 'dataType': ['text']},
            {'name': 'seq', 'dataType': ['int']}
        ]
    }, timeout=10)
    assert r.status_code == 200, f"Schema create: {r.status_code} {r.text}"

    # Insert some base objects
    for i in range(20):
        obj = {'class': CLASS, 'id': str(uuid.uuid4()), 'properties': {'content': f'base-{i}', 'seq': i}}
        r = requests.post(BASE + '/objects', json=obj, timeout=10)
        assert r.status_code == 200, f"Base create {i}: {r.status_code}"

    print(f"[DEF-004] Created 20 base objects")

    errors = []
    lock = threading.Lock()

    def deleter():
        for i in range(3):
            bd = {
                'match': {'class': CLASS, 'where': {'operator': 'GreaterThan', 'path': ['seq'], 'valueInt': 0}},
                'output': 'minimal'
            }
            try:
                r = requests.delete(BASE + '/batch/objects', json=bd, timeout=10)
                if r.status_code != 200:
                    with lock:
                        errors.append(f"DELETE {i}: {r.status_code} {r.text[:150]}")
            except Exception as e:
                with lock:
                    errors.append(f"DELETE {i} EXCEPTION: {e}")

    def inserter():
        for i in range(10):
            obj = {'class': CLASS, 'id': str(uuid.uuid4()), 'properties': {'content': f'ins-{i}', 'seq': i+100}}
            try:
                r = requests.post(BASE + '/objects', json=obj, timeout=10)
                if r.status_code != 200:
                    with lock:
                        errors.append(f"INSERT {i}: {r.status_code} {r.text[:150]}")
            except Exception as e:
                with lock:
                    errors.append(f"INSERT {i} EXCEPTION: {e}")

    threads = []
    for _ in range(3):
        t = threading.Thread(target=deleter)
        threads.append(t)
        t.start()
    for _ in range(3):
        t = threading.Thread(target=inserter)
        threads.append(t)
        t.start()

    for t in threads:
        t.join()

    if errors:
        print(f"[DEF-004] ERRORS ({len(errors)}):")
        for e in errors[:10]:
            print(f"  {e}")
    else:
        print("[DEF-004] No errors")

    requests.delete(BASE + '/schema/' + CLASS, timeout=5)
    print("[DEF-004] Cleanup done")
    return errors

def repro_def006():
    """DELETE then immediate GET visibility (linearizability)"""
    CLASS = 'DevReviewDeleteGetRepro'
    requests.delete(BASE + '/schema/' + CLASS, timeout=5)

    r = requests.post(BASE + '/schema', json={
        'class': CLASS,
        'vectorizer': 'none',
        'properties': [
            {'name': 'content', 'dataType': ['text']}
        ]
    }, timeout=10)
    assert r.status_code == 200, f"Schema: {r.status_code} {r.text}"

    violations = []

    # Create several objects, then test delete/get sequence
    for race_num in range(5):
        obj_id = str(uuid.uuid4())
        obj = {'class': CLASS, 'id': obj_id, 'properties': {'content': f'race-{race_num}'}}
        r = requests.post(BASE + '/objects', json=obj, timeout=10)
        assert r.status_code == 200, f"Create race {race_num}: {r.status_code}"

        # Delete the object
        r = requests.delete(BASE + f'/objects/{CLASS}/{obj_id}', timeout=10)
        if r.status_code != 204:
            print(f"[DEF-006] Race {race_num}: DELETE returned {r.status_code}, expected 204")
            continue

        # Immediately GET the object
        r = requests.get(BASE + f'/objects/{CLASS}/{obj_id}', timeout=10)
        if r.status_code == 200:
            violations.append(f"Race {race_num}: GET returned 200 after DELETE 204")
        elif r.status_code == 404:
            pass  # Expected
        else:
            violations.append(f"Race {race_num}: GET returned {r.status_code} (unexpected)")

    if violations:
        print(f"[DEF-006] VIOLATIONS ({len(violations)}):")
        for v in violations:
            print(f"  {v}")
    else:
        print("[DEF-006] No violations - all GET returned 404 after DELETE")

    requests.delete(BASE + '/schema/' + CLASS, timeout=5)
    print("[DEF-006] Cleanup done")
    return violations

if __name__ == '__main__':
    which = sys.argv[1] if len(sys.argv) > 1 else 'both'

    if which in ('def004', 'both'):
        print("=== REPRO DEF-004 ===")
        repro_def004()

    if which in ('def006', 'both'):
        print("\n=== REPRO DEF-006 ===")
        repro_def006()
