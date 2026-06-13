"""Quick validation of threat_model.json"""
import json
import sys

with open(r"C:\Users\11428\Desktop\mftui\TestVDB\intelligence\weaviate\threat_model.json") as f:
    tm = json.load(f)

errors = []

meta = tm.get("_meta", {})
if not meta.get("target"):
    errors.append("Missing _meta.target")
if not meta.get("version"):
    errors.append("Missing _meta.version")

for area_key in ["high_priority_areas", "medium_priority_areas", "low_priority_areas"]:
    for area in tm.get("attack_surface", {}).get(area_key, []):
        if "blindspots" not in area or not area["blindspots"]:
            errors.append(f"Area '{area.get('area')}' missing blindspots field")

for bd in tm.get("defect_criteria", {}).get("by_design_behaviors", []):
    for field in ["pattern", "specific_example", "source_issue_numbers", "affected_endpoints", "verdict"]:
        if field not in bd:
            errors.append(f"by_design behavior missing field '{field}'")

blindspots = tm.get("cognitive_blindspots", {}).get("blindspots", [])
if len(blindspots) < 3:
    errors.append(f"Only {len(blindspots)} blindspots (need at least 3)")

endpoints = tm.get("attack_priority_map", {}).get("endpoints", [])
if len(endpoints) < 1:
    errors.append("No endpoints in attack_priority_map")

je = tm.get("judge_enhancements", {})
if not je.get("severity_calibration"):
    errors.append("Missing judge_enhancements.severity_calibration")
if not je.get("submission_success_probability"):
    errors.append("Missing judge_enhancements.submission_success_probability")

if errors:
    print(f"VALIDATION FAILED: {len(errors)} errors")
    for e in errors:
        print(f"  - {e}")
    sys.exit(1)
else:
    print(f"VALIDATION PASSED: {len(blindspots)} blindspots, {len(endpoints)} endpoints, {len(tm['defect_criteria']['by_design_behaviors'])} by_design rules")
