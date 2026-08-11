#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""_validate_contract.py — 通用 contract 确定性核验（chroma 经验提炼，不硬编码 target 事实）。

contract-formalizer 可能系统性幻觉 source_verified（chroma 3 轮全 0%，r3 谎报 100%）。
本脚本作为流水线 stage 强制核验，把 ad-hoc _build_*_canonical.py 模式固化为通用 stage。

Checks:
1. schema 合法性（_passport hash + api_endpoints 字段完整性）
2. CRUD 端点覆盖率 ≥ 90%
3. 每条 constraint source_url 真包含 assertion 关键短语（支持 github + 文档站 + 本地 doc_bundle）
4. 编造下限检测（regex `param >= 1` 但 source 只有 default 无 min）
5. DROP 比例 > 20% → 整 contract 不合格

source fetch 策略（优先级）：
  a. 本地 doc_bundle/*.md（若 --doc-bundle 指向）
  b. github.com source → raw.githubusercontent.com
  c. 任意 https URL 直接 fetch
  d. 全失败 → source_unverified（中性，触发 retry，不算 hallucination）

Usage:
    python scripts/_validate_contract.py results/{target}/{version}/structured_contract.json [--doc-bundle DIR]
Exit:
    0 = pass, 1 = fail (contract_validation_report.json 写同目录), 2 = usage/error
"""
from __future__ import annotations

import argparse
import datetime
import json
import re
import sys
import urllib.request
from pathlib import Path

# Windows cp1252 stdout 兼容（attack-boundary 模板同款）
if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

# ---------------- 常量 ----------------

CRUD_KEYWORDS = {
    "create": re.compile(r"\b(create|insert|upsert|put|post|add)\b", re.I),
    "read": re.compile(r"\b(get|list|search|query|find|scroll|describe|count)\b", re.I),
    "update": re.compile(r"\b(update|patch|set|modify)\b", re.I),
    "delete": re.compile(r"\b(delete|drop|remove|clear)\b", re.I),
}

MIN_CRUD_COVERAGE = 0.90
DROP_REJECT_RATIO = 0.20
FETCH_TIMEOUT = 20

# 编造下限检测：平凡 >= 1 / > 0
FABRICATED_LOWER_RE = re.compile(r"\b[a-z][a-z0-9_]*\s*>=?\s*1\b|\b[a-z][a-z0-9_]*\s*>\s*0\b", re.I)
# source 里若含这些词，说明 min/max 是显式文档化的（非编造）
EXPLICIT_BOUND_WORDS = re.compile(r"\b(min(?:imum)?|at least|lower bound|must be at least|range)\b", re.I)
# default 词（判断 source 是否只给 default）
DEFAULT_WORD = re.compile(r"\bdefault\b", re.I)

# cache: source_url -> text or None
_source_cache: dict[str, str | None] = {}


# ---------- source fetch ----------

def github_raw_url(url: str) -> str | None:
    m = re.match(r"https?://github\.com/([^/]+)/([^/]+)/blob/([^/]+)/(.+)", url)
    if not m:
        return None
    owner, repo, branch, path = m.groups()
    return f"https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}"


def fetch_text(url: str) -> str | None:
    if url in _source_cache:
        return _source_cache[url]
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "testvdb-contract-validate"})
        with urllib.request.urlopen(req, timeout=FETCH_TIMEOUT) as resp:
            text = resp.read().decode("utf-8", "replace")
        _source_cache[url] = text
        return text
    except Exception:
        _source_cache[url] = None
        return None


def get_source_text(source_url: str, doc_bundle_dir: Path | None) -> str | None:
    """优先级：本地 doc_bundle → github raw → 直接 URL → None."""
    # a. 本地 doc_bundle：用 source_url 的最后一段作 filename 匹配
    if doc_bundle_dir and doc_bundle_dir.exists():
        last = source_url.rsplit("/", 1)[-1].split("?")[0]
        for f in doc_bundle_dir.glob("*.md"):
            if last and last.split("#")[0] in f.name:
                return f.read_text(encoding="utf-8", errors="replace")
        # fallback: 全 bundle grep（小集合可接受）
        bundle_text = ""
        for f in doc_bundle_dir.glob("*.md"):
            bundle_text += f.read_text(encoding="utf-8", errors="replace") + "\n"
        if bundle_text:
            return bundle_text
    # b/c. 网络 fetch
    raw = github_raw_url(source_url) if "github.com" in source_url else source_url
    return fetch_text(raw) if raw else None


# ---------- 核验 check 函数 ----------

def extract_keywords(constraint: dict) -> list[str]:
    """提取用于核对的关键词（数值优先 + constraint_id 末段 + description 短语）。"""
    kws: list[str] = []
    a = constraint.get("assertion", "") or ""
    d = constraint.get("description", "") or ""
    cid = constraint.get("constraint_id", "") or ""
    for m in re.finditer(r"\b\d{2,}\b", a + " " + d):
        kws.append(m.group())
    parts = cid.split("_")
    if len(parts) >= 3:
        kws.append(parts[-3])
    if d:
        words = re.findall(r"[a-zA-Z_]+", d)[:2]
        if len(words) >= 2:
            kws.append(" ".join(words))
    return list(dict.fromkeys(kws))


def check_schema(contract: dict) -> list[dict]:
    failures: list[dict] = []
    passport = contract.get("_passport") or {}
    if not passport:
        failures.append({"check": "missing_passport", "detail": "_passport 缺失"})
    eps = contract.get("api_endpoints", []) or []
    if not eps:
        failures.append({"check": "empty_endpoints", "detail": "api_endpoints 为空"})
    for i, ep in enumerate(eps):
        if not ep.get("path") or not ep.get("method"):
            failures.append({"check": "endpoint_incomplete",
                             "detail": f"api_endpoints[{i}] 缺 path/method"})
    return failures


def check_crud_coverage(contract: dict) -> dict:
    eps = contract.get("api_endpoints", []) or []
    found = set()
    for ep in eps:
        path_method = ((ep.get("path") or "") + " " + (ep.get("method") or "")).lower()
        for cat, pat in CRUD_KEYWORDS.items():
            if pat.search(path_method):
                found.add(cat)
    coverage = len(found) / len(CRUD_KEYWORDS)
    return {
        "coverage": coverage,
        "found": sorted(found),
        "missing": sorted(set(CRUD_KEYWORDS) - found),
        "pass": coverage >= MIN_CRUD_COVERAGE,
    }


def classify_constraint(constraint: dict, source_text: str | None,
                        doc_bundle_dir: Path | None) -> tuple[str, str]:
    """返回 (label, reason). label ∈ {EXPLICIT, INFERRED, DROP, UNVERIFIED}."""
    cid = constraint.get("constraint_id", "") or ""
    a = (constraint.get("assertion", "") or "").lower()
    src_url = constraint.get("source_url", "") or ""

    # source 不可达 → UNVERIFIED（中性，触发 retry）
    if source_text is None:
        return "UNVERIFIED", "source unreachable (网络或本地 bundle 都拿不到)"

    # 编造下限检测：assertion 含 `param >= 1` 但 source 无 min/max 词
    if FABRICATED_LOWER_RE.search(a):
        if not EXPLICIT_BOUND_WORDS.search(source_text) and DEFAULT_WORD.search(source_text):
            return "DROP", f"编造下限: assertion='{a.strip()[:60]}' 但 source 只有 default 无 min"

    # 数值关键词必须全在 source 找到
    kws = extract_keywords(constraint)
    numeric = [kw for kw in kws if kw.isdigit()]
    if numeric:
        missing = [kw for kw in numeric if kw not in source_text]
        if missing:
            return "DROP", f"数值关键词在 source 找不到: {missing}"

    return "EXPLICIT", "数值关键词全找到 或 非数值约束"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("contract_path")
    ap.add_argument("--doc-bundle", help="本地文档 bundle 目录（优先于网络 fetch）")
    args = ap.parse_args()

    path = Path(args.contract_path)
    if not path.exists():
        print(f"FATAL: {path} not found", file=sys.stderr)
        return 2
    doc_bundle = Path(args.doc_bundle) if args.doc_bundle else None

    contract = json.loads(path.read_text(encoding="utf-8"))

    failures: list[dict] = []
    classifications: list[dict] = []

    # Check 1: schema
    schema_failures = check_schema(contract)
    failures.extend(schema_failures)

    # Check 2: CRUD coverage
    crud = check_crud_coverage(contract)
    if not crud["pass"]:
        failures.append({
            "check": "low_crud_coverage",
            "detail": f"CRUD coverage {crud['coverage']:.0%} < {MIN_CRUD_COVERAGE:.0%}; missing={crud['missing']}",
        })

    # Check 3+4: per-constraint source verify + fabricated lower bound
    constraints_root = contract.get("constraints", {}) or {}
    drop_count = 0
    total = 0
    for gname in ["type_constraints", "range_constraints", "state_constraints"]:
        for c in constraints_root.get(gname, []) or []:
            total += 1
            src_url = c.get("source_url", "") or ""
            source_text = get_source_text(src_url, doc_bundle) if src_url else None
            label, reason = classify_constraint(c, source_text, doc_bundle)
            classifications.append({
                "constraint_id": c.get("constraint_id") or c.get("assertion_id", ""),
                "group": gname,
                "label": label,
                "reason": reason,
                "source_url": src_url,
            })
            if label == "DROP":
                drop_count += 1
                failures.append({
                    "check": "constraint_hallucination",
                    "constraint_id": c.get("constraint_id", ""),
                    "detail": reason,
                })

    # Check 5: DROP 比例
    drop_ratio = drop_count / total if total else 1.0
    if drop_ratio > DROP_REJECT_RATIO:
        failures.append({
            "check": "high_drop_ratio",
            "detail": f"DROP 比例 {drop_ratio:.0%} > {DROP_REJECT_RATIO:.0%} 阈值 ({drop_count}/{total})",
        })

    verdict = "PASS" if not failures else "FAIL"
    report = {
        "validated_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "target": contract.get("target", ""),
        "version": contract.get("version", ""),
        "schema_check": {"failures": schema_failures},
        "crud_coverage": crud,
        "constraint_classifications": classifications,
        "drop_ratio": drop_ratio,
        "drop_count": drop_count,
        "total_constraints": total,
        "total_failures": len(failures),
        "failures": failures,
        "verdict": verdict,
    }

    out = path.parent / "contract_validation_report.json"
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")

    print("=== Contract Validation ===")
    print(f"Target: {report['target']} {report['version']}")
    print(f"CRUD coverage: {crud['coverage']:.0%} (found {crud['found']}, missing {crud['missing']})")
    print(f"Constraints: {total} (DROP={drop_count} [{drop_ratio:.0%}], EXPLICIT={sum(1 for c in classifications if c['label']=='EXPLICIT')})")
    print(f"Total failures: {len(failures)}")
    if failures:
        print("\n=== Failures (top 10) ===")
        for f in failures[:10]:
            cid = f.get('constraint_id', '')
            print(f"  ⚠️  [{f['check']}] {cid}: {f['detail'][:100]}")
    print(f"\nverdict: {verdict}")
    print(f"report: {out}")

    return 0 if not failures else 1


if __name__ == "__main__":
    sys.exit(main())
