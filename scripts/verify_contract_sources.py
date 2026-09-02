#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""verify_contract_sources.py — 批量核对 structured_contract.json 的 source_url 是否真包含对应 constraint 文本。

反幻觉工具：contract-formalizer agent 可能编造 constraint 且 source_url 指向真实文件但文件不含对应内容。
本脚本通过 GitHub raw URL 获取 source 内容，对每个 constraint 的关键词做子串匹配，标记 source_verified。

Usage:
    python scripts/verify_contract_sources.py results/{target}/{version}/structured_contract.json [--fix]
"""
from __future__ import annotations
import sys, json, re, argparse, urllib.request, urllib.error
from pathlib import Path

def github_raw_url(source_url: str) -> str | None:
    """github.com/owner/repo/blob/branch/path → raw.githubusercontent.com/owner/repo/branch/path"""
    m = re.match(r"https?://github\.com/([^/]+)/([^/]+)/blob/([^/]+)/(.+)", source_url)
    if not m:
        return None
    owner, repo, branch, path = m.groups()
    return f"https://raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}"

def fetch_text(url: str, timeout: int = 20) -> str | None:
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "testvdb-source-verify"})
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            return resp.read().decode("utf-8", "replace")
    except Exception as e:
        return None

def extract_keywords(constraint: dict) -> list[str]:
    """从 constraint 提取用于核对的关键词（数值、字段名、关键短语）。"""
    kws = []
    a = constraint.get("assertion", "")
    d = constraint.get("description", "")
    cid = constraint.get("constraint_id", "")
    # 数值（如 16384, 100, 32768）
    for m in re.finditer(r"\b\d{2,}\b", a + " " + d):
        kws.append(m.group())
    # constraint_id 末段（如 entities_search_001 -> entities/search）
    parts = cid.split("_")
    if len(parts) >= 3:
        kws.append(parts[-3])  # e.g. "search"
    # description 关键短语（首词组）
    if d:
        # 取前 3 个英文词
        words = re.findall(r"[a-zA-Z_]+", d)[:3]
        if len(words) >= 2:
            kws.append(" ".join(words[:2]))
    return list(dict.fromkeys(kws))  # dedup preserve order

def verify_constraint(constraint: dict, source_text: str | None,
                      allow_partial_numeric: bool = False) -> dict:
    if source_text is None:
        return {"verified": False, "reason": "source_unreachable"}
    kws = extract_keywords(constraint)
    if not kws:
        return {"verified": None, "reason": "no_keywords_to_check"}
    missing = [kw for kw in kws if kw.lower() not in source_text.lower()]
    # 数值类必须全找到；短语允许部分
    numeric = [kw for kw in kws if kw.isdigit()]
    numeric_missing = [kw for kw in numeric if kw not in source_text]
    if numeric_missing and not allow_partial_numeric:
        return {"verified": False, "reason": f"numeric_keywords_missing: {numeric_missing}", "keywords_checked": kws}
    # doc_consistency 双源模式（2026-09-02，与 _validate_contract.py classify 对齐）：
    # 断言是 "spec says X / prose says Y" 双源冲突，两侧数值分属两个 source；
    # 任一冲突侧在 source 中找到即 verified，全部缺失才算 hallucination 嫌疑
    if allow_partial_numeric and len(numeric_missing) == len(numeric):
        return {"verified": False, "reason": f"all_numeric_keywords_missing: {numeric}",
                "keywords_checked": kws}
    return {"verified": True, "reason": "all_numeric_keywords_found", "keywords_checked": kws}

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("contract_path")
    ap.add_argument("--fix", action="store_true", help="写回 source_verified 字段到 contract")
    args = ap.parse_args()

    contract = json.loads(Path(args.contract_path).read_text(encoding="utf-8"))
    constraints_root = contract.get("constraints", {})
    report = {"verified": [], "unverified": [], "unreachable": [], "summary": {}}
    cache = {}  # source_url -> text

    def check_group(group_name: str, items: list):
        # doc_consistency 双源模式（任一冲突侧命中即 verified，防误报 hallucination）
        partial = group_name == "doc_consistency_constraints"
        for c in items:
            cid = c.get("constraint_id") or c.get("assertion_id")
            src_url = c.get("source_url", "")
            if src_url not in cache:
                raw = github_raw_url(src_url) if "github.com" in src_url else src_url
                cache[src_url] = fetch_text(raw) if raw else None
            text = cache[src_url]
            result = verify_constraint(c, text, allow_partial_numeric=partial)
            entry = {"id": cid, "endpoint": c.get("endpoint",""), "source_url": src_url, **result}
            if result["verified"] is True:
                report["verified"].append(entry)
            elif result["verified"] is False and "unreachable" in result["reason"]:
                report["unreachable"].append(entry)
            else:
                report["unverified"].append(entry)
            if args.fix:
                c["source_verified"] = bool(result["verified"])

    # v3.4 六组全覆盖（2026-09-02 补洞：此前只查 type/range/state+assertions，
    # resource_bound/doc_consistency/other 的 source 复检全部跳过——与 _validate_contract 同构空洞）
    for gname in ["type_constraints", "range_constraints", "state_constraints",
                  "resource_bound_constraints", "doc_consistency_constraints",
                  "other_constraints"]:
        check_group(gname, constraints_root.get(gname, []))
    if "assertions" in contract:
        check_group("assertions", contract["assertions"])

    report["summary"] = {
        "total": len(report["verified"]) + len(report["unverified"]) + len(report["unreachable"]),
        "verified_true": len(report["verified"]),
        "verified_false_or_missing": len(report["unverified"]),
        "source_unreachable": len(report["unreachable"]),
    }

    print(f"=== Source Verification Report ===")
    print(f"Total constraints/assertions: {report['summary']['total']}")
    print(f"  source_verified=true: {report['summary']['verified_true']}")
    print(f"  source_verified=false/missing (hallucination 嫌疑): {report['summary']['verified_false_or_missing']}")
    print(f"  source unreachable: {report['summary']['source_unreachable']}")
    print(f"\n=== Hallucination 嫌疑清单（numeric 关键词在 source 中找不到）===")
    for e in report["unverified"]:
        if "missing" in e.get("reason", ""):
            print(f"  ⚠️  {e['id']} ({e['endpoint']}): {e['reason']}")
            print(f"     source: {e['source_url']}")

    out_path = Path(args.contract_path).parent / "source_verification_report.json"
    out_path.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"\nreport written: {out_path}")

    if args.fix:
        Path(args.contract_path).write_text(json.dumps(contract, ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"contract updated with source_verified fields: {args.contract_path}")

if __name__ == "__main__":
    main()
