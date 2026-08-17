#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""extract_candidates.py — ADR-0008 候选清单机械提取（evidence-builder fan-out 的派发清单）。

背景：ADR-0008 双 agent 架构（evidence-builder 按候选并发派发 + chain-auditor 收口）需要
一份确定性的候选清单才能 fan-out。旧流程候选隐式来自 judge 读日志；本脚本取代该隐式路径。

规则（确定性，0 LLM）：
  1. 扫 SESSION_DIR/output_*.log，行含 `VERDICT: DEFECT_FOUND` → 该脚本产出一个候选
  2. 排除 `VERDICT: SCRIPT_ERROR` 的 log（脚本自身错误，非 DB 行为证据）
  3. defect_id = log 文件名去 `output_` 前缀去 `.log` 后缀（与旧 stage2 命名一致）
  4. constraint_id / strategy 从脚本 docstring 头部行提取（`Constraint: xxx` / `Attack: xxx`）；
     脚本文件按 stage1 归档惯例在 debate_logs/{script_id}.py，找不到则字段为空字符串
  5. VERDICT 行原文截断 200 字符存 claim_hint（builder 链追溯的起点提示）

输出：SESSION_DIR/candidates.jsonl（每行一个候选 JSON）+ stdout 汇总 JSON。
Exit: 0 = 有候选或无候选均正常（机制非 gate），2 = usage 错误。

Consumers: orchestrator Step 8e（EVIDENCE_BUILD 前）按此 fan-out evidence-builder；
           verify_live_l1.py 的 L1 前移位也读此清单（B1 拍板）。
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

VERDICT_DEFECT_RE = re.compile(r"VERDICT:\s*DEFECT_FOUND")
VERDICT_SCRIPT_ERROR_RE = re.compile(r"VERDICT:\s*SCRIPT_ERROR")
CONSTRAINT_RE = re.compile(r"^Constraint:\s*(\S+)", re.MULTILINE)
ATTACK_RE = re.compile(r"^Attack:\s*(.+)$", re.MULTILINE)
CLAIM_MAX_LEN = 200


def _script_field(script_path: Path) -> tuple[str, str]:
    """从脚本 docstring 头提取 (constraint_id, attack_strategy)。读失败 → ("", "")."""
    try:
        head = script_path.read_text(encoding="utf-8", errors="replace")[:2000]
    except OSError:
        return "", ""
    m = CONSTRAINT_RE.search(head)
    constraint_id = m.group(1) if m else ""
    a = ATTACK_RE.search(head)
    attack = a.group(1).strip() if a else ""
    return constraint_id, attack


def extract_candidates(session_dir: Path) -> list[dict]:
    candidates = []
    for log_path in sorted(session_dir.glob("output_*.log")):
        try:
            text = log_path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        if not VERDICT_DEFECT_RE.search(text):
            continue
        if VERDICT_SCRIPT_ERROR_RE.search(text):
            continue  # 脚本自错 → 非候选（_classify_script_errors 的 retry 子循环管辖）
        script_id = log_path.name[len("output_"):-len(".log")]
        script_path = session_dir / "debate_logs" / f"{script_id}.py"
        constraint_id, attack = ("", "")
        if script_path.exists():
            constraint_id, attack = _script_field(script_path)
        claim_line = next(
            (ln.strip() for ln in text.splitlines() if VERDICT_DEFECT_RE.search(ln)),
            "",
        )
        if len(claim_line) > CLAIM_MAX_LEN:
            claim_line = claim_line[:CLAIM_MAX_LEN] + "…"
        candidates.append({
            "defect_id": script_id,
            "script": script_id,
            "log_path": log_path.name,
            "constraint_id": constraint_id,
            "attack": attack,
            "claim_hint": claim_line,
        })
    return candidates


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: extract_candidates.py SESSION_DIR", file=sys.stderr)
        return 2
    sd = Path(sys.argv[1])
    if not sd.is_dir():
        print(f"ERROR: not a directory: {sd}", file=sys.stderr)
        return 2

    candidates = extract_candidates(sd)
    out = sd / "candidates.jsonl"
    with open(out, "w", encoding="utf-8") as f:
        for c in candidates:
            f.write(json.dumps(c, ensure_ascii=False) + "\n")

    with_constraint = sum(1 for c in candidates if c["constraint_id"])
    print(json.dumps({
        "total": len(candidates),
        "with_constraint_id": with_constraint,
        "output": str(out),
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
