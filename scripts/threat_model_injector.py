#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Threat Model Injector v2.1"""

import json
import os
import sys
from pathlib import Path
from typing import Optional, Dict

from _pipeline_utils import setup_encoding

VALID_TARGETS = {"milvus", "qdrant", "weaviate", "pgvector", "meilisearch", "chroma"}

PROJECT_ROOT = os.environ.get("PROJECT_ROOT", os.getcwd())
_proj_resolved = Path(PROJECT_ROOT).resolve()

def _validate_path_inside_project(resolved):
    try:
        resolved.relative_to(_proj_resolved)
    except ValueError:
        raise ValueError("path must be inside project root")


def load_threat_model(target):
    if target not in VALID_TARGETS:
        return None
    p = _proj_resolved / "intelligence" / target / "threat_model.json"
    try:
        _validate_path_inside_project(p.resolve())
        with open(p, encoding="utf-8") as fh:
            return json.load(fh)
    except (FileNotFoundError, json.JSONDecodeError, ValueError):
        return None

# ---- Attack Agent injection ----

def _bridge_attack_surface_to_blindspots(tm):
    """Fallback: when attack_priority_map.endpoints is empty, bridge
    attack_surface.high_priority_areas + cognitive_blindspots to build
    endpoint priority data.

    Strategy:
    1. Read attack_surface.*_priority_areas for endpoint lists
    2. Read cognitive_blindspots.blindspots for attack_strategies
    3. Read cognitive_blindspots.attack_strategy_mapping for blindspot→agent→strategy
    4. Merge into endpoint-level priority data
    """
    endpoints = []
    atk = tm.get("attack_surface", {})
    cb = tm.get("cognitive_blindspots", {})
    blindspots_data = {b["blindspot_id"]: b for b in cb.get("blindspots", [])}

    # Build blindspot→primary_agent mapping from attack_strategy_mapping
    sm = cb.get("attack_strategy_mapping", {})
    if isinstance(sm, dict):
        strategy_list = sm.get("blindspot_to_strategy", [])
    else:
        strategy_list = sm if isinstance(sm, list) else []

    bs_agent_map = {}
    for x in strategy_list:
        bid = x.get("blindspot_id", "")
        agent = x.get("primary_attack_agent", "")
        if bid and agent:
            bs_agent_map[bid] = agent

    # Process all priority levels
    for level, areas in [("high", atk.get("high_priority_areas", [])),
                          ("medium", atk.get("medium_priority_areas", [])),
                          ("low", atk.get("low_priority_areas", []))]:
        for area in areas:
            area_name = area.get("area", "?")
            eps = area.get("mapped_contract_endpoints", [])
            hdc = area.get("historical_defect_count", 0)
            bs_ids = area.get("blindspots", [])
            ao = area.get("attack_order", [])

            # If blindspots field is missing, try to infer from area semantics
            if not bs_ids:
                bs_ids = _infer_blindspots_for_area(area_name, blindspots_data)

            # If attack_order is missing, build from blindspot strategies
            if not ao and bs_ids:
                ao = _build_attack_order_from_blindspots(bs_ids, blindspots_data, bs_agent_map)

            # Build cross-DB score from blindspot transferability
            cross_db = _compute_cross_db_score(bs_ids, blindspots_data)

            for ep in eps:
                ep_normalized = ep.replace("+", "/").replace(" ", "")
                endpoints.append({
                    "endpoint": ep,
                    "overall_priority": level,
                    "priority_factors": {
                        "blindspot_coverage": bs_ids,
                        "historical_defect_count": hdc,
                        "cross_db_vulnerability_score": cross_db
                    },
                    "recommended_attack_order": ao,
                    "area": area_name
                })

    # Deduplicate by endpoint (keep highest priority)
    seen = {}
    for ep in endpoints:
        key = ep["endpoint"]
        if key not in seen:
            seen[key] = ep
        else:
            # Keep higher priority entry
            prio_order = {"high": 0, "medium": 1, "low": 2}
            if prio_order.get(ep["overall_priority"], 99) < prio_order.get(seen[key]["overall_priority"], 99):
                seen[key] = ep

    return list(seen.values())


def _infer_blindspots_for_area(area_name, blindspots_data):
    """Infer which blindspots apply to an attack surface area based on area name and blindspot semantics."""
    area_lower = area_name.lower()
    matches = []
    for bid, bdata in blindspots_data.items():
        strategies = [s.lower() for s in bdata.get("attack_strategies", [])]
        name_lower = bdata.get("name", "").lower()
        desc_lower = bdata.get("description", "").lower()

        score = 0
        # Storage engine error handling → BS with runtime/error strategies
        if "error" in area_lower or "error handling" in area_lower:
            if any("error" in s or "runtime" in s or "crash" in s for s in strategies):
                score += 2
        # Parameter validation → BS with boundary/type/param strategies
        if "parameter" in area_lower or "validation" in area_lower:
            if any("boundary" in s or "type_confusion" in s or "missing_param" in s or "parameter" in s for s in strategies):
                score += 2
        # State consistency → BS with state/race/concurrent strategies
        if "state" in area_lower or "consistency" in area_lower:
            if any("state" in s or "race" in s or "concurrent" in s or "consistency" in s for s in strategies):
                score += 2
        # API contract → BS with contract/parity strategies
        if "contract" in area_lower or "api" in area_lower:
            if any("contract" in s or "parity" in s or "interface" in s for s in strategies):
                score += 2
        # Diagnostic → BS with error_quality/semantic strategies
        if "diagnos" in area_lower or "error message" in area_lower:
            if any("error_quality" in s or "semantic" in s or "diagnos" in s for s in strategies):
                score += 2
        # Memory → BS with resource/memory strategies
        if "memory" in area_lower:
            if any("resource" in s or "memory" in s or "pressure" in s for s in strategies):
                score += 2

        if score >= 1:
            matches.append((bid, score))

    matches.sort(key=lambda x: -x[1])
    return [m[0] for m in matches[:3]]


def _build_attack_order_from_blindspots(bs_ids, blindspots_data, bs_agent_map):
    """Build attack_order from blindspot strategy data when it's missing from the threat model."""
    order = []
    for bid in bs_ids:
        bdata = blindspots_data.get(bid)
        if not bdata:
            continue
        strategies = bdata.get("attack_strategies", [])
        for strategy in strategies[:2]:  # Top 2 strategies per blindspot
            # Map strategy name to constraint keywords
            strategy_lower = strategy.lower()
            if "boundary" in strategy_lower:
                constraints = ["limit_range", "dimension_range", "batch_size"]
            elif "type_confusion" in strategy_lower or "missing_param" in strategy_lower:
                constraints = ["param_type", "null_vs_empty", "missing_required"]
            elif "race" in strategy_lower or "concurrent" in strategy_lower:
                constraints = ["concurrent_ops", "atomic_state"]
            elif "recovery" in strategy_lower:
                constraints = ["snapshot_state", "alias_integrity"]
            elif "error_quality" in strategy_lower or "semantic" in strategy_lower:
                constraints = ["error_message", "status_code"]
            elif "resource" in strategy_lower or "memory" in strategy_lower:
                constraints = ["memory_pressure", "connection_pool"]
            elif "interface" in strategy_lower or "parity" in strategy_lower:
                constraints = ["rest_vs_grpc", "response_parity"]
            else:
                constraints = ["general"]

            order.append({
                "strategy": strategy_lower,
                "blindspot": bid,
                "constraints": constraints
            })
    return order[:6]


def _compute_cross_db_score(bs_ids, blindspots_data):
    """Compute cross-DB transferability score from blindspot data."""
    if not bs_ids:
        return 0.5
    scores = []
    for bid in bs_ids:
        bdata = blindspots_data.get(bid)
        if bdata and bdata.get("cross_db_transferable"):
            scores.append(0.85)
        elif bdata:
            scores.append(0.5)
    return round(sum(scores) / len(scores), 2) if scores else 0.5


def _inject_attack_priority(tm):
    r = []
    apm = tm.get("attack_priority_map", {})
    eps = apm.get("endpoints", [])
    # Fallback: if attack_priority_map is empty, bridge from attack_surface + cognitive_blindspots
    if not eps:
        eps = _bridge_attack_surface_to_blindspots(tm)
    r.append("### Attack Surface Priority")
    r.append("")
    if not eps:
        r.append("(No priority data)")
        r.append("")
    else:
        for ep in eps[:8]:
            nm = ep.get("endpoint", "?")
            pr = ep.get("overall_priority", "?")
            f = ep.get("priority_factors", {})
            bs = f.get("blindspot_coverage", [])
            hc = f.get("historical_defect_count", 0)
            cs = f.get("cross_db_vulnerability_score")
            if cs is None: cs = "N/A"
            r.append(f"#### {nm} (Priority: {pr})")
            r.append(f"- Hist defects: {hc}")
            bss = ", ".join(bs) if bs else "None"
            r.append(f"- Blindspots: {bss}")
            r.append(f"- Cross-DB: {cs}")
            r.append("")
            ra = ep.get("recommended_attack_order", ep.get("attack_order", []))
            if ra:
                r.append("**Attack order:**")
                for x in ra[:6]:
                    s = x.get("strategy", "?")
                    c = ", ".join(x.get("constraints", [])[:3])
                    b = x.get("blindspot", "?")
                    r.append(f"  - {s} -> BS {b}: {c}")
                r.append("")
    gw = apm.get("global_strategy_weights", {})
    if gw:
        r.append("### Global Weights")
        r.append("")
        for k, v in gw.items():
            r.append(f"  - {k}: {v:.0%}")
        r.append("")
    return r

def _inject_cognitive_blindspots(tm):
    r = []
    cb = tm.get("cognitive_blindspots", {})
    bs = cb.get("blindspots", [])
    r.append("### Cognitive Blindspots")
    r.append("")
    if bs:
        for b in bs[:3]:
            bid = b.get("blindspot_id", "?")
            bn = b.get("name", "?")
            bd = b.get("description", "")
            ast = b.get("attack_strategies", [])
            tmf = b.get("typical_manifestation", "")
            r.append(f"#### {bid}: {bn}")
            r.append(f"  {bd}")
            if tmf: r.append(f"  Manifestation: {tmf}")
            if ast: r.append("  Attacks: " + ", ".join(ast))
            r.append("")
    else:
        r.append("(No blindspot data)")
        r.append("")
    sm = cb.get("attack_strategy_mapping", {})
    if isinstance(sm, dict):
        strategy_list = sm.get("blindspot_to_strategy", [])
    else:
        strategy_list = sm  # backward compat: raw list
    if strategy_list:
        r.append("### Attack Mapping")
        r.append("")
        for x in strategy_list[:5]:
            r.append("  - " + (x.get("blindspot_id", x.get("blindspot_name","?"))) + " -> " + x.get("primary_attack_agent","?") + ": " + x.get("strategy_focus",""))
        r.append("")
    else:
        r.append("(No mapping data)")
        r.append("")
    return r

def _inject_by_design_behaviors(tm):
    r = []
    bd = tm.get("defect_criteria", {}).get("by_design_behaviors", [])
    r.append("### By-Design Behaviors (Avoid)")
    r.append("")
    if bd:
        for x in bd:
            r.append("  - " + x.get("pattern","") + ": " + x.get("rationale",""))
            r.append("")
    else:
        r.append("(No by-design data)")
        r.append("")
    return r


def generate_attack_injection(tm):
    if tm is None:
        return "(threat model unavailable)"
    p = ["## 威胁模型与认知盲点注入（v2.1 Strategic Intelligence）", ""]
    p.extend(_inject_attack_priority(tm))
    p.extend(_inject_cognitive_blindspots(tm))
    p.extend(_inject_by_design_behaviors(tm))
    return chr(10).join(p)


# ---- Judge Agent injection ----


def generate_judge_severity_injection(tm):
    if tm is None:
        return "(threat model unavailable)"
    j = tm.get("judge_enhancements", {})
    c = j.get("severity_calibration", {})
    if not c:
        return "(no severity calibration)"
    p = ["## 严重性校准规则（v2.1 Strategic Intelligence）", ""]
    for k, lbl in [("by_design_behaviors","Auto-downgrade"),("out_of_scope_patterns","Out-of-scope"),("historical_high_severity_patterns","Confirm severity"),("wontfix_patterns","Downgrade")]:
        sec = c.get(k, {})
        if sec.get("action"):
            p.append("### " + lbl)
            p.append("Reason: " + sec.get("rationale",""))
            p.append("")
            for pt in sec.get("patterns", []):
                p.append("  - " + pt)
            p.append("")
    return chr(10).join(p)

def generate_judge_novelty_injection(tm):
    if tm is None:
        return "(threat model unavailable)"
    j = tm.get("judge_enhancements", {})
    nc = j.get("novelty_context", {})
    if not nc:
        return "(no novelty context)"
    p = ["## 新颖性上下文（v2.1 Strategic Intelligence）", ""]
    rfp = nc.get("recently_fixed_patterns", [])
    if rfp:
        p.append("### Recently Fixed")
        for r in rfp:
            pat = r.get("pattern", "")
            fix = r.get("fix_pr", "")
            p.append("  - " + pat + " (PR #" + str(fix) + ")")
        p.append("")
    koi = nc.get("known_ongoing_issues", [])
    if koi:
        issues = ", ".join("#" + str(i) for i in koi)
        p.append("### Ongoing: " + issues)
        p.append("")
    rra = nc.get("regression_risk_areas", [])
    if rra:
        p.append("### Regression Risks")
        for r in rra:
            if isinstance(r, dict):
                r = r.get("area") or r.get("name") or r.get("description") or r.get("pattern") or r.get("id") or str(r)
            p.append("  - " + str(r))
        p.append("")
    return chr(10).join(p)

def generate_judge_evidence_injection(tm):
    if tm is None:
        return "(threat model unavailable)"
    j = tm.get("judge_enhancements", {})
    ssp = j.get("submission_success_probability", {})
    if not ssp:
        return "(no submission success data)"
    p = ["## 提交成功率校准（v2.1 Strategic Intelligence）", ""]
    for k, lbl in [("high","High >0.8"),("medium","Medium 0.4-0.8"),("low","Low <0.4")]:
        items = ssp.get(k, [])
        if items:
            p.append("### " + lbl)
            for x in items:
                prob = x.get("probability")
                if prob is None: prob = 0
                cond = x.get("condition", "")
                reason = x.get("reason", "")
                p.append("  - " + cond + " (" + str(prob) + "): " + reason)
            p.append("")
    return chr(10).join(p)


# ---- Main entry ----


def main():
    setup_encoding()
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("target_db", choices=sorted(VALID_TARGETS))
    ap.add_argument("--mode", required=True, choices=["attack","judge"])
    ap.add_argument("--judge-type", choices=["severity","novelty","evidence"])
    ap.add_argument("--text-only", action="store_true")
    ap.add_argument("--intelligence-dir", default=None)
    args = ap.parse_args()
    if args.mode == "judge" and not args.judge_type:
        print("Error: --judge-type required", file=sys.stderr)
        sys.exit(1)
    if args.intelligence_dir:
        try:
            _validate_path_inside_project(Path(args.intelligence_dir).resolve())
            tm_path = Path(args.intelligence_dir).resolve() / "threat_model.json"
            with open(tm_path, encoding="utf-8") as fh:
                tm = json.load(fh)
        except (ValueError, FileNotFoundError, json.JSONDecodeError):
            tm = None
    else:
        tm = load_threat_model(args.target_db)
    meta = {"mode": args.mode, "target": args.target_db, "has_threat_model": tm is not None}
    if args.mode == "attack":
        txt = generate_attack_injection(tm)
        if tm:
            meta["blindspot_count"] = len(tm.get("cognitive_blindspots", {}).get("blindspots", []))
            meta["endpoint_count"] = len(tm.get("attack_priority_map", {}).get("endpoints", []))
    else:
        gens = {"severity": generate_judge_severity_injection, "novelty": generate_judge_novelty_injection, "evidence": generate_judge_evidence_injection}
        txt = gens[args.judge_type](tm)
    if args.text_only:
        print(txt)
    else:
        if args.mode == "judge":
            meta["judge_type"] = args.judge_type
        print(json.dumps({**meta, "injection_text": txt, "injection_length": len(txt)}, indent=2, ensure_ascii=False))

if __name__ == "__main__":
    main()