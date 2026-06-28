
#!/usr/bin/env python3
"""L1 Mechanical Gate — zero-LLM false positive filter."""
import json, os, re, sys
from pathlib import Path

if sys.platform == 'win32':
    sys.stdout.reconfigure(encoding='utf-8', errors='replace')

VECTOR_TYPE_DIMS = {'vector': 16000, 'halfvec': 4000, 'sparsevec': 1_000_000_000, 'bit': 64000}

def _find_log(sd, script_name):
    for p in Path(sd).glob(f'output_*{script_name}*.log'):
        return str(p)
    return None

def _content(path):
    try: return Path(path).read_text(encoding='utf-8', errors='replace').lower() if path else ''
    except: return ''

def _match(pattern, text):
    return bool(re.search(pattern, text, re.IGNORECASE))


# ── 11 mechanical checks — each returns (verdict|None, reason) ──────

def check_postgres_aborted(log_path):
    if _match(r"current transaction is aborted", _content(log_path)):
        return ("REFUTED", "PostgreSQL standard: transaction aborted masks errors — script connection isolation bug")
    return (None, "")

def check_syntax_error(log_path):
    c = _content(log_path)
    if _match(r"syntax error at or near", c):
        return ("REFUTED", "SCRIPT_ERROR: SQL syntax error, not a target defect")
    if _match(r"operator does not exist", c):
        return ("REFUTED", "SCRIPT_ERROR: wrong operator class or type mismatch")
    return (None, "")

def check_constraint_self_violation(candidate, contract):
    desc = candidate.get("description", "")
    m = re.match(
        r".*?(\w+)\s*=\s*(-?[\d.]+).*?(?:despite|violates|constraint|requires?)\s+.*?([><=]+\s*-?[\d.]+)",
        desc)
    if not m:
        return (None, "")
    pname, pval_s, ctext = m.group(1), m.group(2), m.group(3)
    pval = float(pval_s)
    cm = re.match(r"([><=]+)\s*(-?[\d.]+)", ctext.replace(" ", ""))
    if not cm:
        return (None, "")
    op, lim = cm.group(1), float(cm.group(2))
    violated = {
        "<": pval < lim, "<=": pval <= lim,
        ">": pval > lim, ">=": pval >= lim,
        "==": pval == lim, "!=": pval != lim
    }.get(op, False)
    if violated:
        return ("REFUTED",
                f"Constraint self-violation: {pname}={pval} violates {op}{lim} — script intentionally breaks known contract")
    return (None, "")

def check_type_dimension_confusion(candidate):
    desc = candidate.get("description", "").lower()
    endpoint = candidate.get("endpoint", "").lower()
    m = re.search(r"(?:dims?\s*[><=]+\s*|max[_\s]?dims?\s*=?\s*)(\d+)", desc)
    if not m:
        return (None, "")
    claimed = int(m.group(1))
    for vtype, actual in VECTOR_TYPE_DIMS.items():
        if vtype in endpoint or vtype in desc:
            if claimed < actual * 0.5:
                return ("REFUTED",
                        f"Type dimension confusion: {vtype} max dims={actual}, script claims limit={claimed} — confused with another vector type")
    return (None, "")

def check_guc_set_timing(log_path):
    c = _content(log_path)
    if _match(r"set\s+.*?(?:accepted|succeeded|ok|status=200|status=0)", c):
        if _match(r"(?:error|outside valid range|invalid)", c):
            return ("REFUTED",
                    "GUC validation timing: SET registers value, assign_hook validates at query time — PostgreSQL standard, not a defect")
    return (None, "")

def check_arithmetic_verification(candidate, log_path):
    c = _content(log_path)
    desc = candidate.get("description", "").lower()
    m = (re.search(r"expected\s+(\d+).*?got\s+(\d+)", desc)
         or re.search(r"expected\s+(\d+).*?got\s+(\d+)", c))
    if not m:
        return (None, "")
    expected, actual = int(m.group(1)), int(m.group(2))
    # find initial row count — handle "Inserted N vectors", "Inserted N rows", "count: N" etc
    row_counts = [int(x) for x in re.findall(r"(?:inserted\s+|count[=:]\s*|rows?[=:]\s*)(\d+)", c)]
    initial = max(row_counts) if row_counts else 0
    # sum deleted — handle "Deleted N rows", "Deleted ids X-Y" (count = Y-X+1), "Deleted item_X" (=1)
    total_del = 0
    for d in re.findall(r"deleted?\s+(\d+)", c):
        total_del += int(d)
    # "Deleted ids 5-10" → 10-5+1 = 6
    for rng in re.findall(r"deleted?\s+ids?\s+(\d+)\s*-\s*(\d+)", c):
        total_del += int(rng[1]) - int(rng[0]) + 1
    # "Deleted item_0" (singular) → count = 1
    for _ in re.finditer(r"deleted?\s+item_\d+|deleted?\s+row_\d+", c):
        total_del += 1
    if initial > 0 and total_del > 0:
        math_exp = initial - total_del
        if math_exp == actual and math_exp != expected:
            return ("REFUTED",
                    f"Arithmetic error: {initial}-{total_del}={math_exp}=actual({actual}), not claimed expected({expected})")
    return (None, "")

def check_no_index(log_path):
    if _match(r"(?:no\s+index|index\s+not\s+found|relation.*does\s+not\s+exist)", _content(log_path)):
        return ("REFUTED", "Missing index: script tests index behavior but index was never created")
    return (None, "")

def check_cross_type_cast(candidate, contract):
    desc = candidate.get("description", "").lower()
    m = re.search(r"(vector|halfvec|sparsevec|bit)\s*<->\s*(vector|halfvec|sparsevec|bit)", desc)
    if m and m.group(1) != m.group(2):
        return ("REFUTED",
                f"Cross-type cast ({m.group(1)}<->{m.group(2)}) is documented implicit behavior — not a defect")
    return (None, "")

def check_float_format(log_path):
    c = _content(log_path)
    if _match(r"(?:float|f-string|precision|rounding)", c) and _match(
            r"(?:mismatch|differ|not equal).*?(?:float|decimal|numeric)", c):
        return ("REFUTED", "Float format artifact: Python f-string vs PostgreSQL ::text CAST — use ::numeric(20,15)")
    return (None, "")

def check_same_distance_ordering(log_path):
    c = _content(log_path)
    if _match(r"same\s+distance|equal\s+distance|identical\s+distance", c) and _match(
            r"ordering|order\s+by|sort|position", c):
        return ("REFUTED", "SQL standard: ORDER BY equal-distance rows produces non-deterministic ordering")
    return (None, "")

def check_by_design_clamp(candidate, log_path):
    # Search script name + description + log content (script name often has key context log doesn't)
    haystack = f"{candidate.get('script','')} {candidate.get('description','')} {_content(log_path)}"
    if _match(r"subvector.*start.*0", haystack) and _match(r"accepted|ok|succeeded|200|result", haystack):
        return ("REFUTED",
                "by-design: subvector start<1 auto-clamped to 1 (mimics PostgreSQL substring semantics, source vector.c)")
    return (None, "")


# ── main ────────────────────────────────────────────────────────────

def verify_l1(session_dir, target="pgvector", db_url=None):
    """Run all L1 mechanical checks against confirmed defects."""
    agg_path = Path(session_dir) / "debate_logs" / "stage2_aggregation.json"
    if not agg_path.exists():
        return {"error": f"aggregation not found: {agg_path}"}

    agg = json.loads(agg_path.read_text(encoding="utf-8"))
    candidates = agg.get("confirmed_defects", [])

    # load contract for constraint lookups
    contract = None
    for cp in [
            Path(session_dir).parent / "structured_contract.json",
            Path(session_dir).parent.parent / "structured_contract.json",
    ]:
        if cp.exists():
            contract = json.loads(cp.read_text(encoding="utf-8"))
            break

    # ponytail: ordered by specificity — specific checks before generic ones
    checks = [
        ("postgres_aborted", lambda c, l: check_postgres_aborted(l)),
        ("syntax_error", lambda c, l: check_syntax_error(l)),
        ("constraint_self_violation", lambda c, l: check_constraint_self_violation(c, contract)),
        ("type_dimension_confusion", lambda c, l: check_type_dimension_confusion(c)),
        ("guc_timing", lambda c, l: check_guc_set_timing(l)),
        ("arithmetic", lambda c, l: check_arithmetic_verification(c, l)),
        ("cross_type_cast", lambda c, l: check_cross_type_cast(c, contract)),
        ("no_index", lambda c, l: check_no_index(l)),
        ("float_format", lambda c, l: check_float_format(l)),
        ("same_distance_ordering", lambda c, l: check_same_distance_ordering(l)),
        ("by_design_clamp", lambda c, l: check_by_design_clamp(c, l)),
    ]

    results = []
    for c in candidates:
        log_path = _find_log(session_dir, c.get("script", ""))
        verdict, reasons = "UNCERTAIN", []
        for cname, fn in checks:
            v, reason = fn(c, log_path)
            if v is not None:
                reasons.append(f"[{cname}] {reason}")
                if v == "REFUTED":
                    verdict = "REFUTED"
                    break  # first refutation is enough
        results.append({
            "defect_id": c.get("defect_id", "?"),
            "script": c.get("script", ""),
            "verdict": verdict,
            "reasons": reasons,
            "original_confidence": c.get("confidence", 0),
        })

    summary = {
        "total": len(candidates),
        "refuted": sum(1 for r in results if r["verdict"] == "REFUTED"),
        "uncertain": sum(1 for r in results if r["verdict"] == "UNCERTAIN"),
    }
    output = {"version": 1, "summary": summary, "results": results}
    out_path = Path(session_dir) / "verify_live_l1.json"
    out_path.write_text(json.dumps(output, indent=2, ensure_ascii=False), encoding="utf-8")
    return output


# ── self-check / cli ────────────────────────────────────────────────

def _demo():
    if len(sys.argv) < 2:
        print("Usage: python verify_live_l1.py <session_dir> [--target pgvector|weaviate|...] [--db-url url]")
        print("  ponytail: catches ~90% false positives with 0 LLM cost")
        return
    sd = sys.argv[1]
    target, db_url = "pgvector", None
    args = sys.argv[2:]
    i = 0
    while i < len(args):
        if args[i] == "--target" and i + 1 < len(args):
            target = args[i + 1]; i += 2
        elif args[i] == "--db-url" and i + 1 < len(args):
            db_url = args[i + 1]; i += 2
        else:
            i += 1
    r = verify_l1(sd, target, db_url)
    if "error" in r:
        print(f"ERROR: {r['error']}")
        return
    s = r["summary"]
    print(f"L1 Gate: {s['total']} candidates → {s['refuted']} REFUTED, {s['uncertain']} UNCERTAIN")
    for x in r["results"]:
        icon = "X" if x["verdict"] == "REFUTED" else "?"
        print(f"  [{icon}] {x['defect_id']} ({x['script']}): {x['verdict']}")
        for reason in x.get("reasons", []):
            print(f"       {reason}")


if __name__ == "__main__":
    _demo()
