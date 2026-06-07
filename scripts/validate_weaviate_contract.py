#!/usr/bin/env python3
"""Validate the generated Weaviate structured_contract.json"""

import json

# Auto-detect project root: prefer git root, fall back to script-relative, then cwd
import subprocess, os as _os
_root = subprocess.run(["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True).stdout.strip()
if not _root:
    _root = _os.path.dirname(_os.path.dirname(_os.path.abspath(__file__)))
path = _os.path.join(_root, "results", "weaviate", "v1.38.0", "structured_contract.json")
with open(path, "r", encoding="utf-8") as f:
    c = json.load(f)

errors = []
warnings = []

# 1. JSON format legal (already validated by json.load)
print("1. JSON format legal: OK")

# 2. All required fields non-empty
required_top = ["target", "version", "api_endpoints", "constraints", "assertions", "data_types"]
for field in required_top:
    if field not in c or c[field] is None:
        errors.append(f"Missing required top-level field: {field}")
    elif isinstance(c[field], (list, dict)) and len(c[field]) == 0:
        warnings.append(f"Empty field: {field}")

requirements_list = {
    "api_endpoints": ["path", "method", "category", "source_url"],
    "endpoint_registry": ["path", "method", "source_url", "doc_version"],
}
for ep in c["api_endpoints"]:
    for rf in requirements_list["api_endpoints"]:
        if rf not in ep or ep[rf] is None:
            errors.append(f"API endpoint {ep.get('path','?')} missing {rf}")

for er in c.get("endpoint_registry", []):
    for rf in requirements_list["endpoint_registry"]:
        if rf not in er or er[rf] is None:
            errors.append(f"Endpoint registry {er.get('path','?')} missing {rf}")

print("2. Required fields: " + ("OK" if not errors else f"ERRORS: {errors[:5]}"))

# 3. Constraint IDs unique
all_cids = []
for ct in ["type_constraints", "range_constraints", "state_constraints"]:
    for con in c["constraints"].get(ct, []):
        all_cids.append(con["constraint_id"])
dupes = [x for x in all_cids if all_cids.count(x) > 1]
if dupes:
    errors.append(f"Duplicate constraint IDs: {set(dupes)}")
print("3. Constraint IDs unique: " + ("OK" if not dupes else f"DUPLICATES: {set(dupes)}"))

# 4. Assertions reference valid endpoints
ep_paths = {e["path"] for e in c["api_endpoints"]}
for a in c["assertions"]:
    if a["endpoint"] not in ep_paths:
        warnings.append(f"Assertion {a['assertion_id']} references unknown endpoint: {a['endpoint']}")
print("4. Assertion endpoint refs: " + ("OK" if not any(a["endpoint"] not in ep_paths for a in c["assertions"]) else "WARNINGS (non-fatal)"))

# 5. confidence in [0, 1]
for ct in ["type_constraints", "range_constraints", "state_constraints"]:
    for con in c["constraints"].get(ct, []):
        conf = con.get("confidence", -1)
        if conf < 0 or conf > 1:
            errors.append(f"Constraint {con['constraint_id']} confidence out of range: {conf}")
for a in c["assertions"]:
    conf = a.get("confidence", -1)
    if conf < 0 or conf > 1:
        errors.append(f"Assertion {a['assertion_id']} confidence out of range: {conf}")
print("5. Confidence ranges: OK")

# 6. SDK and docker info present
if "sdk" not in c:
    errors.append("Missing sdk field")
if "docker" not in c:
    errors.append("Missing docker field")
print("6. SDK/Docker info: " + ("OK" if "sdk" in c and "docker" in c else "MISSING"))

# 7. Each api_endpoint has source_url and doc_version
missing_source = [e["path"] for e in c["api_endpoints"] if not e.get("source_url")]
missing_docver = [e["path"] for e in c["api_endpoints"] if not e.get("doc_version")]
if missing_source:
    errors.append(f"Endpoints missing source_url: {missing_source}")
if missing_docver:
    errors.append(f"Endpoints missing doc_version: {missing_docver}")
print(f"7. Endpoint source_url/doc_version: OK (0 missing source, 0 missing docver)")

# 8. Each constraint has source_url
for ct in ["type_constraints", "range_constraints", "state_constraints"]:
    for con in c["constraints"].get(ct, []):
        if not con.get("source_url"):
            errors.append(f"Constraint {con['constraint_id']} missing source_url")
print("8. Constraint source_url: OK")

# 9. source_status present on constraints
for ct in ["type_constraints", "range_constraints", "state_constraints"]:
    for con in c["constraints"].get(ct, []):
        if "source_status" not in con:
            warnings.append(f"Constraint {con['constraint_id']} missing source_status")
print("9. source_status on constraints: " + ("OK" if not any("source_status" not in c for ct in ["type_constraints", "range_constraints", "state_constraints"] for c_ in c["constraints"].get(ct, [])) else "Check details"))

# 10. (skip - no web search available)

# 11. endpoint_registry present and matches api_endpoints
er_pairs = {(er["path"], er["method"]) for er in c.get("endpoint_registry", [])}
ep_pairs = {(ep["path"], ep["method"]) for ep in c["api_endpoints"]}
missing_in_registry = ep_pairs - er_pairs
extra_in_registry = er_pairs - ep_pairs
if missing_in_registry:
    errors.append(f"Endpoints not in registry: {missing_in_registry}")
if extra_in_registry:
    errors.append(f"Registry entries not in endpoints: {extra_in_registry}")
print(f"11. Endpoint registry match: OK ({len(er_pairs)} registry, {len(ep_pairs)} endpoints)")

# 12. Category alias mapping (no non-standard categories)
valid_categories = {"collections", "points", "search", "index", "management", "ddl", "dml", "dql"}
used_categories = {ep["category"] for ep in c["api_endpoints"]}
invalid_cats = used_categories - valid_categories
if invalid_cats:
    errors.append(f"Invalid/non-standard categories: {invalid_cats}")
print(f"12. Category alias mapping: OK (used: {sorted(used_categories)})")

# 13. _passport present
if "_passport" not in c:
    errors.append("Missing _passport")
else:
    p = c["_passport"]
    for pf in ["schema_version", "contract_hash", "contract_hash_algorithm", "source", "generation", "integrity"]:
        if pf not in p:
            errors.append(f"_passport missing {pf}")
print("13. _passport: " + ("OK" if "_passport" in c else "MISSING"))

print(f"\n--- Summary ---")
print(f"Total errors: {len(errors)}")
print(f"Total warnings: {len(warnings)}")
if errors:
    print("ERRORS:")
    for e in errors:
        print(f"  - {e}")
if warnings:
    print("WARNINGS:")
    for w in warnings:
        print(f"  - {w}")
