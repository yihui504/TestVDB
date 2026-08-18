#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_chain_grounding.py — 视角 A（契约）机械判定（ADR-0008 稳定化，E1 实验定稿 2026-08-18）。

背景：RQ2 归因实验实锤 auditor 视角 A 会话方差（同链同约束，四轮判词 A 值波动 44/71 case；
milvus_039-042 三值全变）。E1 回测（rq2_e1_grounding_report.md）：机械判定对 GT 方向
一致率 0.545 = LLM 最好轮（v2），且确定性零方差——判据通过，A 全量机械化。

判定规则（不含例外条款——grilling 决策 8：例外由视角 D 认知锚点吸收，
最后出口 NEEDS_MORE_EVIDENCE）：
  无 constraint_id 引用            → NEUTRAL (no_reference)
  id 不在契约中（精确匹配）         → NEUTRAL (constraint_absent)
  id 存在 + 引文是契约原文子串      → api_violates_assertion ? CONFIRMED : REFUTED (id+quote_ok)
  id 存在 + 引文不一致              → NEUTRAL (quote_mismatch, 以契约为准)

Usage:
  python check_chain_grounding.py <chain_json> <contract_json>     # 单链
  python check_chain_grounding.py --all <tvdb_sessions_root>       # 扫全部 vendor/version 组
输出 stdout JSON。Exit 0 正常（判定结果含 NEUTRAL 也是正常输出）。

Consumers: chain-auditor（SOP：读 grounding_check 输出并采信，不得自行改判 A）。
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


def judge_grounding(chain: dict, contract_text: str) -> dict:
    """视角 A 机械判定。chain = evidence_chain/{did}.json 内容；contract_text = 契约全文。"""
    cg = (chain.get("steps") or {}).get("contract_grounding") or {}
    cid = str(cg.get("constraint_id", "") or "").strip()
    if not cid:
        return {"verdict_A": "NEUTRAL", "reason": "no_reference",
                "constraint_id": None}
    if cid not in contract_text:
        return {"verdict_A": "NEUTRAL", "reason": "constraint_absent",
                "constraint_id": cid}
    quote = cg.get("assertion_text_quoted", "") or ""
    if quote and quote in contract_text:
        return {"verdict_A": "CONFIRMED" if cg.get("api_violates_assertion") else "REFUTED",
                "reason": "id+quote_ok", "constraint_id": cid}
    return {"verdict_A": "NEUTRAL", "reason": "quote_mismatch",
            "constraint_id": cid}


def scan_all(root: Path) -> dict:
    """扫 {root}/sessions/{vendor}/{version}/{did}/evidence_chain/{did}.json。"""
    out = {}
    for chain_path in sorted(root.rglob("evidence_chain/*.json")):
        if chain_path.name.endswith(".done"):
            continue
        did = chain_path.stem
        try:
            chain = json.loads(chain_path.read_text(encoding="utf-8", errors="replace"))
        except json.JSONDecodeError:
            out[did] = {"verdict_A": None, "reason": "chain_json_error"}
            continue
        contract = chain_path.parent.parent.parent / "structured_contract.json"
        if not contract.exists():
            out[did] = {"verdict_A": None, "reason": "contract_missing"}
            continue
        ct = contract.read_text(encoding="utf-8", errors="replace")
        out[did] = judge_grounding(chain, ct)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("chain", nargs="?", help="evidence_chain/{did}.json（单链模式）")
    ap.add_argument("contract", nargs="?", help="structured_contract.json（单链模式）")
    ap.add_argument("--all", dest="all_mode", action="store_true",
                    help="全扫模式：参数为 tvdb_sessions 根")
    args = ap.parse_args()

    if args.all_mode:
        root = Path(args.chain or r"C:/Users/11428/Desktop/tvdb_sessions")
        print(json.dumps(scan_all(root), ensure_ascii=False, indent=1))
        return 0

    if not args.chain or not args.contract:
        print("Usage: check_chain_grounding.py <chain.json> <contract.json> | --all <root>",
              file=sys.stderr)
        return 2
    chain = json.loads(Path(args.chain).read_text(encoding="utf-8", errors="replace"))
    ct = Path(args.contract).read_text(encoding="utf-8", errors="replace")
    print(json.dumps(judge_grounding(chain, ct), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
