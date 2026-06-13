#!/usr/bin/env python3
"""
Complete Bug Shape Extraction Pipeline for Milvus.
Step 1: Issue classification (positive/negative/invalid)
Step 2: Bug shape extraction from positive samples
Step 3: Developer cognition analysis from negative samples
"""
import json
import os
import shutil
from datetime import datetime, timezone

now = datetime.now(timezone.utc).isoformat()

# Load Data
with open(r'C:\Users\11428\Desktop\mftui\TestVDB\intelligence\milvus\issue_corpus.json', 'r', encoding='utf-8') as f:
    issue_data = json.load(f)
issues = issue_data['issues']

with open(r'C:\Users\11428\Desktop\mftui\TestVDB\intelligence\milvus\commit_corpus.json', 'r', encoding='utf-8') as f:
    pr_data = json.load(f)
prs = pr_data.get('merged_prs', [])

# Build PR linked issues map
pr_linked_issues = {}
for pr in prs:
    for linked in pr.get('linked_issues', []):
        if linked not in pr_linked_issues:
            pr_linked_issues[linked] = []
        pr_linked_issues[linked].append(pr)

# Step 1: Classification
classified = []
stats = {"positive": 0, "negative": 0, "invalid": 0}
pos_by_label = {}
neg_by_reason = {}
inv_by_reason = {}

for issue in issues:
    num = issue["number"]
    title = issue["title"]
    labels = issue.get("labels", [])
    state = issue.get("state", "open")
    comments = issue.get("comments", [])
    has_pr = issue.get("has_associated_pr", False)
    linked_prs_in_issue = issue.get("linked_prs", [])

    # Check maintainer comments for explicit signals
    maintainer_comments = [(idx, c) for idx, c in enumerate(comments)
                           if c.get("role") in ("contributor", "maintainer")]

    ack_signals = []   # (idx, author, phrase)
    deny_signals = []  # (idx, author, phrase, attitude)

    for idx, c in maintainer_comments:
        body = c["body"]
        bl = body.lower()

        # Denial keywords (must be maintainer's own words, not quoting issue template)
        deny_kw = [
            ("by design", "by_design"),
            ("not a bug", "not_a_bug"),
            ("works as intended", "expected_behavior"),
            ("wontfix", "wontfix"),
            ("won't fix", "wontfix"),
            ("invalid issue", "invalid_template"),
            ("invalid template", "invalid_template"),
            ("this is intentional", "by_design"),
            ("not guaranteed", "by_design"),
            ("not our bug", "not_a_bug"),
        ]
        for phrase, attitude in deny_kw:
            if phrase in bl:
                # Verify it's not just quoting the issue template
                pos = bl.find(phrase)
                before = bl[max(0, pos-100):pos]
                if phrase == "by design" and "expected behavior" in before:
                    continue
                deny_signals.append((idx, c["author"], phrase, attitude))
                break

        # Acknowledge keywords
        # "fix" is too generic - only use it in context that clearly means acknowledging
        ack_kw = ["good catch", "will fix", "let me fix", "i'll fix",
                  "let me check", "confirmed", "reproduced",
                  "with this bug fixed", "this bug fixed", "this is fixed",
                  "please help on verifying the fix"]
        for phrase in ack_kw:
            if phrase in bl:
                ack_signals.append((idx, c["author"], phrase))
                break

        # Also: maintainer saying "fix" and referencing a PR number is a strong ack
        if "fix" in bl and "pr:" in bl and "#" in bl:
            ack_signals.append((idx, c["author"], "fix_with_pr_ref"))

        # Strong ack: maintainer says "the fix" or "this fix" or "a fix" in a non-template context
        if ("the fix" in bl or "this fix" in bl) and "expected behavior" not in bl[:100]:
            ack_signals.append((idx, c["author"], "the_fix_ref"))

    # Check if this issue number matches a PR in commit_corpus
    has_merged_pr = num in pr_linked_issues

    # Denial check: is it a real denial, or just quoting the issue?
    real_denial = False
    best_deny = None
    for sd in deny_signals:
        # "by design", "not a bug", "expected_behavior" are clear denials
        if sd[3] in ("by_design", "not_a_bug", "wontfix"):
            real_denial = True
            best_deny = sd
            break
        # "invalid" is clear denial only if it's "invalid template/issue/report"
        if sd[3] == "invalid_template":
            real_denial = True
            best_deny = sd
            break

    # Classification
    if real_denial and not ack_signals and not has_merged_pr:
        cls = "negative"
        conf = 0.92
        attitude = best_deny[3]
        rationale = f"Maintainer @{best_deny[1]} denied ('{best_deny[2]}') in comment #{best_deny[0]}"
        neg_by_reason[attitude] = neg_by_reason.get(attitude, 0) + 1

        classified.append({
            "issue_number": num, "classification": cls, "confidence": conf,
            "classification_rationale": rationale, "developer_attitude": attitude,
            "rejecting_comment_index": best_deny[0],
            "rejecting_author_role": comments[best_deny[0]]['role'] if best_deny[0] < len(comments) else "maintainer",
        })
        stats["negative"] += 1
        continue

    # Merged fix PR
    if has_merged_pr:
        pr_info = pr_linked_issues[num]
        pr_nums = [p['number'] for p in pr_info]
        cls = "positive"
        conf = 0.95
        attitude = "acknowledged_and_fixed"
        rationale = f"Linked merged PR(s) #{','.join(map(str, pr_nums))} found in commit corpus"

        for lbl in labels:
            pos_by_label[lbl] = pos_by_label.get(lbl, 0) + 1

        classified.append({
            "issue_number": num, "classification": cls, "confidence": conf,
            "classification_rationale": rationale, "developer_attitude": attitude,
            "acknowledging_comment_index": None,
            "acknowledging_author_role": "maintainer",
        })
        stats["positive"] += 1
        continue

    # Ack signals
    if ack_signals:
        ack = ack_signals[0]
        cls = "positive"
        if state == "closed":
            conf = 0.90
            attitude = "acknowledged_and_fixed"
        else:
            conf = 0.80
            attitude = "acknowledged_but_unfixed"
        rationale = f"Maintainer @{ack[1]} acknowledged (keyword: '{ack[2]}') in comment #{ack[0]}"

        for lbl in labels:
            pos_by_label[lbl] = pos_by_label.get(lbl, 0) + 1

        classified.append({
            "issue_number": num, "classification": cls, "confidence": conf,
            "classification_rationale": rationale, "developer_attitude": attitude,
            "acknowledging_comment_index": ack[0],
            "acknowledging_author_role": comments[ack[0]]['role'] if ack[0] < len(comments) else "maintainer",
        })
        stats["positive"] += 1
        continue

    # Has linked PR
    if has_pr or linked_prs_in_issue:
        cls = "positive"
        conf = 0.70
        attitude = "acknowledged_but_unfixed"
        rationale = f"Linked PR(s) found: {linked_prs_in_issue if linked_prs_in_issue else 'associated_pr'}"

        for lbl in labels:
            pos_by_label[lbl] = pos_by_label.get(lbl, 0) + 1

        classified.append({
            "issue_number": num, "classification": cls, "confidence": conf,
            "classification_rationale": rationale, "developer_attitude": attitude,
        })
        stats["positive"] += 1
        continue

    # No clear signal -> invalid
    has_mc = bool(maintainer_comments)
    if not has_mc:
        inv_reason = "no_maintainer_response"
        conf = 0.85
    elif "stale" in labels:
        inv_reason = "stale_bot_closed"
        conf = 0.80
    elif "triage/needs-information" in labels:
        inv_reason = "needs_information"
        conf = 0.70
    else:
        last_mc = maintainer_comments[-1][1]['body'].lower()
        if "i think" in last_mc or "maybe" in last_mc or "could you" in last_mc or "?" in last_mc:
            inv_reason = "ambiguous"
            conf = 0.60
        else:
            inv_reason = "ambiguous"
            conf = 0.55

    inv_by_reason[inv_reason] = inv_by_reason.get(inv_reason, 0) + 1
    classified.append({
        "issue_number": num, "classification": "invalid", "confidence": conf,
        "classification_rationale": f"No clear bug confirmation or denial from maintainer ({inv_reason})",
        "developer_attitude": "unclear" if inv_reason != "ambiguous" else "ambiguous",
    })
    stats["invalid"] += 1

# Save Step 1 output
classified_output = {
    "_meta": {
        "target": "milvus",
        "analyzed_at": now,
        "total_classified": len(classified),
        "positive": stats["positive"],
        "negative": stats["negative"],
        "invalid": stats["invalid"],
    },
    "classified": classified,
    "statistics": {
        "positive_by_label": dict(sorted(pos_by_label.items(), key=lambda x: -x[1])[:30]),
        "negative_by_reason": neg_by_reason,
        "invalid_by_reason": inv_by_reason,
    }
}

with open(r'C:\Users\11428\Desktop\mftui\TestVDB\intelligence\milvus\classified_issues.json.tmp', 'w', encoding='utf-8') as f:
    json.dump(classified_output, f, ensure_ascii=False, indent=2)

print(f"Classification: {stats['positive']} positive, {stats['negative']} negative, {stats['invalid']} invalid")

# Step 2: Bug Shape Extraction
positive_issues = [c for c in classified if c['classification'] == 'positive']
positive_nums = {c['issue_number'] for c in positive_issues}
issue_map = {i['number']: i for i in issues}

# Helper
def find_pr_for_issue(num):
    if num in pr_linked_issues:
        return pr_linked_issues[num]
    return []

def categorize_issue(issue, pr_infos=None):
    """Analyze an issue and return extracted features"""
    num = issue['number']
    title = issue['title']
    body = issue.get('body', '')
    body_lower = body.lower()
    comments = issue.get('comments', [])
    all_text = title.lower() + ' ' + body_lower
    if pr_infos:
        for p in pr_infos:
            all_text += ' ' + (p.get('title', '') or '') + ' '
            all_text += ' ' + (p.get('body', '') or '') + ' '
            for f in p.get('changed_files', []):
                all_text += ' ' + f

    features = {}

    rc_candidates = {}

    if any(kw in all_text for kw in ['race', 'concurrent', 'deadlock', 'mutex', 'lock',
                                       'goroutine leak', 'data race', 'synchronization',
                                       'timer', 'timeout', 'stuck']):
        rc_candidates['concurrency_race'] = all_text.count('race') + all_text.count('concurrent') + all_text.count('lock')

    if any(kw in all_text for kw in ['inconsistent', 'state', 'stale cache', 'cache invalidation',
                                       'out of sync', 'desync', 'dirty', 'version mismatch',
                                       'target version', 'not found', 'missing']):
        rc_candidates['state_consistency'] = sum(1 for kw in ['inconsistent', 'state', 'stale', 'cache', 'sync', 'missing'] if kw in all_text)

    if any(kw in all_text for kw in ['panic', 'nil pointer', 'nil dereference', 'crash',
                                       'segfault', 'segmentation', 'SIGSEGV', 'OOM',
                                       'oomkill', 'resource leak']):
        rc_candidates['error_handling'] = all_text.count('panic') + all_text.count('nil') + all_text.count('crash') + all_text.count('OOM')

    if any(kw in all_text for kw in ['memory', 'leak', 'oom', 'resource', 'pool',
                                       'connection pool', 'unbounded']):
        rc_candidates['resource_management'] = sum(1 for kw in ['memory', 'leak', 'oom', 'unbounded'] if kw in all_text)

    if any(kw in all_text for kw in ['validation', 'invalid', 'valid', 'constraint',
                                       'null', 'empty', 'missing param', 'required']):
        rc_candidates['parameter_validation'] = sum(1 for kw in ['invalid', 'valid', 'null', 'empty', 'missing'] if kw in all_text)

    if any(kw in all_text for kw in ['REST', 'restful', 'return code', 'wrong result',
                                       'error code', 'error message', 'meaningless',
                                       'incorrect', 'wrong', 'unexpected']):
        rc_candidates['api_contract_violation'] = sum(1 for kw in ['wrong', 'incorrect', 'return', 'error code', 'message'] if kw in all_text)

    if any(kw in all_text for kw in ['overflow', 'boundary', 'limit', 'max', 'min',
                                       'edge case', 'corner', 'out of range']):
        rc_candidates['boundary_handling'] = sum(1 for kw in ['overflow', 'boundary', 'limit', 'max', 'out of range'] if kw in all_text)

    if any(kw in all_text for kw in ['performance', 'slow', 'latency', 'throughput',
                                       'regression', 'RT', 'runtime']):
        rc_candidates['performance_regression'] = sum(1 for kw in ['performance', 'slow', 'latency', 'regression'] if kw in all_text)

    if any(kw in all_text for kw in ['serialize', 'deserialize', 'parquet', 'delta log',
                                       'binlog', 'arrow', 'protobuf']):
        rc_candidates['serialization_deserialization'] = sum(1 for kw in ['serialize', 'binlog', 'delta'] if kw in all_text)

    if any(kw in all_text for kw in ['config', 'default', 'setting', 'param', 'milvus.yaml']):
        rc_candidates['configuration_defaults'] = sum(1 for kw in ['config', 'default', 'setting'] if kw in all_text)

    if rc_candidates:
        features['root_cause'] = max(rc_candidates, key=rc_candidates.get)
    else:
        features['root_cause'] = 'error_handling'

    # Affected Layer
    layer_candidates = {}
    if any(kw in all_text for kw in ['proxy', 'REST', 'restful', 'handler', 'endpoint', 'API']):
        layer_candidates['api_gateway'] = all_text.count('proxy') + all_text.count('REST') + all_text.count('handler')

    if any(kw in all_text for kw in ['query', 'search', 'request', 'parse', 'serialize', 'deserialize']):
        layer_candidates['request_parsing'] = all_text.count('query') + all_text.count('search') + all_text.count('request')

    if any(kw in all_text for kw in ['compaction', 'merge', 'segment', 'insert', 'delete',
                                       'upsert', 'index', 'load', 'flush', 'sync']):
        layer_candidates['business_logic'] = all_text.count('compaction') + all_text.count('segment') + all_text.count('index') + all_text.count('load')

    if any(kw in all_text for kw in ['storage', 'data', 'binlog', 'delta', 'parquet',
                                       'etcd', 'minio', 's3', 'file', 'disk']):
        layer_candidates['storage_engine'] = all_text.count('storage') + all_text.count('data') + all_text.count('binlog') + all_text.count('file')

    if any(kw in all_text for kw in ['streaming', 'channel', 'mq', 'kafka', 'pulsar',
                                       'rocksmq', 'message', 'wal']):
        layer_candidates['networking'] = all_text.count('streaming') + all_text.count('channel') + all_text.count('mq')

    if any(kw in all_text for kw in ['distribution', 'coordinator', 'coord', 'balance',
                                       'replica', 'delegator', 'leader', 'scheduler']):
        layer_candidates['business_logic'] = layer_candidates.get('business_logic', 0) + all_text.count('coord') + all_text.count('balance')

    features['layer'] = max(layer_candidates, key=layer_candidates.get) if layer_candidates else 'business_logic'

    # Defect Type
    if features['root_cause'] in ('parameter_validation', 'api_contract_violation', 'serialization_deserialization'):
        features['defect_type'] = 'Type1_IllegalSuccess'
    elif features['root_cause'] in ('error_handling', 'logging_diagnostics'):
        features['defect_type'] = 'Type2_PoorDiagnostics'
    elif features['root_cause'] in ('boundary_handling', 'resource_management', 'memory_management', 'performance_regression'):
        features['defect_type'] = 'Type3_RuntimeFailure'
    elif features['root_cause'] in ('concurrency_race', 'state_consistency', 'configuration_defaults'):
        features['defect_type'] = 'Type4_StateViolation'
    else:
        features['defect_type'] = 'Type3_RuntimeFailure'

    # Cross-DB applicability
    if features['layer'] in ('api_gateway', 'request_parsing'):
        features['cross_db'] = 'cross_db_applicable'
    elif features['root_cause'] in ('parameter_validation', 'api_contract_violation', 'serialization_deserialization'):
        features['cross_db'] = 'cross_db_applicable'
    elif features['root_cause'] in ('concurrency_race', 'state_consistency'):
        features['cross_db'] = 'partially_applicable'
    else:
        features['cross_db'] = 'db_specific'

    return features

# Extract shapes
shape_map = {}

for num in sorted(positive_nums):
    issue = issue_map[num]
    pr_infos = find_pr_for_issue(num)
    features = categorize_issue(issue, pr_infos)
    rc = features['root_cause']
    layer = features['layer']

    shape_key = f"{rc}:{layer}"

    instance = {
        "issue_number": num,
        "title": issue['title'][:100],
    }

    if pr_infos:
        for p in pr_infos:
            instance["fix_pr"] = p['number']
            instance["fix_pattern"] = p.get('title', '')
            instance["changed_files"] = p.get('changed_files', [])[:10]

    if shape_key not in shape_map:
        shape_map[shape_key] = {
            "root_cause_category": rc,
            "affected_layer": layer,
            "defect_type_mapping": features['defect_type'],
            "cross_db_applicability": features['cross_db'],
            "historical_instances": [],
            "source_issues_count": 0,
        }

    shape_map[shape_key]["historical_instances"].append(instance)
    shape_map[shape_key]["source_issues_count"] += 1

# Generate shape descriptions
bug_shapes_list = []
for key, data in sorted(shape_map.items()):
    rc = data['root_cause_category']
    layer = data['affected_layer']

    shape_id = f"milvus-{rc}-{layer}"

    pr_count = len(set(
        inst.get('fix_pr') for inst in data['historical_instances']
        if inst.get('fix_pr')
    ))

    desc_map = {
        "concurrency_race": f"Concurrency race condition at {layer} layer causing inconsistent state or deadlock",
        "state_consistency": f"State consistency violation at {layer} layer due to incorrect cache or version management",
        "error_handling": f"Error handling failure at {layer} layer causing panic, crash, or silent failure",
        "resource_management": f"Resource management issue at {layer} layer causing memory leak or OOM",
        "parameter_validation": f"Missing or insufficient parameter validation at {layer} layer",
        "api_contract_violation": f"API contract violation at {layer} layer returning incorrect status or data",
        "boundary_handling": f"Boundary/edge case handling defect at {layer} layer",
        "performance_regression": f"Performance regression at {layer} layer causing slowdown or resource exhaustion",
        "serialization_deserialization": f"Serialization/deserialization error at {layer} layer",
        "configuration_defaults": f"Configuration or default values issue at {layer} layer",
    }
    desc = desc_map.get(rc, f"Bug at {layer} layer related to {rc}")

    symptom_map = {
        "concurrency_race": "Search/query returns empty or incorrect results; system deadlock; operation timeout",
        "state_consistency": "Operation fails with 'not found' or 'stale' error; inconsistent state after restart",
        "error_handling": "System crash/panic; nil pointer dereference; OOM kill; silent failure with no error",
        "resource_management": "Memory growth unbounded; OOM kill under load; goroutine leak",
        "parameter_validation": "API accepts invalid input; returns 200 instead of 400; silent no-op on invalid params",
        "api_contract_violation": "Wrong HTTP status code; incorrect error message; operation returns success but does nothing",
        "boundary_handling": "Out-of-range values cause crash; edge cases not handled; limit checks missing",
        "performance_regression": "Latency spikes; throughput degradation after upgrade; query response time increases",
        "serialization_deserialization": "Data corruption during read/write; parquet format error; binlog deserialization failure",
        "configuration_defaults": "Default config causes resource exhaustion; misconfiguration leads to failure",
    }
    symptom = symptom_map.get(rc, "Unexpected behavior causing system failure or data inconsistency")

    attack_hints = {
        "concurrency_race": ["Concurrent search/query operations during rolling restart", "Interleave DDL and DQL operations under load", "Trigger segment loading while querying"],
        "state_consistency": ["Force coordinator failover during write operations", "Cause target version mismatch through rapid load/release cycle", "Trigger etcd partition during DDL operations"],
        "error_handling": ["Send malformed requests to trigger nil pointer paths", "Exhaust resources then observe panic handling", "Force storage errors during segment loading"],
        "resource_management": ["Send continuous high-throughput writes to trigger OOM", "Cause channel checkpoint buildup through slow consumers", "Force unbounded memory allocation through large batches"],
        "parameter_validation": ["Send requests with missing required parameters", "Test boundary values for collection/index parameters", "Send empty or null values to REST endpoints"],
        "api_contract_violation": ["Check HTTP status codes vs actual operation success", "Verify error messages contain actionable information", "Test operations that silently succeed but do nothing"],
        "boundary_handling": ["Test with maximum collection/partition counts", "Use extreme vector dimensions or batch sizes", "Create collections with minimum/maximum schema sizes"],
        "performance_regression": ["Measure query latency during rolling upgrades", "Compare search throughput under concurrent load", "Test with mixed vector types (dense + sparse)"],
        "serialization_deserialization": ["Corrupted binlog/delta log files", "Cross-version data format incompatibility", "Large parquet file with edge case encodings"],
        "configuration_defaults": ["Test with default configuration under production load", "Change mmap settings and observe memory behavior", "Modify compaction intervals and observe resource usage"],
    }
    hints = attack_hints.get(rc, ["General probing of the endpoint for unexpected behavior"])

    shape_entry = {
        "shape_id": shape_id,
        "name": f"{rc.replace('_', ' ').title()} at {layer.replace('_', ' ').title()} Layer",
        "root_cause_category": rc,
        "affected_layer": layer,
        "defect_type_mapping": data['defect_type_mapping'],
        "cross_db_applicability": data['cross_db_applicability'],
        "description": desc,
        "symptom_pattern": symptom,
        "historical_instances": data['historical_instances'],
        "attack_strategy_hints": hints,
        "confidence": min(0.95, 0.75 + data['source_issues_count'] * 0.05),
        "source_issues_count": data['source_issues_count'],
        "source_prs_count": pr_count,
    }
    bug_shapes_list.append(shape_entry)

# Save bug shapes
bug_shapes_output = {"bug_shapes": bug_shapes_list}
with open(r'C:\Users\11428\Desktop\mftui\TestVDB\intelligence\milvus\bug_shapes.json.tmp', 'w', encoding='utf-8') as f:
    json.dump(bug_shapes_output, f, ensure_ascii=False, indent=2)

print(f"\nBug shapes extracted: {len(bug_shapes_list)}")
for bs in bug_shapes_list:
    print(f"  {bs['shape_id']}: {bs['name']} ({bs['source_issues_count']} issues, {bs['source_prs_count']} PRs)")

# Step 3: Developer Cognition & By-Design Patterns
negative_issues = [c for c in classified if c['classification'] == 'negative']
negative_nums = {c['issue_number'] for c in negative_issues}

rejection_patterns = {}
by_design_patterns = []

for num in sorted(negative_nums):
    issue = issue_map.get(num)
    if not issue:
        continue
    comments = issue.get('comments', [])
    deny_info = next((c for c in classified if c['issue_number'] == num and c['classification'] == 'negative'), None)
    if not deny_info:
        continue

    attitude = deny_info['developer_attitude']
    rej_idx = deny_info.get('rejecting_comment_index', 0)

    # Extract by_design patterns
    if attitude in ("by_design", "expected_behavior", "wontfix", "not_a_bug", "invalid_template"):
        if rej_idx < len(comments):
            c_body = comments[rej_idx]['body'][:500]
        else:
            c_body = ""

        endpoint = "general API"
        if "REST" in issue.get('title', '') or "restful" in issue.get('title', '').lower():
            endpoint = "REST API"
        elif "v2" in issue.get('title', '').lower():
            endpoint = "REST v2 API"
        elif "search" in issue.get('title', '').lower():
            endpoint = "Search/Query"
        elif "load" in issue.get('title', '').lower():
            endpoint = "Load operation"
        elif "index" in issue.get('title', '').lower():
            endpoint = "Index operation"

        bdp = {
            "pattern_id": f"BDP-{num}",
            "pattern": f"{issue['title'][:100]}: {c_body[:200]}",
            "endpoint": endpoint,
            "developer_quote": c_body[:300],
            "source_issue_numbers": [num],
            "source_comment_index": rej_idx,
            "developer_attitude": attitude,
            "should_report": False,
            "classification": "FALSE POSITIVE" if attitude in ("by_design", "expected_behavior") else "LOW PRIORITY",
            "attack_guidance": f"DO NOT report {issue['title'][:80]} as a defect. The team explicitly stated this is {attitude}."
        }
        by_design_patterns.append(bdp)

    pattern_key = attitude
    if pattern_key not in rejection_patterns:
        rejection_patterns[pattern_key] = {
            "pattern_id": f"RP-{len(rejection_patterns)+1:03d}",
            "rejection_reason": attitude,
            "description": "",
            "example_issues": [],
            "developer_rationale_summary": "",
            "attack_guidance": "",
            "affected_endpoints_pattern": "",
            "frequency": 0,
        }
    rejection_patterns[pattern_key]["example_issues"].append(num)
    rejection_patterns[pattern_key]["frequency"] += 1

desc_map = {
    "by_design": {
        "desc": "API intentionally designed to accept certain inputs or behave in specific ways; framework layer handles secondary validation",
        "guidance": "DO NOT attack: by-design behaviors. Verify that the framework layer actually performs expected validation instead.",
        "endpoints": "REST API endpoints, proxy handlers",
    },
    "expected_behavior": {
        "desc": "Documented or intended behavior; system operates as specified in documentation",
        "guidance": "Check documentation to confirm expected behavior. If documented, do not report as defect.",
        "endpoints": "Query/Search operations, collection loading, index building",
    },
    "not_a_bug": {
        "desc": "Misunderstanding of system behavior; issue is not a bug in the code",
        "guidance": "Verify understanding of system architecture before reporting.",
        "endpoints": "RBAC operations, authentication endpoints",
    },
    "wontfix": {
        "desc": "Acknowledged but not prioritized for fix due to complexity, low impact, or architectural constraints",
        "guidance": "Can be reported but mark as low priority. Include context about why team declined to fix.",
        "endpoints": "Various",
    },
    "invalid_template": {
        "desc": "Issue report does not follow required template; missing environment/configuration details or user error",
        "guidance": "Ensure reports follow project's issue template and include all required information.",
        "endpoints": "All",
    },
}
for pk, rp in rejection_patterns.items():
    info = desc_map.get(pk, {"desc": "Maintainer indicated this is not a bug", "guidance": "Verify with project documentation", "endpoints": "Unknown"})
    rp["description"] = info["desc"]
    rp["attack_guidance"] = info["guidance"]
    rp["affected_endpoints_pattern"] = info["endpoints"]
    rp["developer_rationale_summary"] = f"Based on {rp['frequency']} issue(s) where maintainers classified the behavior as {pk}"

developer_cognition = {
    "rejection_patterns": list(rejection_patterns.values()),
    "by_design_patterns": by_design_patterns,
    "developer_cognition_signals": {
        "what_developers_consider_not_bugs": [
            "Embedded etcd token management for non-root users is an inherent behavior, not a bug",
            "Lock files left on object storage during segment writes are expected behavior",
            "Suspend/Resume APIs are designed for upgrade scheduling and do not change node runtime state",
            "Restful API returning code=0 is by design; only HTTP error codes indicate failure",
            "RBAC access control granularity (collectionAdmin vs collection access) is intentional design",
            "Memory quota exceeded on query nodes during imbalance is by design for resource sharing",
        ],
        "what_developers_prioritize": [
            "Data consistency and durability over strict API parameter validation",
            "Production stability and recovery speed over comprehensive error messages",
            "Performance optimization under normal load over graceful degradation under extreme conditions",
            "Distributed system availability during rolling upgrades over per-request atomicity",
        ],
        "blindspot_indicators": [
            "Developers assume well-formed client requests; edge cases like empty/null parameters are not systematically tested",
            "Concurrent operations during upgrade/restart scenarios are not comprehensively validated",
            "Error messages often lack actionable diagnostic information for end users",
            "Memory management under sustained high throughput is tuned reactively rather than proactively",
            "Channel checkpoint management assumes consumer-side health; runaway lag detection is insufficient",
        ]
    }
}

with open(r'C:\Users\11428\Desktop\mftui\TestVDB\intelligence\milvus\developer_cognition.json.tmp', 'w', encoding='utf-8') as f:
    json.dump(developer_cognition, f, ensure_ascii=False, indent=2)

print(f"\nDeveloper cognition: {len(rejection_patterns)} rejection patterns, {len(by_design_patterns)} by-design patterns")
print(f"Rejection reasons: {list(rejection_patterns.keys())}")

# Validation
print(f"\n{'='*60}")
print("VALIDATION")
print(f"{'='*60}")
print(f"Total issues: {len(issues)}")
print(f"Classified: {len(classified)} (100%)")
print(f"  Positive: {stats['positive']} ({stats['positive']/len(issues)*100:.1f}%)")
print(f"  Negative: {stats['negative']} ({stats['negative']/len(issues)*100:.1f}%)")
print(f"  Invalid:  {stats['invalid']} ({stats['invalid']/len(issues)*100:.1f}%)")
print(f"Bug shapes extracted: {len(bug_shapes_list)}")
print(f"Minimum required: 3 - {'PASS' if len(bug_shapes_list) >= 3 else 'FAIL'}")

# Verify coverage
positive_issue_numbers_in_shapes = set()
for bs in bug_shapes_list:
    for inst in bs['historical_instances']:
        positive_issue_numbers_in_shapes.add(inst['issue_number'])
uncovered = positive_nums - positive_issue_numbers_in_shapes
if uncovered:
    print(f"WARNING: {len(uncovered)} positive issue(s) not covered by any bug shape")
    for n in sorted(uncovered)[:5]:
        print(f"  #{n}: {issue_map[n]['title'][:80]}")
else:
    print(f"All {len(positive_nums)} positive issues covered by bug shapes: PASS")

# Finalize: rename .tmp to final
base = r'C:\Users\11428\Desktop\mftui\TestVDB\intelligence\milvus'
for name in ['classified_issues', 'bug_shapes', 'developer_cognition']:
    tmp = os.path.join(base, f'{name}.json.tmp')
    final = os.path.join(base, f'{name}.json')
    if os.path.exists(tmp):
        shutil.move(tmp, final)
        print(f"Moved {tmp} -> {final}")
        done = os.path.join(base, f'{name}.json.done')
        with open(done, 'w') as f:
            f.write(now)
        print(f"Created {done}")
