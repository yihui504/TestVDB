#!/usr/bin/env python3
"""
Final Bug Shape Extraction Pipeline for Milvus.
- Accurate issue classification based on manual review
- Bug shape extraction from positive samples
- Developer cognition analysis from negative samples
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

issue_map = {i['number']: i for i in issues}

# ── Step 1: Manual Ground-Truth Classification ──
# Based on detailed comment analysis of all 150 issues

# EXPERT RULINGS:
# Issues with maintainer explicitly stating "by design", "not a bug", "expected behavior" (own words)
NEGATIVE_BY_DESIGN = {36261, 35732, 35969, 33913, 40638}
NEGATIVE_NOT_A_BUG = {45709}
NEGATIVE_EXPECTED_BEHAVIOR = {44369, 40896, 45210, 43295}
# Issues where maintainer said issue is invalid template or not a proper report
NEGATIVE_INVALID_TEMPLATE = {39445, 44517}

negative_nums = NEGATIVE_BY_DESIGN | NEGATIVE_NOT_A_BUG | NEGATIVE_EXPECTED_BEHAVIOR | NEGATIVE_INVALID_TEMPLATE

# Issues that have merged fix PR in commit_corpus
POSITIVE_HAS_FIX_PR = set(pr_linked_issues.keys())

# Issues with explicit developer acknowledgment (ack signals like "reproduced", "confirmed", "good catch", "will fix")
POSITIVE_ACKNOWLEDGED = {
    36593, 36228, 37272, 42098, 36217, 37098, 43295, 43494, 36404,
    43310, 44745, 44857, 45368, 34148, 36370, 37209, 44172, 45557,
    41570, 48903, 36598, 37574, 43186, 43370, 33782, 38736, 43266,
    43318, 43592, 45274, 34553, 40638, 40207, 42129,
}

# Issues with linked PRs (even if no maintainer comment)
POSITIVE_HAS_LINKED_PR = {35329, 44071, 44713, 45368}

positive_nums = POSITIVE_HAS_FIX_PR | POSITIVE_ACKNOWLEDGED | POSITIVE_HAS_LINKED_PR

# Rest are invalid
invalid_nums = set()
for i in issues:
    n = i['number']
    if n not in positive_nums and n not in negative_nums:
        invalid_nums.add(n)

# Verify no overlap
assert positive_nums.isdisjoint(negative_nums), f"Overlap: {positive_nums & negative_nums}"
assert positive_nums.isdisjoint(invalid_nums)
assert negative_nums.isdisjoint(invalid_nums)
assert len(positive_nums) + len(negative_nums) + len(invalid_nums) == 150

print(f"Classification: {len(positive_nums)} positive, {len(negative_nums)} negative, {len(invalid_nums)} invalid")

# ── Build classified_issues output ──
classified = []
stats = {"positive": 0, "negative": 0, "invalid": 0}
pos_by_label = {}
neg_by_reason = {}
inv_by_reason = {}

for issue in issues:
    num = issue["number"]
    labels = issue.get("labels", [])
    comments = issue.get("comments", [])

    if num in positive_nums:
        cls = "positive"
        stats["positive"] += 1
        for lbl in labels:
            pos_by_label[lbl] = pos_by_label.get(lbl, 0) + 1

        # Determine attitude
        if num in POSITIVE_HAS_FIX_PR:
            pr_info = pr_linked_issues[num]
            pr_nums = [p['number'] for p in pr_info]
            attitude = "acknowledged_and_fixed"
            conf = 0.95
            rationale = f"Linked merged PR(s) #{','.join(map(str, pr_nums))} found in commit corpus"
            ack_idx = None
            ack_role = "maintainer"
        elif num in POSITIVE_ACKNOWLEDGED:
            # Find the ack comment
            ack_idx = None
            ack_author = "maintainer"
            for idx, c in enumerate(comments):
                if c["role"] in ("contributor", "maintainer"):
                    bl = c["body"].lower()
                    for kw in ["reproduced", "confirmed", "good catch", "will fix"]:
                        if kw in bl:
                            ack_idx = idx
                            ack_author = c["author"]
                            break
                if ack_idx is not None:
                    break
            attitude = "acknowledged_and_fixed" if issue.get("state") == "closed" else "acknowledged_but_unfixed"
            conf = 0.90 if issue.get("state") == "closed" else 0.80
            rationale = f"Maintainer @{ack_author} acknowledged in comment #{ack_idx}" if ack_idx else "Maintainer acknowledged the bug"
        else:
            attitude = "acknowledged_but_unfixed"
            conf = 0.70
            rationale = "Linked PR(s) found without explicit maintainer confirmation"

        # Handle the special case of #44892 (in deny_analysis but also has fix comment)
        if num == 44892:
            # This one has "fix" in comment #14 from tinswzy
            cls = "positive"
            attitude = "acknowledged_and_fixed"
            conf = 0.80
            rationale = "Maintainer provided fix guidance despite initially questioning issue format"
            stats["negative"] -= 1  # undo
            stats["positive"] += 1

        classified.append({
            "issue_number": num,
            "classification": cls,
            "confidence": conf,
            "classification_rationale": rationale,
            "developer_attitude": attitude,
        })

    elif num in negative_nums:
        cls = "negative"
        stats["negative"] += 1

        # Map to attitude
        if num in NEGATIVE_BY_DESIGN:
            attitude = "by_design"
        elif num in NEGATIVE_NOT_A_BUG:
            attitude = "not_a_bug"
        elif num in NEGATIVE_EXPECTED_BEHAVIOR:
            attitude = "expected_behavior"
        elif num in NEGATIVE_INVALID_TEMPLATE:
            attitude = "invalid_template"
        else:
            attitude = "by_design"

        neg_by_reason[attitude] = neg_by_reason.get(attitude, 0) + 1

        # Find the denying comment
        rej_idx = None
        rej_author = ""
        for idx, c in enumerate(comments):
            if c["role"] in ("contributor", "maintainer"):
                bl = c["body"].lower()
                for phrase in ["by design", "not a bug", "expected behavior", "invalid"]:
                    if phrase in bl:
                        rej_idx = idx
                        rej_author = c["author"]
                        break
            if rej_idx is not None:
                break

        conf = 0.92
        rationale = f"Maintainer @{rej_author} indicated '{attitude}' in comment #{rej_idx}" if rej_author else "Maintainer indicated this is not a bug"

        classified.append({
            "issue_number": num,
            "classification": cls,
            "confidence": conf,
            "classification_rationale": rationale,
            "developer_attitude": attitude,
        })

    else:
        cls = "invalid"
        stats["invalid"] += 1

        # Determine invalidity reason
        has_mc = any(c["role"] in ("contributor", "maintainer") for c in comments)
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
            inv_reason = "ambiguous"
            conf = 0.60

        inv_by_reason[inv_reason] = inv_by_reason.get(inv_reason, 0) + 1

        classified.append({
            "issue_number": num,
            "classification": cls,
            "confidence": conf,
            "classification_rationale": f"No clear bug confirmation or denial ({inv_reason})",
            "developer_attitude": "unclear" if inv_reason != "ambiguous" else "ambiguous",
        })

# Write classified_issues
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

print(f"\nClassification done: P={stats['positive']} N={stats['negative']} I={stats['invalid']}")

# ── Step 2: Bug Shape Extraction ──
def find_pr_for_issue(num):
    if num in pr_linked_issues:
        return pr_linked_issues[num]
    return []

def categorize_issue(issue, pr_infos=None):
    """Analyze an issue and return extracted features"""
    # Collect all text for analysis
    num = issue['number']
    title = issue['title']
    body = issue.get('body', '')
    body_lower = body.lower()
    all_text = title.lower() + ' ' + body_lower
    if pr_infos:
        for p in pr_infos:
            all_text += ' ' + (p.get('title', '') or '') + ' '
            all_text += ' ' + (p.get('body', '') or '') + ' '
            for f in p.get('changed_files', []):
                all_text += ' ' + f

    features = {}
    rc_candidates = {}

    # Concurrency/Race
    if any(kw in all_text for kw in ['race', 'concurrent', 'deadlock', 'mutex', 'lock',
                                       'goroutine leak', 'data race', 'synchronization',
                                       'timer', 'timeout', 'stuck']):
        rc_candidates['concurrency_race'] = all_text.count('race') + all_text.count('concurrent') + all_text.count('lock') + all_text.count('stuck')

    # State consistency
    if any(kw in all_text for kw in ['inconsistent', 'stale cache', 'cache invalidation',
                                       'out of sync', 'desync', 'dirty', 'version mismatch',
                                       'target version', 'not found', 'missing', 'failed to get']):
        rc_candidates['state_consistency'] = sum(1 for kw in ['inconsistent', 'stale', 'cache', 'sync', 'missing', 'version'] if kw in all_text)

    # Error handling (panic/crash/SIGSEGV)
    if any(kw in all_text for kw in ['panic', 'nil pointer', 'nil dereference', 'crash',
                                       'segfault', 'segmentation', 'SIGSEGV', 'SIGSEGV']):
        rc_candidates['error_handling'] = all_text.count('panic') + all_text.count('nil') + all_text.count('crash')

    # Resource management (memory/OOM)
    if any(kw in all_text for kw in ['memory', 'leak', 'oom', 'oomkill', 'oom killed',
                                       'resource', 'unbounded', 'cpu']):
        rc_candidates['resource_management'] = sum(1 for kw in ['memory', 'leak', 'oom', 'oomkill', 'unbounded'] if kw in all_text)

    # Parameter validation
    if any(kw in all_text for kw in ['validation', 'invalid', 'null', 'empty', 'missing param',
                                       'required', 'not validate']):
        rc_candidates['parameter_validation'] = sum(1 for kw in ['invalid', 'null', 'empty', 'missing', 'validate'] if kw in all_text)

    # API contract violation
    if any(kw in all_text for kw in ['REST', 'restful', 'return code', 'wrong result',
                                       'error code', 'error message', 'meaningless',
                                       'incorrect', 'wrong']):
        rc_candidates['api_contract_violation'] = sum(1 for kw in ['wrong', 'incorrect', 'return', 'code', 'message'] if kw in all_text)

    # Performance regression
    if any(kw in all_text for kw in ['performance', 'slow', 'latency', 'throughput',
                                       'regression', 'RT']):
        rc_candidates['performance_regression'] = sum(1 for kw in ['performance', 'slow', 'latency', 'regression'] if kw in all_text)

    # Boundary handling
    if any(kw in all_text for kw in ['overflow', 'boundary', 'limit', 'max', 'min',
                                       'edge case', 'out of range']):
        rc_candidates['boundary_handling'] = sum(1 for kw in ['overflow', 'boundary', 'limit', 'max', 'out of range'] if kw in all_text)

    # Serialization
    if any(kw in all_text for kw in ['serialize', 'deserialize', 'parquet', 'delta log',
                                       'binlog', 'arrow']):
        rc_candidates['serialization_deserialization'] = sum(1 for kw in ['serialize', 'binlog', 'delta'] if kw in all_text)

    # Configuration defaults
    if any(kw in all_text for kw in ['config', 'default', 'setting']):
        rc_candidates['configuration_defaults'] = sum(1 for kw in ['config', 'default', 'setting'] if kw in all_text)

    features['root_cause'] = max(rc_candidates, key=rc_candidates.get) if rc_candidates else 'error_handling'

    # Affected Layer
    layer_candidates = {}
    if any(kw in all_text for kw in ['proxy', 'REST', 'restful', 'handler', 'endpoint']):
        layer_candidates['api_gateway'] = all_text.count('proxy') + all_text.count('REST') + all_text.count('handler')
    if any(kw in all_text for kw in ['query', 'search', 'request', 'parse']):
        layer_candidates['request_parsing'] = all_text.count('query') + all_text.count('search') + all_text.count('request')
    if any(kw in all_text for kw in ['compaction', 'merge', 'segment', 'insert', 'delete',
                                       'upsert', 'index', 'load', 'flush', 'sync', 'dml']):
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

    # Defect Type mapping
    if features['root_cause'] in ('parameter_validation', 'api_contract_violation'):
        features['defect_type'] = 'Type1_IllegalSuccess'
    elif features['root_cause'] in ('error_handling',):
        features['defect_type'] = 'Type2_PoorDiagnostics'
    elif features['root_cause'] in ('boundary_handling', 'resource_management', 'performance_regression'):
        features['defect_type'] = 'Type3_RuntimeFailure'
    elif features['root_cause'] in ('concurrency_race', 'state_consistency', 'configuration_defaults'):
        features['defect_type'] = 'Type4_StateViolation'
    else:
        features['defect_type'] = 'Type3_RuntimeFailure'

    # Cross-DB applicability
    if features['layer'] in ('api_gateway',):
        features['cross_db'] = 'cross_db_applicable'
    elif features['root_cause'] in ('parameter_validation', 'api_contract_violation'):
        features['cross_db'] = 'cross_db_applicable'
    elif features['root_cause'] in ('concurrency_race', 'state_consistency'):
        features['cross_db'] = 'partially_applicable'
    else:
        features['cross_db'] = 'db_specific'

    return features

# Extract shapes
shape_map = {}

for num in sorted(positive_nums):
    # Skip #44892 which was corrected but may not fit well
    if num == 44892:
        continue
    issue = issue_map[num]
    pr_infos = find_pr_for_issue(num)
    features = categorize_issue(issue, pr_infos)
    rc = features['root_cause']
    layer = features['layer']

    shape_key = f"{rc}:{layer}"

    instance = {"issue_number": num, "title": issue['title'][:100]}

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

# Build bug shapes with rich descriptions
bug_shapes_list = []
for key, data in sorted(shape_map.items()):
    rc = data['root_cause_category']
    layer = data['affected_layer']
    shape_id = f"milvus-{rc}-{layer}"

    pr_count = len(set(inst.get('fix_pr') for inst in data['historical_instances'] if inst.get('fix_pr')))

    # Rich descriptions per shape category
    shape_descriptions = {
        "concurrency_race:business_logic": {
            "name": "Concurrency Race Condition in Data Operations",
            "desc": "Concurrent access to shared data structures (segments, channels, delegators) without proper synchronization causes race conditions, leading to inconsistent state, deadlock, or silent data loss",
            "symptom": "Search/query returns empty or incorrect results; system deadlock; operation timeout during concurrent DML/DQL",
        },
        "concurrency_race:request_parsing": {
            "name": "Concurrency Race Condition in Query Request Handling",
            "desc": "Race conditions during query request parsing or routing cause queries to fail intermittently, particularly during rolling upgrades or when multiple requests arrive simultaneously",
            "symptom": "Queries intermittently failing with 'delegator not found' or 'channel distribution not serviceable' errors",
        },
        "concurrency_race:storage_engine": {
            "name": "Concurrency Race Condition in Storage Engine",
            "desc": "Race conditions in storage layer operations (flush, compaction, segment sync) cause data inconsistency or corruption when multiple goroutines access shared state without coordination",
            "symptom": "Data inconsistency after concurrent write operations; flush failures; corrupted segments",
        },
        "concurrency_race:networking": {
            "name": "Concurrency Race Condition in Message Queue Layer",
            "desc": "Race conditions in MQ/channel operations (consumer registration, timer usage, dispatcher) cause streaming pipeline disruptions",
            "symptom": "Channel stuck; refresh load fails; timer misuse leading to dispatcher hold",
        },
        "state_consistency:business_logic": {
            "name": "State Consistency Violation in Coordinator Operations",
            "desc": "Stale cache or out-of-sync state between coordinators and workers causes inconsistent behavior after leadership changes or upgrades",
            "symptom": "Operations fail after coordinator failover; stale cache returns incorrect metadata",
        },
        "resource_management:request_parsing": {
            "name": "Memory/Resource Exhaustion During Query Processing",
            "desc": "Unbounded memory allocation during query processing causes OOM kills, particularly under high concurrency or with large result sets",
            "symptom": "QueryNode OOM killed during concurrent DML/DQL; memory grows unboundedly",
        },
        "resource_management:business_logic": {
            "name": "Resource Exhaustion in Data Operations",
            "desc": "Unbounded resource consumption during data operations (compaction, stats, index building) causes node instability",
            "symptom": "DataNode/IndexNode OOM during background operations; CPU/memory runaway",
        },
        "resource_management:networking": {
            "name": "Resource Leak in Message Queue Subsystem",
            "desc": "Resource leaks in message queue or streaming layer cause unbounded memory growth or goroutine leaks",
            "symptom": "Pulsar memory not released; channel checkpoint lag growth; etcd OOM",
        },
        "error_handling:storage_engine": {
            "name": "Panic/Crash in Storage Engine Error Paths",
            "desc": "Error handling gaps in storage engine cause nil pointer dereference, panic, or silent failures during edge case scenarios",
            "symptom": "SIGSEGV during storage operations; crash after etcd failure; nil dereference in segment loading",
        },
        "parameter_validation:request_parsing": {
            "name": "Missing Parameter Validation in Request Parsing",
            "desc": "Insufficient input validation allows invalid parameters through, causing silent failures or inconsistent state downstream",
            "symptom": "API accepts invalid parameters silently; wrong error codes returned to clients",
        },
        "api_contract_violation:api_gateway": {
            "name": "REST API Contract Violation",
            "desc": "RESTful API endpoints return incorrect status codes, error messages, or silently accept invalid operations",
            "symptom": "API returns 200 for failed operations; error messages are meaningless or misleading",
        },
    }

    sd_key = f"{rc}:{layer}"
    if sd_key in shape_descriptions:
        sd = shape_descriptions[sd_key]
        name = sd["name"]
        desc = sd["desc"]
        symptom = sd["symptom"]
    else:
        name = f"{rc.replace('_', ' ').title()} at {layer.replace('_', ' ').title()} Layer"
        desc = f"Bug at {layer} layer related to {rc}"
        symptom = "Unexpected behavior causing system failure or data inconsistency"

    attack_hints_map = {
        "concurrency_race": [
            "Concurrent search/query operations during rolling restart triggers race conditions",
            "Interleave DDL (create/drop collection) and DQL (search/query) operations under load",
            "Trigger rapid load/release cycle while querying to hit version mismatch races",
        ],
        "state_consistency": [
            "Force coordinator failover during write operations to trigger stale cache",
            "Cause target version mismatch through rapid load/release cycle",
            "Trigger etcd partition during DDL operations to cause state desync",
        ],
        "error_handling": [
            "Send malformed requests to trigger nil pointer dereference paths",
            "Exhaust disk/quota then observe panic handling during load operations",
            "Force etcd failure during node startup to test crash handling",
        ],
        "resource_management": [
            "Send continuous high-throughput writes to trigger OOM in query nodes",
            "Cause channel checkpoint buildup through slow consumers",
            "Force unbounded memory allocation through large batch insert/search",
        ],
        "parameter_validation": [
            "Send requests with missing required parameters to test validation gaps",
            "Test boundary values for collection/index parameters (nlist, dim, max_length)",
            "Send empty or null values to REST endpoints expecting non-empty inputs",
        ],
        "api_contract_violation": [
            "Check HTTP status codes vs actual operation success for all REST endpoints",
            "Verify error messages contain actionable information (not just error codes)",
            "Test operations that silently succeed but do nothing (return 200, zero rows affected)",
        ],
        "boundary_handling": [
            "Test with maximum collection/partition counts (500 partitions)",
            "Use extreme vector dimensions or batch sizes (empty vectors, max int)",
            "Create collections with minimum/maximum schema sizes",
        ],
        "performance_regression": [
            "Measure query latency during rolling upgrades between versions",
            "Compare search throughput under concurrent load with and without streaming node",
            "Test with mixed vector types (dense + sparse) under sustained load",
        ],
        "serialization_deserialization": [
            "Corrupted binlog/delta log files during read-back",
            "Cross-version data format incompatibility during upgrade",
            "Large parquet file with edge case encodings",
        ],
        "configuration_defaults": [
            "Test with default configuration under production load",
            "Change mmap settings and observe memory behavior",
            "Modify compaction intervals and observe resource usage patterns",
        ],
    }

    shape_entry = {
        "shape_id": shape_id,
        "name": name,
        "root_cause_category": rc,
        "affected_layer": layer,
        "defect_type_mapping": data['defect_type_mapping'],
        "cross_db_applicability": data['cross_db_applicability'],
        "description": desc,
        "symptom_pattern": symptom,
        "historical_instances": data['historical_instances'],
        "attack_strategy_hints": attack_hints_map.get(rc, ["General probing for unexpected behavior"]),
        "confidence": min(0.95, 0.75 + data['source_issues_count'] * 0.05),
        "source_issues_count": data['source_issues_count'],
        "source_prs_count": pr_count,
    }
    bug_shapes_list.append(shape_entry)

# Save bug shapes
with open(r'C:\Users\11428\Desktop\mftui\TestVDB\intelligence\milvus\bug_shapes.json.tmp', 'w', encoding='utf-8') as f:
    json.dump({"bug_shapes": bug_shapes_list}, f, ensure_ascii=False, indent=2)

print(f"Bug shapes extracted: {len(bug_shapes_list)}")
for bs in bug_shapes_list:
    print(f"  {bs['shape_id']}: {bs['name']} ({bs['source_issues_count']} instances, {bs['source_prs_count']} PRs)")

# ── Step 3: Developer Cognition ──
rejection_patterns = {}
by_design_patterns = []

for num in sorted(negative_nums):
    issue = issue_map[num]
    comments = issue.get('comments', [])

    # Determine attitude from our classification
    if num in NEGATIVE_BY_DESIGN:
        attitude = "by_design"
    elif num in NEGATIVE_NOT_A_BUG:
        attitude = "not_a_bug"
    elif num in NEGATIVE_EXPECTED_BEHAVIOR:
        attitude = "expected_behavior"
    elif num in NEGATIVE_INVALID_TEMPLATE:
        attitude = "invalid_template"
    else:
        continue

    rej_idx = None
    rej_quote = ""
    for idx, c in enumerate(comments):
        if c["role"] in ("contributor", "maintainer"):
            bl = c["body"].lower()
            found = False
            for phrase in ["by design", "not a bug", "expected behavior", "invalid"]:
                if phrase in bl:
                    rej_idx = idx
                    rej_quote = c["body"][:500]
                    found = True
                    break
            if found:
                break

    # By-Design pattern entry
    endpoint = "general"
    if "REST" in issue.get('title', '') or "restful" in issue.get('title', '').lower():
        endpoint = "REST API"
    elif "v2" in issue.get('title', '').lower():
        endpoint = "REST v2 API"
    elif "load" in issue.get('title', '').lower():
        endpoint = "Collection/Partition Load"
    elif "search" in issue.get('title', '').lower() or "query" in issue.get('title', '').lower():
        endpoint = "Search/Query"

    bdp = {
        "pattern_id": f"BDP-{num}",
        "pattern": f"{issue['title'][:100]}",
        "endpoint": endpoint,
        "developer_quote": rej_quote[:300],
        "source_issue_numbers": [num],
        "source_comment_index": rej_idx,
        "developer_attitude": attitude,
        "should_report": False,
        "classification": "FALSE POSITIVE" if attitude in ("by_design", "expected_behavior") else "LOW PRIORITY",
        "attack_guidance": f"DO NOT report '{issue['title'][:80]}' as a defect. The team stated this is {attitude} behavior."
    }
    by_design_patterns.append(bdp)

    # Aggregate rejection patterns
    if attitude not in rejection_patterns:
        rejection_patterns[attitude] = {
            "pattern_id": f"RP-{len(rejection_patterns)+1:03d}",
            "rejection_reason": attitude,
            "description": "",
            "example_issues": [],
            "developer_rationale_summary": "",
            "attack_guidance": "",
            "affected_endpoints_pattern": "",
            "frequency": 0,
        }
    rejection_patterns[attitude]["example_issues"].append(num)
    rejection_patterns[attitude]["frequency"] += 1

desc_guidance = {
    "by_design": {
        "desc": "API intentionally designed to accept certain inputs or exhibit specific behaviors by design; not a bug",
        "guidance": "DO NOT attack by-design behaviors. Instead verify that the framework/upstream layer performs expected validation.",
        "endpoints": "REST API, collection operations, proxy handlers, load balancing",
    },
    "expected_behavior": {
        "desc": "System operates as documented or as intentionally designed; maintainer states this is expected behavior",
        "guidance": "Verify with project documentation. If documented, do not report as defect. Focus on undocumented behavior instead.",
        "endpoints": "Query operations, collection loading, node lifecycle, storage operations",
    },
    "not_a_bug": {
        "desc": "Misunderstanding of system architecture; behavior is correct from the project's perspective",
        "guidance": "Verify understanding of system design before reporting. Check documentation and API contracts first.",
        "endpoints": "RBAC operations, authentication, authorization endpoints",
    },
    "invalid_template": {
        "desc": "Issue report lacks required information (environment, reproduction steps, logs) or is environmental/user error",
        "guidance": "Ensure all reports include: Milvus version, deployment mode, MQ type, SDK version, OS, logs, and reproduction steps.",
        "endpoints": "All endpoints require proper issue formatting",
    },
}

for pk, rp in rejection_patterns.items():
    info = desc_guidance.get(pk, {"desc": "Maintainer indicated this is not a bug", "guidance": "Verify with documentation", "endpoints": "Unknown"})
    rp["description"] = info["desc"]
    rp["attack_guidance"] = info["guidance"]
    rp["affected_endpoints_pattern"] = info["endpoints"]
    total = rp["frequency"]
    examples_str = ", ".join(f"#{n}" for n in rp["example_issues"][:5])
    rp["developer_rationale_summary"] = f"Based on {total} issue(s) ({examples_str}) where maintainers classified the behavior as {pk}"

developer_cognition = {
    "rejection_patterns": list(rejection_patterns.values()),
    "by_design_patterns": by_design_patterns,
    "developer_cognition_signals": {
        "what_developers_consider_not_bugs": [
            "Lock files left on object storage during segment writes are expected behavior, cleaned up automatically",
            "If collection loading makes no progress within 10 minutes, auto-timeout is expected behavior for preventing indefinite hangs",
            "Suspend/Resume APIs are designed for Kubernetes upgrade scheduling; they do not change query node runtime state",
            "Restful HTTP API returning code=0 on error is by design; only HTTP status codes should be checked for errors",
            "Even vchannel distribution across streaming nodes is by design for WAL resource isolation, not guaranteed uniform",
            "QueryNode memory quota exceeded during node imbalance is expected resource sharing behavior",
            "RBAC access granularity (collectionLevel vs collectionAdmin) is intentional by design for privilege separation",
            "Channel checkpoint lag with downstream consumer behavior is not a bug; recovery through manual etcd cleanup is expected",
        ],
        "what_developers_prioritize": [
            "Data consistency and durability over strict API parameter validation",
            "Production stability and recovery speed over comprehensive error messages",
            "Performance under normal load over graceful degradation under extreme conditions",
            "Distributed system availability during rolling upgrades over per-request atomicity",
            "Fast path recovery from failures over exhaustive defensive checks",
        ],
        "blindspot_indicators": [
            "Developers assume well-formed client requests; edge cases (empty/null parameters) are not systematically validated",
            "Concurrent operations during upgrade/restart scenarios are not comprehensively tested at API level",
            "Error messages often lack actionable diagnostic information for end users, making troubleshooting harder",
            "Memory management under sustained high throughput is tuned reactively rather than proactively validated",
            "Channel checkpoint management assumes consumer-side health; runaway lag detection is insufficient",
            "File/index cleanup during segment lifecycle transitions (drop after GC) is not comprehensively handled",
            "Cross-version compatibility for stored indexes (null_offset files) is not tested during upgrades",
        ]
    }
}

with open(r'C:\Users\11428\Desktop\mftui\TestVDB\intelligence\milvus\developer_cognition.json.tmp', 'w', encoding='utf-8') as f:
    json.dump(developer_cognition, f, ensure_ascii=False, indent=2)

print(f"\nDeveloper cognition: {len(rejection_patterns)} rejection patterns, {len(by_design_patterns)} by-design patterns")
print(f"Rejection reasons: {list(rejection_patterns.keys())}")

# ── Validation ──
print(f"\n{'='*60}")
print("VALIDATION")
print(f"{'='*60}")
print(f"Total: {len(issues)} | P:{stats['positive']} N:{stats['negative']} I:{stats['invalid']}")
print(f"Bug shapes: {len(bug_shapes_list)} (min 3 required) {'PASS' if len(bug_shapes_list) >= 3 else 'FAIL'}")

positive_nums_in_shapes = set()
for bs in bug_shapes_list:
    for inst in bs['historical_instances']:
        positive_nums_in_shapes.add(inst['issue_number'])
uncovered = positive_nums - positive_nums_in_shapes
if uncovered:
    print(f"Uncovered positives: {len(uncovered)} issues: {sorted(uncovered)[:10]}")
else:
    print(f"All {len(positive_nums)} positives covered by bug shapes: PASS")

# ── Finalize ──
base = r'C:\Users\11428\Desktop\mftui\TestVDB\intelligence\milvus'
for name in ['classified_issues', 'bug_shapes', 'developer_cognition']:
    tmp = os.path.join(base, f'{name}.json.tmp')
    final = os.path.join(base, f'{name}.json')
    if os.path.exists(tmp):
        shutil.move(tmp, final)
        print(f"  {name}: written")
        with open(os.path.join(base, f'{name}.json.done'), 'w') as f:
            f.write(now)
