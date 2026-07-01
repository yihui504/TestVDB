
#!/usr/bin/env python3
"""L1 Mechanical Gate — zero-LLM false positive filter (ADR-0006 Check Protocol)."""
from __future__ import annotations

import json, os, re, sys
from pathlib import Path
from typing import Optional

from _pipeline_utils import setup_encoding, read_json, write_json, debate_log_path, find_log
from checks import Check, CheckContext, Verdict

setup_encoding()

VECTOR_TYPE_DIMS = {'vector': 16000, 'halfvec': 4000, 'sparsevec': 1_000_000_000, 'bit': 64000}

def _content(path):
    try: return Path(path).read_text(encoding='utf-8', errors='replace').lower() if path else ''
    except: return ''

def _match(pattern, text):
    return bool(re.search(pattern, text, re.IGNORECASE))


# ── 11 Check protocol adapters (ADR-0006) ──────────────────────────

class PostgresAbortedCheck:
    """Transaction aborted masking — script connection isolation bug."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        if _match(r"current transaction is aborted", _content(log_path)):
            return Verdict("REFUTED", "PostgreSQL standard: transaction aborted masks errors — script connection isolation bug", "postgres_aborted")
        return None

class SyntaxErrorCheck:
    """SQL syntax errors — script bugs, not target defects."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        c = _content(log_path)
        if _match(r"syntax error at or near", c):
            return Verdict("REFUTED", "SCRIPT_ERROR: SQL syntax error, not a target defect", "syntax_error")
        if _match(r"operator does not exist", c):
            return Verdict("REFUTED", "SCRIPT_ERROR: wrong operator class or type mismatch", "syntax_error")
        return None

class ConstraintSelfViolationCheck:
    """Script intentionally breaks a known contract constraint — not a real defect."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        if ctx.contract is None:
            return None  # explicit: cannot check without contract
        desc = candidate.get("description", "")
        m = re.match(
            r".*?(\w+)\s*=\s*(-?[\d.]+).*?(?:despite|violates|constraint|requires?)\s+.*?([><=]+\s*-?[\d.]+)",
            desc)
        if not m:
            return None
        pname, pval_s, ctext = m.group(1), m.group(2), m.group(3)
        pval = float(pval_s)
        cm = re.match(r"([><=]+)\s*(-?[\d.]+)", ctext.replace(" ", ""))
        if not cm:
            return None
        op, lim = cm.group(1), float(cm.group(2))
        violated = {
            "<": pval < lim, "<=": pval <= lim,
            ">": pval > lim, ">=": pval >= lim,
            "==": pval == lim, "!=": pval != lim
        }.get(op, False)
        if violated:
            return Verdict("REFUTED",
                f"Constraint self-violation: {pname}={pval} violates {op}{lim} — script intentionally breaks known contract",
                "constraint_self_violation")
        return None

class TypeDimensionConfusionCheck:
    """Vector type dimension confusion — claimed limit << actual max dims."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        desc = candidate.get("description", "").lower()
        endpoint = candidate.get("endpoint", "").lower()
        m = re.search(r"(?:dims?\s*[><=]+\s*|max[_\s]?dims?\s*=?\s*)(\d+)", desc)
        if not m:
            return None
        claimed = int(m.group(1))
        for vtype, actual in VECTOR_TYPE_DIMS.items():
            if vtype in endpoint or vtype in desc:
                if claimed < actual * 0.5:
                    return Verdict("REFUTED",
                        f"Type dimension confusion: {vtype} max dims={actual}, script claims limit={claimed} — confused with another vector type",
                        "type_dimension_confusion")
        return None

class GucSetTimingCheck:
    """GUC validation timing — SET accepted, assign_hook validates later."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        c = _content(log_path)
        if _match(r"set\s+.*?(?:accepted|succeeded|ok|status=200|status=0)", c):
            if _match(r"(?:error|outside valid range|invalid)", c):
                return Verdict("REFUTED",
                    "GUC validation timing: SET registers value, assign_hook validates at query time — PostgreSQL standard, not a defect",
                    "guc_timing")
        return None

class ArithmeticVerificationCheck:
    """Arithmetic error — script miscalculated expected result."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        c = _content(log_path)
        desc = candidate.get("description", "").lower()
        m = (re.search(r"expected\s+(\d+).*?got\s+(\d+)", desc)
             or re.search(r"expected\s+(\d+).*?got\s+(\d+)", c))
        if not m:
            return None
        expected, actual = int(m.group(1)), int(m.group(2))
        row_counts = [int(x) for x in re.findall(r"(?:inserted\s+|count[=:]\s*|rows?[=:]\s*)(\d+)", c)]
        initial = max(row_counts) if row_counts else 0
        total_del = 0
        for d in re.findall(r"deleted?\s+(\d+)", c):
            total_del += int(d)
        for rng in re.findall(r"deleted?\s+ids?\s+(\d+)\s*-\s*(\d+)", c):
            total_del += int(rng[1]) - int(rng[0]) + 1
        for _ in re.finditer(r"deleted?\s+item_\d+|deleted?\s+row_\d+", c):
            total_del += 1
        if initial > 0 and total_del > 0:
            math_exp = initial - total_del
            if math_exp == actual and math_exp != expected:
                return Verdict("REFUTED",
                    f"Arithmetic error: {initial}-{total_del}={math_exp}=actual({actual}), not claimed expected({expected})",
                    "arithmetic")
        return None

class NoIndexCheck:
    """Missing index — script tests index behavior without creating one."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        if _match(r"(?:no\s+index|index\s+not\s+found|relation.*does\s+not\s+exist)", _content(log_path)):
            return Verdict("REFUTED", "Missing index: script tests index behavior but index was never created", "no_index")
        return None

class CrossTypeCastCheck:
    """Cross-type cast — documented implicit behavior, not a defect."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        desc = candidate.get("description", "").lower()
        m = re.search(r"(vector|halfvec|sparsevec|bit)\s*<->\s*(vector|halfvec|sparsevec|bit)", desc)
        if m and m.group(1) != m.group(2):
            return Verdict("REFUTED",
                f"Cross-type cast ({m.group(1)}<->{m.group(2)}) is documented implicit behavior — not a defect",
                "cross_type_cast")
        return None

class FloatFormatCheck:
    """Float format artifact — Python f-string vs PostgreSQL ::text CAST."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        c = _content(log_path)
        if _match(r"(?:float|f-string|precision|rounding)", c) and _match(
                r"(?:mismatch|differ|not equal).*?(?:float|decimal|numeric)", c):
            return Verdict("REFUTED", "Float format artifact: Python f-string vs PostgreSQL ::text CAST — use ::numeric(20,15)", "float_format")
        return None

class SameDistanceOrderingCheck:
    """Equal-distance ordering — SQL standard non-deterministic behavior."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        c = _content(log_path)
        if _match(r"same\s+distance|equal\s+distance|identical\s+distance", c) and _match(
                r"ordering|order\s+by|sort|position", c):
            return Verdict("REFUTED", "SQL standard: ORDER BY equal-distance rows produces non-deterministic ordering", "same_distance_ordering")
        return None

class ByDesignClampCheck:
    """By-design: subvector start<1 auto-clamped to 1 (PostgreSQL substring semantics)."""
    def check(self, candidate: dict, log_path: str, ctx: CheckContext) -> Optional[Verdict]:
        haystack = f"{candidate.get('script','')} {candidate.get('description','')} {_content(log_path)}"
        if _match(r"subvector.*start.*0", haystack) and _match(r"accepted|ok|succeeded|200|result", haystack):
            return Verdict("REFUTED",
                "by-design: subvector start<1 auto-clamped to 1 (mimics PostgreSQL substring semantics, source vector.c)",
                "by_design_clamp")
        return None


# ── Check registry (ADR-0006 — ordered by specificity) ────────────

ALL_CHECKS: list[Check] = [
    PostgresAbortedCheck(),
    SyntaxErrorCheck(),
    ConstraintSelfViolationCheck(),
    TypeDimensionConfusionCheck(),
    GucSetTimingCheck(),
    ArithmeticVerificationCheck(),
    CrossTypeCastCheck(),
    NoIndexCheck(),
    FloatFormatCheck(),
    SameDistanceOrderingCheck(),
    ByDesignClampCheck(),
]


# ── main ────────────────────────────────────────────────────────────

def verify_l1(session_dir, target="pgvector", db_url=None):
    """Run all L1 mechanical checks against confirmed defects (ADR-0006 Check Protocol)."""
    agg_path = debate_log_path(session_dir, "stage2_aggregation")
    if not agg_path.exists():
        return {"error": f"aggregation not found: {agg_path}"}

    agg = read_json(agg_path)
    if agg is None:
        return {"error": f"failed to read: {agg_path}"}
    candidates = agg.get("confirmed_defects", [])

    # load contract for constraint lookups
    contract = None
    for cp in [
            Path(session_dir).parent / "structured_contract.json",
            Path(session_dir).parent.parent / "structured_contract.json",
    ]:
        contract = read_json(cp)
        if contract is not None:
            break

    ctx = CheckContext(contract=contract, target=target)

    results = []
    for c in candidates:
        log_path = find_log(session_dir, c.get("script", ""))
        verdict, reasons = "UNCERTAIN", []
        for check in ALL_CHECKS:
            v = check.check(c, str(log_path) if log_path else "", ctx)
            if v is not None:
                reasons.append(f"[{v.check_name}] {v.reason}")
                if v.result == "REFUTED":
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
    output = {"version": 2, "summary": summary, "results": results}
    write_json(Path(session_dir) / "verify_live_l1.json", output)
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
