#!/usr/bin/env python3
"""
策略注入器 — 读取 Strategy Registry 并输出适合 Agent prompt 注入的策略文本。

用法:
  python scripts/strategy_injector.py <target_db> [--max N] [--min-confidence C]

输入:
  target_db: milvus/qdrant/weaviate/pgvector
  --max N: 最多注入 N 条策略（默认 10）
  --min-confidence C: 最低 confidence 阈值（默认 0.6）

输出 (stdout):
  JSON {strategies: [...], injection_text: "..."}

策略注入时附带 confidence，Agent 应对低 confidence 策略降低依赖。
status=deprecated 的策略不注入。
"""

import json
import os
import sys

from _pipeline_utils import setup_encoding


PROJECT_ROOT = os.environ.get("PROJECT_ROOT", os.getcwd())
REGISTRY_DIR = os.path.join(PROJECT_ROOT, "strategy_registry")


def load_json(path: str) -> dict:
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {"strategies": []}


def get_strategies(target_db: str, max_count: int = 10, min_confidence: float = 0.6) -> list:
    """获取适用于目标 DB 的策略列表"""
    global_path = os.path.join(REGISTRY_DIR, "global_strategies.json")
    db_path = os.path.join(REGISTRY_DIR, f"{target_db}_strategies.json")

    global_reg = load_json(global_path)
    db_reg = load_json(db_path)

    all_strategies = {}
    for s in global_reg.get("strategies", []):
        all_strategies[s["strategy_id"]] = s
    for s in db_reg.get("strategies", []):
        all_strategies[s["strategy_id"]] = s

    candidates = []
    for sid, s in all_strategies.items():
        if s.get("status") == "deprecated":
            continue
        if target_db in s.get("migration", {}).get("rejected_dbs", []):
            continue
        conf = s.get("performance", {}).get("avg_confidence", 0.0)
        if conf < min_confidence:
            continue
        applicable = s.get("migration", {}).get("applicable_dbs", [])
        if applicable and target_db not in applicable and "all" not in applicable:
            continue

        candidates.append(s)

    candidates.sort(key=lambda s: s.get("performance", {}).get("avg_confidence", 0), reverse=True)

    return candidates[:max_count]


def generate_injection_text(strategies: list, target_db: str) -> str:
    """生成注入到 Attack Agent prompt 的策略文本"""
    if not strategies:
        return "（无跨会话策略可用）"

    lines = [
        "## 跨会话策略注入",
        "",
        "以下策略来自之前成功挖掘的经验（跨 DB 迁移）。使用这些策略作为初始 seed。",
        "",
    ]

    for i, s in enumerate(strategies, 1):
        pattern = s.get("pattern", {})
        migration = s.get("migration", {})
        perf = s.get("performance", {})

        migration_rule = migration.get("migration_rules", {}).get(target_db, "no specific rule")

        lines.append(f"### 策略 {i}: {pattern.get('name', s['strategy_id'])}")
        lines.append(f"- **模板**: {pattern.get('template', 'N/A')}")
        lines.append(f"- **类别**: {s.get('category', 'unknown')}")
        lines.append(f"- **置信度**: {perf.get('avg_confidence', 0):.2f}")
        lines.append(f"- **适用端点**: {', '.join(pattern.get('applicable_endpoints', []))}")
        lines.append(f"- **DB 适配**: {migration_rule}")
        lines.append(f"- **约束类型**: {', '.join(pattern.get('constraint_types', []))}")
        lines.append(f"- **来源**: {s.get('origin', {}).get('db', 'unknown')} v{s.get('origin', {}).get('version', '?')}")

        desc = pattern.get('description', '')
        if desc:
            lines.append(f"- **描述**: {desc}")

        lines.append("")

    lines.append("注意：对低置信度策略降低依赖优先级，优先使用高置信度策略。")
    return "\n".join(lines)


def main():
    setup_encoding()

    import argparse

    parser = argparse.ArgumentParser(description="注入跨会话策略到 Attack Agent prompt")
    parser.add_argument("target_db", help="目标数据库 (milvus/qdrant/weaviate/pgvector)")
    parser.add_argument("--max", type=int, default=10, dest="max_count")
    parser.add_argument("--min-confidence", type=float, default=0.6)
    parser.add_argument("--text-only", action="store_true", help="仅输出注入文本，不输出 JSON")

    args = parser.parse_args()

    strategies = get_strategies(args.target_db, args.max_count, args.min_confidence)
    injection_text = generate_injection_text(strategies, args.target_db)

    if args.text_only:
        print(injection_text)
    else:
        result = {
            "strategies": [{"strategy_id": s["strategy_id"],
                           "confidence": s.get("performance", {}).get("avg_confidence", 0),
                           "category": s.get("category")}
                          for s in strategies],
            "count": len(strategies),
            "target_db": args.target_db,
            "injection_text": injection_text
        }
        print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
