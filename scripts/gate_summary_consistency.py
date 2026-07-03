#!/usr/bin/env python3
"""gate_summary_consistency — summary.md 声称数必须 == defects/*.md 实际文件数（设计 §3.1, §8）。

chroma v1.5.9 触发案例：summary 声称 "Defects Reported: 5"，defects/ 实际只有 1 个文件
（reporter 未落地或被删）→ fail。

契约（设计 §3.3）：
  输入: <session_dir> [--target T] [--strict]
  输出: JSON {status, reason, details} + exit(0=pass / 1=fail，strict=false 时 fail 也 exit 0)
  落盘: {session_dir}/gate_logs/gate_summary_consistency.json
"""
from __future__ import annotations

import glob
import json
import os
import re
import sys
from pathlib import Path

from _pipeline_utils import setup_encoding

setup_encoding()

# ponytail: claimed_count 取关键词行的**最后一个**数字（避免中间数字误匹配）。
# 旧 regex pattern 陷阱：`| Defects Confirmed (Debate Stage 2) | 8 |` 的 "Stage 2" 被
# `[^\d\n|]*\|?\s*(\d+)` 捕获为 count=2（错误，应 8）。根因：pattern 允许数字前有非数字文本，
# 贪婪匹配到第一个数字。修复：按行扫描关键词，取该行最后一个数字（表格 cell 的 count）。
_CLAIM_KEYWORDS = ("defects reported", "defects confirmed", "confirmed defects")


def claimed_count(summary_text: str) -> int | None:
    """从 summary.md 提取声称的 confirmed defect 数。

    取关键词行的最后一个数字，避免 "(Debate Stage 2)" 等中间数字误匹配。
    """
    for kw in _CLAIM_KEYWORDS:
        for line in summary_text.split("\n"):
            if kw in line.lower():
                nums = re.findall(r"\d+", line)
                if nums:
                    return int(nums[-1])  # 最后一个数字（表格 cell 的 count）
    return None


def actual_count(session_dir: str | Path) -> int:
    """数 defects/defect-*.md（排除 -enhanced 重复）。"""
    files = glob.glob(str(Path(session_dir) / "defects" / "defect-*.md"))
    return sum(1 for f in files if "-enhanced" not in os.path.basename(f))


def run(session_dir: str, target: str = "", strict: bool = False) -> dict:
    summary_path = Path(session_dir) / "summary.md"
    summary_text = summary_path.read_text(encoding="utf-8", errors="replace") if summary_path.exists() else ""
    claimed = claimed_count(summary_text)
    actual = actual_count(session_dir)

    if claimed is None:
        result = {"status": "fail",
                  "reason": "无法从 summary.md 提取 confirmed defect 数",
                  "details": {"claimed": None, "actual": actual}}
    elif claimed != actual:
        result = {"status": "fail",
                  "reason": f"summary 声称 {claimed}，defects/ 实际 {actual} 个文件",
                  "details": {"claimed": claimed, "actual": actual}}
    else:
        result = {"status": "pass",
                  "reason": f"summary ({claimed}) == defects/ ({actual})",
                  "details": {"claimed": claimed, "actual": actual}}

    # 落盘 gate_logs/
    log_dir = Path(session_dir) / "gate_logs"
    log_dir.mkdir(exist_ok=True)
    log_path = log_dir / "gate_summary_consistency.json"
    payload = {**result, "target": target, "strict": strict}
    log_path.write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    return payload


def _self_check() -> None:
    """ponytail: ONE runnable check — claim/actual mismatch → fail，一致 → pass。"""
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        # mismatch: 5 claimed, 0 actual
        (Path(td) / "summary.md").write_text("# S\n| Defects Reported | 5 |\n", encoding="utf-8")
        r = run(td)
        assert r["status"] == "fail" and r["details"]["claimed"] == 5, "5/0 must fail"
        # 一致: 5 claimed, 5 actual
        os.makedirs(Path(td) / "defects")
        for i in range(5):
            (Path(td) / "defects" / f"defect-{i+1}.md").write_text("x", encoding="utf-8")
        r = run(td)
        assert r["status"] == "pass", "5/5 must pass"
        # Stage 2 陷阱：中间数字 "Stage 2" 不应被捕获为 count（实战 bug 2026-07-03）
        (Path(td) / "summary.md").write_text(
            "# S\n| Defects Confirmed (Debate Stage 2) | 8 |\n", encoding="utf-8")
        os.makedirs(Path(td) / "defects_2", exist_ok=True)  # actual_count 仍读 defects/
        # 清空 defects/ 重置 actual=0，验证 claimed 取最后数字（8）而非 "Stage 2" 的 2
        for f in glob.glob(str(Path(td) / "defects" / "defect-*.md")):
            os.remove(f)
        r = run(td)
        assert r["details"]["claimed"] == 8, \
            f"Stage 2 trap: claimed must be 8 (last number), got {r['details']['claimed']}"
    print("self-check OK")


def main() -> None:
    args = sys.argv[1:]
    if not args or args[0] in ("--self-check", "-s"):
        _self_check()
        return
    session_dir = args[0]
    target = ""
    strict = False
    if "--strict" in args:
        strict = True
    if "--target" in args:
        idx = args.index("--target")
        if idx + 1 < len(args):
            target = args[idx + 1]
    if not os.path.isdir(session_dir):
        print(json.dumps({"status": "fail", "reason": f"session_dir not found: {session_dir}"}, ensure_ascii=False))
        sys.exit(1)
    r = run(session_dir, target, strict)
    print(json.dumps(r, ensure_ascii=False, indent=2))
    # strict=false 时 advisory（fail 也 exit 0）；strict=true 时 fail exit 1
    sys.exit(0 if (r["status"] == "pass" or not strict) else 1)


if __name__ == "__main__":
    main()
