"""TestVDB attack runtime — single dispatch entry.

agent 通过 get_runtime() 拿到当前 target 的 runtime 模块，不直接接触路径字符串。
"""
from __future__ import annotations

import json
import os
import sys


def _fallback_target_from_contract() -> str:
    """v3.4 bootstrap 层 2/3（X1）：env 缺失时从调用脚本位置向上找
    structured_contract.json 读 target 字段（最多 6 级，覆盖
    SESSION_DIR/debate_logs/ → results/{target}/{version}/）。
    """
    here = os.path.dirname(os.path.abspath(sys.argv[0] or os.getcwd()))
    for _ in range(6):
        p = os.path.join(here, "structured_contract.json")
        if os.path.exists(p):
            try:
                with open(p, "r", encoding="utf-8") as f:
                    return str(json.load(f).get("target") or "").lower()
            except Exception:
                return ""
        parent = os.path.dirname(here)
        if parent == here:
            break
        here = parent
    return ""


def get_runtime():
    """按 TESTVDB_TARGET env 分发到对应 target runtime 模块。

    返回的模块暴露统一接口：PATHS / request(method, path_key, body) /
    setup_default(name, dim) / drop_collection(name) / judge_4xx / judge_200。
    env 缺失时向上遍历找 structured_contract.json 读 target（v3.4 bootstrap
    三层 fallback 的库层实现——脚本层实现质量不齐，统一在此兜底）。
    """
    target = os.environ.get("TESTVDB_TARGET", "").lower()
    if not target:
        target = _fallback_target_from_contract()
    if target == "milvus":
        from . import milvus
        return milvus
    if target == "qdrant":
        from . import qdrant
        return qdrant
    if target == "weaviate":
        from . import weaviate
        return weaviate
    # ponytail: meilisearch 加一行 elif，weaviate 验收后顺序补。
    # pgvector/chroma 范式不同（SQL/SDK），单独立项不在此分发。
    raise RuntimeError(
        f"unsupported TESTVDB_TARGET={target!r}; implemented: milvus, qdrant, weaviate. "
        "Set TESTVDB_TARGET or implement scripts/runtime/<target>.py."
    )
