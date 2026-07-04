#!/usr/bin/env python3
"""Cross-round defect deduplication.

Usage:
  python scripts/dedup_defects.py <session_dir>
"""
import json
import os
import sys
from datetime import datetime, timezone

from _pipeline_utils import read_json, write_json, debate_log_path, extract_confirmed


def dedup_defects(session_dir: str) -> dict:
    stage2_agg_path = debate_log_path(session_dir, "stage2_aggregation")
    current = read_json(str(stage2_agg_path))
    if current is None:
        return {
            "before_count": 0,
            "after_count": 0,
            "deduped": [],
            "error": "stage2_aggregation.json not found",
        }

    history_file = os.path.join(os.path.dirname(session_dir), "dedup_state.json")
    history = []
    if os.path.exists(history_file):
        with open(history_file, encoding="utf-8") as f:
            history = json.load(f).get("confirmed", [])

    seen = set()
    deduped = []
    defects = extract_confirmed(current)

    for d in defects:
        key = d.get("defect_id", "")
        if not key:
            continue
        if key in seen:
            continue

        is_dup = any(h.get("defect_id", "") == key for h in history)

        if not is_dup:
            seen.add(key)
            deduped.append(d)

    output_path = debate_log_path(session_dir, "stage2_deduped")
    output = {
        "defects": deduped,
        "deduped_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    write_json(str(output_path), output)

    if deduped:
        existing_ids = {h.get("defect_id", "") for h in history}
        updated_history = {
            "confirmed": history + [
                {"defect_id": d.get("defect_id", "")}
                for d in deduped
                if d.get("defect_id", "") not in existing_ids
            ],
            "updated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        }
        try:
            write_json(history_file, updated_history)
        except OSError:
            pass

    return {
        "before_count": len(defects),
        "after_count": len(deduped),
        "deduped": [d.get("defect_id", "") for d in deduped],
    }


def _self_check() -> None:
    """Test schema compatibility and cross-round deduplication."""
    import tempfile
    from pathlib import Path

    failures = []

    def expect(cond, msg):
        failures.append(msg) if not cond else None

    # 1. extract_confirmed schema 兼容
    expect(extract_confirmed({"confirmed": {"d1": {"defect_id": "d1"}, "d2": {"defect_id": "d2"}}}) ==
           [{"defect_id": "d1"}, {"defect_id": "d2"}], "dict schema → list of entries")
    expect(extract_confirmed({"confirmed_defects": [{"defect_id": "x"}]}) ==
           [{"defect_id": "x"}], "legacy list schema passthrough")
    expect(extract_confirmed({"defects": [{"defect_id": "y"}]}) == [], "BUG 回归：`defects` key 不是合法 schema → 空")
    expect(extract_confirmed({}) == [], "空 aggregation → 空 list")
    expect(extract_confirmed(None) == [], "None → 空 list")

    with tempfile.TemporaryDirectory() as root:
        td = Path(root) / "session"
        td.mkdir()
        bdir = td / "debate_logs"
        bdir.mkdir()
        (bdir / "stage2_aggregation.json").write_text(json.dumps({
            "confirmed": {
                "qdrant_boundary_01_x": {"defect_id": "qdrant_boundary_01_x", "severity_level": "high"},
                "qdrant_boundary_02_y": {"defect_id": "qdrant_boundary_02_y", "severity_level": "medium"},
            }}), encoding="utf-8")
        r = dedup_defects(str(td))
        expect(r["before_count"] == 2, f"代码版 schema → before=2（原 bug 得 0），实际 {r['before_count']}")
        expect(r["after_count"] == 2, "无历史 → after=2（全保留）")
        expect(len(r["deduped"]) == 2, "deduped 列表含 2 个 defect_id")

        # 3. 跨轮 dedup：历史含同一 defect_id → 被 dedup 掉
        (td.parent / "dedup_state.json").write_text(json.dumps({
            "confirmed": [{"defect_id": "qdrant_boundary_01_x"}]}), encoding="utf-8")
        r2 = dedup_defects(str(td))
        expect(r2["before_count"] == 2, "before 仍 2")
        expect(r2["after_count"] == 1, f"历史含 d1 → after=1（dedup 掉历史重复），实际 {r2['after_count']}")
        expect(r2["deduped"] == ["qdrant_boundary_02_y"], "deduped 只含新 defect_id")

        # 4. aggregation 缺失 → error
        (bdir / "stage2_aggregation.json").unlink()
        r3 = dedup_defects(str(td))
        expect("error" in r3, "aggregation 缺失 → error 字段")

    if failures:
        print("self-check FAIL:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        sys.exit(1)
    print("self-check OK")


def main():
    args = sys.argv[1:]
    if args and args[0] in ("--self-check", "-s"):
        _self_check()
        return
    if len(sys.argv) < 2:
        print("Usage: python scripts/dedup_defects.py <session_dir>", file=sys.stderr)
        sys.exit(1)

    session_dir = sys.argv[1]
    if not os.path.isdir(session_dir):
        print(f"ERROR: {session_dir} not found", file=sys.stderr)
        sys.exit(2)

    result = dedup_defects(session_dir)
    print(
        f"Before dedup: {result['before_count']}, After: {result['after_count']}"
    )
    print(json.dumps(result, indent=2, ensure_ascii=False))
    sys.exit(0)


if __name__ == "__main__":
    main()
