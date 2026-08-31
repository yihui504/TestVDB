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

信号冲突检测（2026-08-18 E5 后新增）：A=REFUTED（violates=False）但机械 B=CONFIRMED
（check_physical_constraints 独立证据源触发）时，implied_verdict 降为 CONFLICT——
violates 是 builder 声明的语义事实，与独立机械信号打架说明约束引用可能错位
（E5 实测 4 案全是"链引错约束/契约缺正约束 + B 抓到真信号"），verdict 走
NEEDS_MORE_EVIDENCE + rework 工单（不直接 DEFECT，走打回闭环）。

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
    """视角 A 机械判定 + 聚合暗示（2026-08-18 E2 差距解剖后增：A 定案 case 聚合唯一）。

    implied_verdict 四态：
      DEFECT      —— A=CONFIRMED，聚合唯一结果 DEFECT（LLM 无权改判）
      NOT_DEFECT  —— A=REFUTED，聚合唯一结果 NOT_DEFECT（LLM 无权改判）；
                     但机械 B=CONFIRMED 时降 CONFLICT（信号冲突，走打回）
      GREY_ZONE   —— A=NEUTRAL，聚合交 LLM 按视角 B/C/D 行使（SOP 聚合灰区分支）
      CONFLICT    —— A=REFUTED 与机械 B=CONFIRMED 冲突 → NEEDS_MORE_EVIDENCE + rework
    """
    cg = (chain.get("steps") or {}).get("contract_grounding") or {}
    # v2 链格式 fallback(R21 state_recommend_02 实测 GREY_ZONE 伪影修复,2026-08-31):
    # v2 链无 steps.contract_grounding——锚取 unit_ref(同样剥 :: 前缀),忠实性改查
    # doc_evidence[].quote 与契约原文的逐字匹配。violates 声明前置于候选层(8e 只收
    # VERDICT: DEFECT_FOUND 的候选建链),故引文忠实即 CONFIRMED,reason=id+quote_ok_v2。
    v2_mode = not cg
    if v2_mode:
        cg = {"constraint_id": str(chain.get("unit_ref", "") or ""),
              "assertion_text_quoted": "", "api_violates_assertion": True}
    cid = str(cg.get("constraint_id", "") or "").strip()
    # unit_ref 前缀归一（R6 实证：链内 cid 带 "assertions::" 前缀导致 constraint_absent
    # 误判 GREY_ZONE——candidates.jsonl 的 constraint_id 保留前缀形态，此处剥到底）
    cid = cid.split("::")[-1].strip() if "::" in cid else cid
    if not cid:
        return {"verdict_A": "NEUTRAL", "reason": "no_reference",
                "implied_verdict": "GREY_ZONE", "constraint_id": None}
    if cid not in contract_text:
        return {"verdict_A": "NEUTRAL", "reason": "constraint_absent",
                "implied_verdict": "GREY_ZONE", "constraint_id": cid}
    quote = cg.get("assertion_text_quoted", "") or ""
    # v2:忠实性检查对象 = doc_evidence[].quote 任一命中（v2 链无单引文字段）
    v2_quotes: list[str] = []
    if v2_mode:
        v2_quotes = [str(d.get("quote", "") or "") for d in (chain.get("doc_evidence") or [])
                     if isinstance(d, dict)]
        v2_quotes = [q for q in v2_quotes if q.strip()]
    # JSON 转义规范化（2026-08-20 v8 考古案发现）：quote 含引号时，契约文件原文是 \" 形态
    # 而链内 quote 是解析后的 "——两侧都试，命中即算逐字
    norm = contract_text.replace('\\"', '"')

    def _quote_hit(q: str) -> bool:
        return bool(q) and (q in contract_text or q in norm)

    if (quote and _quote_hit(quote)) or any(_quote_hit(q) for q in v2_quotes):
        if cg.get("api_violates_assertion"):
            # by_design 锚点感知冲突（v8 FP 侧 2026-08-20）：A=CONFIRMED 但链现象命中
            # cognition 的 by_design 锚点（确定性关键词匹配）→ 降 CONFLICT 走打回闭环，
            # 不直接翻 NOT_DEFECT（机械纪律保持——终判交闭环复核）
            try:
                from check_anchor_conflict import detect_anchor_conflict
                ac = detect_anchor_conflict(chain)
                if ac.get("anchored_conflict"):
                    return {"verdict_A": "CONFIRMED", "reason": "id+quote_ok_anchor_conflict",
                            "implied_verdict": "CONFLICT", "constraint_id": cid,
                            "conflict_note": ac.get("anchor")}
            except ImportError:
                pass
            return {"verdict_A": "CONFIRMED", "reason": "id+quote_ok_v2" if v2_mode else "id+quote_ok",
                    "implied_verdict": "DEFECT", "constraint_id": cid}
        # 信号冲突检测：violates=False 但机械 B 有独立 CONFIRMED 证据
        try:
            from check_physical_constraints import judge_physical
            if judge_physical(chain).get("verdict_B") == "CONFIRMED":
                return {"verdict_A": "REFUTED", "reason": "id+quote_ok_conflict",
                        "implied_verdict": "CONFLICT", "constraint_id": cid,
                        "conflict_note": "violates=False 与机械 B=CONFIRMED 冲突——约束引用可能错位，走 rework"}
        except ImportError:
            pass
        return {"verdict_A": "REFUTED", "reason": "id+quote_ok",
                "implied_verdict": "NOT_DEFECT", "constraint_id": cid}
    return {"verdict_A": "NEUTRAL", "reason": "quote_mismatch",
            "implied_verdict": "GREY_ZONE", "constraint_id": cid}


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
