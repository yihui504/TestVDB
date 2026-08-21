"""TestVDB attack runtime — single dispatch entry.

agent 通过 get_runtime() 拿到当前 target 的 runtime 模块，不直接接触路径字符串。
"""
from __future__ import annotations

import os


def get_runtime():
    """按 TESTVDB_TARGET env 分发到对应 target runtime 模块。

    返回的模块暴露统一接口：PATHS / request(method, path_key, body) /
    setup_default(name, dim) / drop_collection(name) / judge_4xx / judge_200。
    """
    target = os.environ.get("TESTVDB_TARGET", "").lower()
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
