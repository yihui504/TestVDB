#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""chunk_contract.py — ADR-0008 契约分块（attack agents 每轮消费一块）。

背景（导师 2026-08-17 反馈）：structured_contract.json 整包一次投喂给 attack agents，
上下文压力大且每轮攻击目标不聚焦。改为确定性分块，每轮派发一块。

分块规则（确定性，按 endpoint 分组，0 LLM）：
  1. 收集契约的全部"可攻单元"：constraints + assertions + behavioral_contracts +
     state_invariants，每项按其 endpoint 归组（无 endpoint 的 state_invariants 归
     "__global__" 组）
  2. 按 endpoint 组切块，每块目标规模 ≤ CHUNK_SIZE（默认 12 个可攻单元）；
     单组超限时组内顺序切多块（块名带 -1/-2 序号）
  3. 块顺序固定（endpoint 字典序），保证跨 run 可复现
  4. 产出 chunks.json（块清单 + 每块的单元引用，不复制内容——attack agent 按
     引用回读 structured_contract.json，避免数据冗余漂移）

Usage:
  python scripts/chunk_contract.py <contract_path> [--chunk-size 12] [--session-dir DIR]

Exit: 0 = 正常（含 0 块）；2 = usage 错误。
Consumers: orchestrator Step 8b（每轮取 chunk[i % n] 派发 Attack Trio）。
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

# 可攻单元的契约字段 → (id_key, endpoint_key)
UNIT_SOURCES = [
    ("constraints", "constraint_id", "endpoint"),
    ("assertions", "assertion_id", "endpoint"),
    ("behavioral_contracts", "contract_id", "related_endpoints"),
    ("state_invariants", "invariant_id", "scope"),
]
GLOBAL_GROUP = "__global__"


def _endpoint_of(unit: dict, endpoint_key: str) -> str:
    v = unit.get(endpoint_key)
    if isinstance(v, str) and v.strip():
        return v.strip()
    if isinstance(v, list) and v:
        return str(v[0])  # 多端点约束归首端点（同组攻击，其余端点在单元内可见）
    return GLOBAL_GROUP


def build_chunks(contract: dict, chunk_size: int) -> list[dict]:
    groups: dict[str, list[dict]] = {}
    for field, id_key, ep_key in UNIT_SOURCES:
        items = contract.get(field)
        if not isinstance(items, list):
            continue
        for unit in items:
            if not isinstance(unit, dict) or not unit.get(id_key):
                continue
            groups.setdefault(_endpoint_of(unit, ep_key), []).append({
                "unit_ref": f"{field}::{unit[id_key]}",
                "source": field,
                id_key: unit[id_key],
                "endpoint": _endpoint_of(unit, ep_key),
            })

    chunks: list[dict] = []
    for ep in sorted(groups):
        units = groups[ep]
        if len(units) <= chunk_size:
            chunks.append({"chunk_id": f"chunk_{ep}", "endpoints": [ep],
                           "units": units, "unit_count": len(units)})
        else:
            n = (len(units) + chunk_size - 1) // chunk_size
            for i in range(n):
                part = units[i * chunk_size:(i + 1) * chunk_size]
                chunks.append({"chunk_id": f"chunk_{ep}-{i + 1}of{n}",
                               "endpoints": [ep], "units": part,
                               "unit_count": len(part)})
    return chunks


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("contract_path")
    ap.add_argument("--chunk-size", type=int, default=12)
    ap.add_argument("--session-dir", default=None,
                    help="写入 {session_dir}/chunks.json；缺省写契约同目录")
    args = ap.parse_args()

    cp = Path(args.contract_path)
    if not cp.is_file():
        print(f"ERROR: contract not found: {cp}", file=sys.stderr)
        return 2
    contract = json.loads(cp.read_text(encoding="utf-8"))

    chunks = build_chunks(contract, args.chunk_size)
    out_dir = Path(args.session_dir) if args.session_dir else cp.parent
    out = out_dir / "chunks.json"
    out.write_text(json.dumps({
        "contract": str(cp),
        "chunk_size": args.chunk_size,
        "total_chunks": len(chunks),
        "total_units": sum(c["unit_count"] for c in chunks),
        "chunks": chunks,
    }, ensure_ascii=False, indent=2), encoding="utf-8")

    print(json.dumps({
        "total_chunks": len(chunks),
        "total_units": sum(c["unit_count"] for c in chunks),
        "chunk_ids": [c["chunk_id"] for c in chunks],
        "output": str(out),
    }, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
