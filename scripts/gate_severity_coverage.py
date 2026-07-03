#!/usr/bin/env python3
"""gate_severity_coverage — judge-severity 必须为所有 confirmed defect 产出 severity 投票。

chroma v1.5.9 触发案例：stage2_severity.json 是空 {}（judge-severity 整体失败），
aggregation 里 severity 反对权全丢 → 5 个"异常类型不匹配"假阳性流入 confirmed。

设计 §1.2 诊断的"severity 被多数决淹没"的极端形态：severity 根本没投票。
本 gate 把"severity 反对权"从 prompt（LLM 可跳过）变成 mechanism（代码强制）。

契约（设计 §3.3）：输入 <session_dir> [--target T] [--strict]；输出 JSON + exit 0/1。
"""
from __future__ import annotations

import json
import os
import sys
from pathlib import Path

from _pipeline_utils import setup_encoding, read_json, debate_log_path

setup_encoding()


def _confirmed_count(agg: dict) -> int:
    """兼容两种 aggregation schema：pgvector confirmed_defects(list) / chroma confirmed(dict)。"""
    if not isinstance(agg, dict):
        return 0
    cds = agg.get("confirmed_defects")
    if isinstance(cds, list):
        return len(cds)
    confirmed = agg.get("confirmed")
    return len(confirmed) if isinstance(confirmed, dict) else 0


def _severity_vote_count(sev: dict) -> int:
    """severity 投票条目数。兼容 {votes:[...]} / 顶层 dict / list。"""
    if not sev:
        return 0
    if isinstance(sev, list):
        return len(sev)
    votes = sev.get("votes")
    if isinstance(votes, list):
        return len(votes)
    if isinstance(sev, dict):
        # 顶层 dict：非 meta 键算 vote
        return sum(1 for k in sev if k not in ("judge", "timestamp", "target", "version", "session_dir"))
    return 0


def run(session_dir: str, target: str = "", strict: bool = False) -> dict:
    sev = read_json(debate_log_path(session_dir, "stage2_severity"))
    agg = read_json(debate_log_path(session_dir, "stage2_aggregation"))
    confirmed_n = _confirmed_count(agg or {})
    vote_n = _severity_vote_count(sev or {})

    if not sev or vote_n == 0:
        result = {"status": "fail",
                  "reason": "stage2_severity.json 为空或无 vote — judge-severity 未产出有效数据",
                  "details": {"confirmed": confirmed_n, "severity_votes": 0}}
    elif confirmed_n > 0 and vote_n < confirmed_n:
        result = {"status": "fail",
                  "reason": f"severity 覆盖不全：{confirmed_n} confirmed，仅 {vote_n} severity votes",
                  "details": {"confirmed": confirmed_n, "severity_votes": vote_n}}
    else:
        result = {"status": "pass",
                  "reason": f"severity 覆盖充分：{vote_n} votes / {confirmed_n} confirmed",
                  "details": {"confirmed": confirmed_n, "severity_votes": vote_n}}

    log_dir = Path(session_dir) / "gate_logs"
    log_dir.mkdir(exist_ok=True)
    payload = {**result, "target": target, "strict": strict}
    (log_dir / "gate_severity_coverage.json").write_text(
        json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
    return payload


def _self_check() -> None:
    """ponytail: 空 severity → fail；覆盖不全 → fail；充分 → pass。"""
    import tempfile
    with tempfile.TemporaryDirectory() as td:
        bdir = Path(td) / "debate_logs"
        bdir.mkdir()
        agg = json.dumps({"confirmed": {"a": {}, "b": {}}})

        # 1. 空 severity → fail
        (bdir / "stage2_severity.json").write_text("{}", encoding="utf-8")
        (bdir / "stage2_aggregation.json").write_text(agg, encoding="utf-8")
        r = run(td)
        assert r["status"] == "fail" and r["details"]["confirmed"] == 2, "empty severity must fail"

        # 2. 覆盖不全（1 vote / 2 confirmed）→ fail
        (bdir / "stage2_severity.json").write_text(
            json.dumps({"votes": [{"defect_id": "a"}]}), encoding="utf-8")
        r = run(td)
        assert r["status"] == "fail", "partial coverage must fail"

        # 3. 覆盖充分 → pass
        (bdir / "stage2_severity.json").write_text(
            json.dumps({"votes": [{"defect_id": "a"}, {"defect_id": "b"}]}), encoding="utf-8")
        r = run(td)
        assert r["status"] == "pass", "full coverage must pass"
    print("self-check OK")


def main() -> None:
    args = sys.argv[1:]
    if not args or args[0] in ("--self-check", "-s"):
        _self_check()
        return
    session_dir = args[0]
    target, strict = "", False
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
    sys.exit(0 if (r["status"] == "pass" or not strict) else 1)


if __name__ == "__main__":
    main()
