#!/usr/bin/env python3
"""
策略提取器 — 从 experience_handoff.json 提取可复用策略并写入 Strategy Registry。

用法:
  python scripts/strategy_extractor.py <session_dir> <target_db>

输入:
  session_dir: 会话目录（含 experience_handoff.json）
  target_db: milvus/qdrant/weaviate/pgvector

行为:
  1. 读取 experience_handoff.json 中的 confirmed_defects
  2. 提取 attack_type + constraint_type + endpoint 模式
  3. 泛化：将 DB 特定的 API 调用替换为抽象模式
  4. 交叉分析：检查相同模式是否已在其他 DB 的 registry 中存在
  5. 新策略 → 写入对应 DB 的 registry
  6. 已有策略 → 更新 performance 计数 + 调整 confidence
  7. 追加 evolution_log.jsonl 审计条目
"""

import json
import os
import sys
import re
from datetime import datetime, timezone
from pathlib import Path

from _pipeline_utils import setup_encoding


PROJECT_ROOT = os.environ.get("PROJECT_ROOT", os.getcwd())
REGISTRY_DIR = os.path.join(PROJECT_ROOT, "strategy_registry")
LOG_PATH = os.path.join(REGISTRY_DIR, "evolution_log.jsonl")

ENDPOINT_CATEGORIES = [
    (r"(?:create|insert|put|post).*collection", "schema_create"),
    (r"(?:delete|drop).*collection", "schema_delete"),
    (r"(?:get|list|describe).*collection", "schema_read"),
    (r"search.*points?", "search"),
    (r"(?:insert|upsert|put).*points?", "data_insert"),
    (r"(?:delete).*points?", "data_delete"),
    (r"(?:get|retrieve).*points?", "data_read"),
    (r"(?:update).*points?", "data_update"),
    (r"count.*points?", "data_count"),
    (r"(?:create|build).*index", "index_create"),
    (r"create.*table", "schema_create"),
]


def load_json(path: str) -> dict:
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def save_json(path: str, data: dict):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)


def append_log(entry: dict):
    """追加一条 evolution log"""
    with open(LOG_PATH, "a", encoding="utf-8") as f:
        f.write(json.dumps(entry, ensure_ascii=False) + "\n")


def classify_endpoint(endpoint: str) -> str:
    """分类端点模式"""
    for pattern, category in ENDPOINT_CATEGORIES:
        if re.search(pattern, endpoint, re.IGNORECASE):
            return category
    return "other"


def generalize_endpoint(endpoint: str, source_db: str) -> str:
    """将 DB 特定端点泛化为抽象模式。

    匹配规则（按优先级）：
    1. 精确前缀匹配（如 milvus /v2/vectordb/collections → {db}.collections.{op}）
    2. 通配符模式匹配（如 qdrant /collections/{name}/points/search）
    3. 回退：按 HTTP 方法 + 路径段数分类
    """
    # 标准化：去掉前导/后缀斜杠，小写
    ep = endpoint.strip("/").lower()

    # 提取操作类型（最后一个路径段中的动词）
    segments = ep.split("/")
    operation = segments[-1] if segments else "unknown"
    if "{" in operation or "}" in operation:
        # 最后一段是参数，取倒数第二段
        operation = segments[-2] if len(segments) >= 2 else "unknown"

    # 提取资源类型（倒数第二段或第一个有意义的段）
    resource = "unknown"
    for seg in reversed(segments[:-1]):
        if "{" not in seg and "}" not in seg and seg not in ("v1", "v2", "api", "rest"):
            resource = seg
            break

    # 泛化端点中的所有 UUID/ID 占位符
    ep_generalized = re.sub(
        r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}',
        '{id}', ep
    )
    ep_generalized = re.sub(r'/[0-9]+/', '/{id}/', ep_generalized)

    return f"{{db}}.{resource}.{operation}({ep_generalized[:60]})"


def extract_strategy_from_defect(defect: dict, session_dir: str) -> dict:
    """从单个缺陷提取策略"""
    strategy = {
        "strategy_id": "",
        "category": "",
        "origin": {
            "db": "",
            "version": "",
            "session_id": "",
            "defect_id": "",
            "created_at": datetime.now(timezone.utc).isoformat()
        },
        "pattern": {
            "name": "",
            "description": "",
            "template": "",
            "constraint_types": [],
            "applicable_endpoints": []
        },
        "migration": {
            "applicable_dbs": ["milvus", "qdrant", "weaviate", "pgvector", "meilisearch", "chroma"],
            "confirmed_dbs": [],
            "rejected_dbs": [],
            "migration_rules": {}
        },
        "performance": {
            "total_attempts": 1,
            "defects_found": 1,
            "false_positives": 0,
            "avg_confidence": 0.8,
            "last_used": datetime.now(timezone.utc).isoformat()
        },
        "status": "experimental"
    }

    endpoint = defect.get("endpoint", "")
    defect_type = defect.get("defect_type", "")
    confidence = defect.get("confidence", 0.5)
    summary = defect.get("summary", "")

    if defect_type and "Type1" in defect_type:
        strategy["category"] = "boundary"
    elif defect_type and "Type4" in defect_type:
        strategy["category"] = "state"
    else:
        strategy["category"] = "semantic"

    ep_category = classify_endpoint(endpoint)
    strategy["strategy_id"] = f"{ep_category}_{strategy['category']}_{defect_type or 'unknown'}"

    strategy["pattern"]["name"] = summary[:50] if summary else f"Attack on {endpoint}"
    strategy["pattern"]["description"] = summary
    strategy["pattern"]["template"] = f"Test {endpoint} for {defect_type or 'defect'} violation"
    strategy["pattern"]["applicable_endpoints"] = [f"*+{ep_category}", "*+create", "*+insert", "*+search"]

    strategy["performance"]["avg_confidence"] = confidence

    return strategy


def generate_strategy_id(base: str, registry: dict) -> str:
    """生成唯一 strategy_id，避免冲突"""
    existing_ids = {s["strategy_id"] for s in registry.get("strategies", [])}
    candidate = base.lower().replace(" ", "_").replace("/", "_")
    if candidate not in existing_ids:
        return candidate
    for i in range(2, 100):
        v = f"{candidate}_v{i}"
        if v not in existing_ids:
            return v
    return f"{candidate}_{int(datetime.now().timestamp())}"


def merge_strategy(new_strategy: dict, existing: dict) -> dict:
    """合并策略：更新 performance，调整 confidence"""
    perf = existing["performance"]
    perf["total_attempts"] += 1
    perf["defects_found"] += new_strategy["performance"]["defects_found"]
    new_conf = new_strategy["performance"]["avg_confidence"]
    old_conf = perf["avg_confidence"]
    perf["avg_confidence"] = round((old_conf * 0.7 + new_conf * 0.3), 2)
    perf["last_used"] = datetime.now(timezone.utc).isoformat()

    origin_db = new_strategy["origin"]["db"]
    if origin_db not in existing["migration"]["confirmed_dbs"]:
        existing["migration"]["confirmed_dbs"].append(origin_db)

    return existing


def main():
    setup_encoding()

    if len(sys.argv) < 3:
        print("Usage: python scripts/strategy_extractor.py <session_dir> <target_db>")
        sys.exit(1)

    session_dir = sys.argv[1]
    target_db = sys.argv[2].lower()

    if target_db not in ("milvus", "qdrant", "weaviate", "pgvector", "meilisearch", "chroma"):
        print(f"Error: Unknown target_db '{target_db}'")
        sys.exit(1)

    exp_path = os.path.join(session_dir, "experience_handoff.json")
    exp = load_json(exp_path)

    if not exp:
        print(json.dumps({"status": "no_data", "reason": "experience_handoff.json not found or empty"}))
        return

    key_findings = exp.get("key_findings", [])
    extracted = 0
    merged = 0

    global_path = os.path.join(REGISTRY_DIR, "global_strategies.json")
    global_reg = load_json(global_path)

    for defect in key_findings:
        strategy = extract_strategy_from_defect(defect, session_dir)
        strategy["origin"]["db"] = target_db
        strategy["origin"]["session_id"] = exp.get("session_id", "unknown")
        strategy["origin"]["version"] = exp.get("version", "unknown")
        strategy["origin"]["defect_id"] = defect.get("defect_id", "unknown")

        existing = None
        for gs in global_reg.get("strategies", []):
            if gs["strategy_id"] == strategy["strategy_id"]:
                existing = gs
                break

        if existing:
            merge_strategy(strategy, existing)
            merged += 1
        else:
            strategy["strategy_id"] = generate_strategy_id(strategy["strategy_id"], global_reg)
            strategy["migration"]["confirmed_dbs"] = [target_db]
            global_reg.setdefault("strategies", []).append(strategy)
            extracted += 1

            append_log({
                "ts": datetime.now(timezone.utc).isoformat(),
                "event": "strategy_created",
                "strategy_id": strategy["strategy_id"],
                "origin_db": target_db,
                "origin_defect": strategy["origin"]["defect_id"]
            })

        save_json(global_path, global_reg)

    db_path = os.path.join(REGISTRY_DIR, f"{target_db}_strategies.json")
    db_reg = load_json(db_path)
    for gs in global_reg.get("strategies", []):
        if target_db in gs["migration"]["applicable_dbs"]:
            exists = any(s["strategy_id"] == gs["strategy_id"]
                        for s in db_reg.get("strategies", []))
            if not exists:
                db_reg.setdefault("strategies", []).append(gs)
    save_json(db_path, db_reg)

    result = {
        "status": "ok",
        "extracted": extracted,
        "merged": merged,
        "target_db": target_db,
        "session_id": exp.get("session_id", "unknown")
    }
    print(json.dumps(result, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    main()
