#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""_validate_bug_shapes.py — 确定性核验 bug_shapes.json 反空壳 + 反 repro 泄漏。

LLM 即使在 v2.3 prompt 强制后仍产空壳（chroma 44 个 evidence 全空，
摘要谎称含 #6664）。本脚本作为流水线 stage 强制核验，反幻觉。

Checks:
1. abstract_pattern 非空 + 字符数 ≥ 30
2. abstract_pattern 不含具体值（regex param=value，反 repro 泄漏）
3. known_instances 非空 + 每条有 issue_number
4. symptom_pattern / attack_strategy_hints 非空
5. shape_type ∈ 6 类 minimal taxonomy
6. source_issues_count ≥ 3

Usage:
    python scripts/_validate_bug_shapes.py intelligence/{target}/bug_shapes.json
Exit:
    0 = pass, 1 = fail (bug_shapes_validation_report.json 写同目录), 2 = usage/error
"""
from __future__ import annotations

import datetime
import json
import re
import sys
from pathlib import Path

# Windows cp1252 stdout 兼容（attack-boundary 模板同款）
if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

VALID_SHAPE_TYPES = {
    "numeric_boundary",
    "type_confusion",
    "null_handling",
    "resource_limit",
    "concurrency_race",
    "semantic_drift",
}

# 检测 param=value 模式（snake_case 参数名 = 数字），如 shard_number=0 / nprobe=0 / ef=128
# 抽象 pattern 不应含具体值；含 = 数字 = repro 泄漏（attack agent 会照抄）
REPRO_LEAK_RE = re.compile(r"\b[a-z][a-z0-9_]*\s*=\s*\[?[-]?\d")

MIN_ABSTRACT_LEN = 30
MIN_SOURCE_ISSUES = 3


def check_shape(shape: dict) -> list[dict]:
    """返回该 shape 的所有 failure（空列表 = pass）。"""
    failures: list[dict] = []
    sid = shape.get("shape_id", "<unknown>")

    # Check 1: abstract_pattern 非空 + 长度
    abstract = shape.get("abstract_pattern", "") or ""
    if len(abstract) < MIN_ABSTRACT_LEN:
        failures.append({
            "shape_id": sid,
            "check": "empty_abstract",
            "detail": f"abstract_pattern len={len(abstract)} < {MIN_ABSTRACT_LEN}",
        })

    # Check 2: repro 泄漏
    if REPRO_LEAK_RE.search(abstract):
        failures.append({
            "shape_id": sid,
            "check": "repro_leak",
            "detail": f"abstract_pattern 含 param=value: {REPRO_LEAK_RE.search(abstract).group()}",
        })

    # Check 3: known_instances 非空 + issue_number
    known = shape.get("known_instances", []) or []
    if not known:
        failures.append({
            "shape_id": sid,
            "check": "empty_known_instances",
            "detail": "known_instances 为空（无法做 regression 验证 + novelty 判定）",
        })
    else:
        for i, inst in enumerate(known):
            if not inst.get("issue_number"):
                failures.append({
                    "shape_id": sid,
                    "check": "missing_issue_number",
                    "detail": f"known_instances[{i}] 缺 issue_number",
                })

    # Check 4: symptom_pattern + attack_strategy_hints 非空
    if not (shape.get("symptom_pattern") or "").strip():
        failures.append({
            "shape_id": sid,
            "check": "empty_symptom_pattern",
            "detail": "symptom_pattern 为空（v2.3 前 实测缺失，现强制）",
        })
    hints = shape.get("attack_strategy_hints", []) or []
    if not hints:
        failures.append({
            "shape_id": sid,
            "check": "empty_attack_hints",
            "detail": "attack_strategy_hints 为空",
        })

    # Check 5: shape_type taxonomy
    st = shape.get("shape_type")
    if st not in VALID_SHAPE_TYPES:
        failures.append({
            "shape_id": sid,
            "check": "invalid_shape_type",
            "detail": f"shape_type={st} 不在 6 类 taxonomy {sorted(VALID_SHAPE_TYPES)}",
        })

    # Check 6: source_issues_count
    if shape.get("source_issues_count", 0) < MIN_SOURCE_ISSUES:
        failures.append({
            "shape_id": sid,
            "check": "insufficient_evidence",
            "detail": f"source_issues_count={shape.get('source_issues_count', 0)} < {MIN_SOURCE_ISSUES}",
        })

    return failures


def main() -> int:
    if len(sys.argv) != 2:
        print("Usage: _validate_bug_shapes.py <bug_shapes.json>", file=sys.stderr)
        return 2
    path = Path(sys.argv[1])
    if not path.exists():
        print(f"FATAL: {path} not found", file=sys.stderr)
        return 2

    data = json.loads(path.read_text(encoding="utf-8"))
    shapes = data.get("bug_shapes", []) if isinstance(data, dict) else []

    all_failures: list[dict] = []
    for shape in shapes:
        all_failures.extend(check_shape(shape))

    verdict = "PASS" if not all_failures else "FAIL"
    report = {
        "validated_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "total_shapes": len(shapes),
        "total_failures": len(all_failures),
        "failures": all_failures,
        "verdict": verdict,
    }

    out = path.parent / "bug_shapes_validation_report.json"
    out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")

    print("=== Bug Shapes Validation ===")
    print(f"Total shapes: {report['total_shapes']}")
    print(f"Total failures: {report['total_failures']}")
    if all_failures:
        print("\n=== Failures ===")
        for f in all_failures:
            print(f"  ⚠️  {f['shape_id']} [{f['check']}]: {f['detail']}")
    print(f"\nverdict: {verdict}")
    print(f"report: {out}")

    return 0 if not all_failures else 1


if __name__ == "__main__":
    sys.exit(main())
