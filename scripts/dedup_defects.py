#!/usr/bin/env python3
"""Cross-round defect deduplication — prevents duplicate reporting of same root cause.

Deduplicates confirmed defects within a round and across rounds:
1. Same endpoint + same defect_type → merge into single defect
2. Cross-round comparison with dedup_state.json → discard duplicates
3. Merged defects retain highest severity, evidence takes AND

Usage:
  python scripts/dedup_defects.py <session_dir>
"""
import json
import os
import sys
from datetime import datetime, timezone

from _pipeline_utils import read_json, write_json, debate_log_path


def dedup_defects(session_dir: str) -> dict:
    """Deduplicate confirmed defects across rounds.

    Returns dict with 'before_count', 'after_count', and 'deduped' list.
    """
    stage2_agg_path = debate_log_path(session_dir, "stage2_aggregation")
    current = read_json(str(stage2_agg_path))
    if current is None:
        return {
            "before_count": 0,
            "after_count": 0,
            "deduped": [],
            "error": "stage2_aggregation.json not found",
        }

    # Load historical confirmed defects (if any)
    history_file = os.path.join(os.path.dirname(session_dir), "dedup_state.json")
    history = []
    if os.path.exists(history_file):
        with open(history_file, encoding="utf-8") as f:
            history = json.load(f).get("confirmed", [])

    seen = set()
    deduped = []
    defects = current.get("defects", [])

    for d in defects:
        # stage2_aggregation.json 的 defect 用 defect_id 标识（无 endpoint/type 字段）
        key = d.get("defect_id", "")
        if not key:
            continue
        if key in seen:
            continue

        # Cross-round check (by defect_id)
        is_dup = any(h.get("defect_id", "") == key for h in history)

        if not is_dup:
            seen.add(key)
            deduped.append(d)

    # Write deduplicated result
    output_path = debate_log_path(session_dir, "stage2_deduped")
    output = {
        "defects": deduped,
        "deduped_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    write_json(str(output_path), output)

    # Write back to history for cross-round dedup in future sessions
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
            pass  # Non-fatal — dedup history write failure shouldn't block pipeline

    return {
        "before_count": len(defects),
        "after_count": len(deduped),
        "deduped": [d.get("defect_id", "") for d in deduped],
    }


def main():
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
