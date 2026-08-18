#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""check_physical_constraints.py — 视角 B（物理/语义约束）机械判定（E3 后稳定化扩展）。

背景：E3 实测聚合机械化后，剩余噪声源 = GREY_ZONE 34 case 的 B/C/D 判级轮间方差
（E2-r2→E3 翻正 5 TP + 丢 5 TP 全在灰区）。B 判据五类中数值下界与 HTTP 语义两类
判定条件可确定性匹配（链内信息已结构化：log_pattern 有"参数=值→http状态"原文、
http_semantics 有 client_error_returned_as、contract_grounding 有断言原文）。

设计（肯定性触发，同 implied_verdict 模式——机械层只管确定的）：
  规则1 数值下界：
    cg.assertion_text_quoted 含 "参数 >= N"（N>0）或 "positive" 断言
    AND log_pattern/secondary_observations 含该参数 = 违反值（< N 或 <=0）且被接受
    （http=200 / code=0 / accepted）→ B=CONFIRMED (数值下界)
  规则2 HTTP 语义（强限定双条件）：
    条件① http_semantics.client_error_returned_as ∈ {HTTP 2xx + 业务错误码, HTTP 5xx}
    条件② cg.assertion_text_quoted 含 reject/invalid/error/must be 类"应拒绝"声称
    双条件齐 → B=CONFIRMED (HTTP语义恒真)
  都不命中 → NOT_TRIGGERED（B 留 LLM 判定）

Usage:
  python check_physical_constraints.py <chain_json>   # 单链
  python check_physical_constraints.py --all <root>   # 全扫
输出 stdout JSON。
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

# 规则1：cg 断言中的数值下界（参数 >= N 或 positive）
LOWER_BOUND_RE = re.compile(r"(\w+)\s*>=?\s*(\d+(?:\.\d+)?)")
POSITIVE_RE = re.compile(r"(\w+)\s+(?:is|must be|should be)\s+positive", re.IGNORECASE)
# 规则1：log 中该参数接受违规值的观测（参数=<v> 且同段含接受标记）
ACCEPT_MARK = re.compile(r"http=200|code=0|accepted|success", re.IGNORECASE)
# 规则2 条件②：契约断言含"应拒绝"声称
REJECT_CLAIM = re.compile(r"reject|invalid|must be|error|should not accept", re.IGNORECASE)


def _observations(chain: dict) -> str:
    ee = (chain.get("steps") or {}).get("execution_evidence") or {}
    parts = [str(ee.get("log_pattern") or "")]
    parts += [str(x) for x in ee.get("secondary_observations") or []]
    return "\n".join(parts)


# 计数/数值类参数词典（负值在任何语义下无意义——SOP 视角 B"不需要契约背书"的机械落实。
# ⚠️ 不含 ef/nprobe：HNSW 类参数有 by-design 负值 sentinel 先例（weaviate DefaultEF=-1））
COUNT_PARAMS = {
    "desiredcount", "shardnum", "shard_number", "count", "size", "limit", "topk",
    "groupsize", "group_size", "replicationfactor", "factor", "offset",
    "maxresults", "query_maximum_results",
}

# 规则3 类型恒真（null 版）：契约 quote 含 must be/required 声称
MUST_BE_CLAIM = re.compile(r"must be|required", re.IGNORECASE)

# 规则2 条件②第三分支（A1，2026-08-18）：服务器错误消息自证值非法（却以 2xx 返回）
SERVER_SELF_INVALID = re.compile(r"is invalid|out of range|should be in range|must be in range", re.IGNORECASE)
BIZ_CODE_MARK = re.compile(r"code[=:]\s*\d{4,}", re.IGNORECASE)

# 规则4 资源边界（A2）：合法值触发服务端挂起 + 源码校验器只有下界无上界
HANG_OBS = re.compile(r"status=None|timeout|hang|unresponsive|OOM", re.IGNORECASE)
NO_UPPER_BOUND = re.compile(r"lower-bound only|no upper", re.IGNORECASE)


def judge_physical(chain: dict) -> dict:
    """视角 B 机械判定。返回 verdict_B / objective_constraint_class / evidence，或 NOT_TRIGGERED。"""
    cg = (chain.get("steps") or {}).get("contract_grounding") or {}
    quote = str(cg.get("assertion_text_quoted") or "")
    obs = _observations(chain)
    ee = (chain.get("steps") or {}).get("execution_evidence") or {}

    # ── 规则1a 数值下界（cg 断言参数级匹配，最精确） ──
    bounds: dict[str, float] = {}
    for m in LOWER_BOUND_RE.finditer(quote):
        param, val = m.group(1), float(m.group(2))
        if val > 0 and param.lower() not in ("version", "port"):  # 排除明显非约束参数
            bounds.setdefault(param, val)
    for m in POSITIVE_RE.finditer(quote):
        bounds.setdefault(m.group(1), 1.0)

    for param, bound in bounds.items():
        # log 中该参数的取值观测：param=<数值>（同一行内）
        pat = re.compile(rf"\b{re.escape(param)}\s*=\s*(-?\d+(?:\.\d+)?)")
        for m in pat.finditer(obs):
            v = float(m.group(1))
            # 取观测行的上下文（该行含接受标记才算"被接受"）
            line_start = obs.rfind("\n", 0, m.start()) + 1
            line_end = obs.find("\n", m.end())
            line = obs[line_start:line_end if line_end != -1 else len(obs)]
            if v < bound and ACCEPT_MARK.search(line):
                return {
                    "verdict_B": "CONFIRMED",
                    "objective_constraint_class": "数值下界",
                    "trigger": f"{param}={v} < {bound} 且被接受（{line.strip()[:80]}）",
                }

    # ── 规则1b 数值下界（计数类词典裸触发：负值被接受，不需要契约背书） ──
    for param in sorted(COUNT_PARAMS, key=len, reverse=True):
        pat = re.compile(rf"\b{re.escape(param)}\s*=\s*(-?\d+(?:\.\d+)?)", re.IGNORECASE)
        for m in pat.finditer(obs):
            v = float(m.group(1))
            line_start = obs.rfind("\n", 0, m.start()) + 1
            line_end = obs.find("\n", m.end())
            line = obs[line_start:line_end if line_end != -1 else len(obs)]
            if v < 0 and ACCEPT_MARK.search(line):
                return {
                    "verdict_B": "CONFIRMED",
                    "objective_constraint_class": "数值下界",
                    "trigger": f"计数参数 {param}={v}（负值）被接受（{line.strip()[:80]}）",
                }

    # ── 规则2 HTTP 语义（双条件：错误形态 + 契约应拒绝声称或观测注明 validation 或服务器自证） ──
    hs = ee.get("http_semantics") or {}
    code_mode = str(hs.get("client_error_returned_as") or "")
    hs_note = str(hs.get("note") or "")
    if code_mode in ("HTTP 2xx + 业务错误码", "HTTP 5xx") and (
            REJECT_CLAIM.search(quote) or "validation" in hs_note.lower()
            or SERVER_SELF_INVALID.search(obs)):
        trigger = (f"服务器自证（{SERVER_SELF_INVALID.search(obs).group(0) if SERVER_SELF_INVALID.search(obs) else ''}）"
                   if SERVER_SELF_INVALID.search(obs) else
                   f"契约/观测声称应拒绝（{quote[:40]} | {hs_note[:30]}）")
        return {
            "verdict_B": "CONFIRMED",
            "objective_constraint_class": "HTTP语义恒真",
            "trigger": f"{code_mode} 且 {trigger}",
        }

    # ── 规则3 类型恒真（null 被接受 + 契约 must be/required 声称） ──
    null_pat = re.compile(r"(\w+)\s*=\s*null\b", re.IGNORECASE)
    if MUST_BE_CLAIM.search(quote):
        for m in null_pat.finditer(obs):
            line_start = obs.rfind("\n", 0, m.start()) + 1
            line_end = obs.find("\n", m.end())
            line = obs[line_start:line_end if line_end != -1 else len(obs)]
            if ACCEPT_MARK.search(line):
                return {
                    "verdict_B": "CONFIRMED",
                    "objective_constraint_class": "类型恒真",
                    "trigger": f"{m.group(1)}=null 被接受且契约声称 must be/required（{line.strip()[:80]}）",
                }

    # ── 规则4 资源边界（A2，2026-08-18）：合法值触发挂起 + 源码校验器只有下界无上界 ──
    sg = (chain.get("steps") or {}).get("source_grounding") or {}
    excerpt = str(sg.get("source_excerpt") or "")
    if (HANG_OBS.search(obs)
            and (NO_UPPER_BOUND.search(excerpt) or "lower-bound" in excerpt.lower())):
        return {
            "verdict_B": "CONFIRMED",
            "objective_constraint_class": "资源边界",
            "trigger": f"合法值触发挂起（{HANG_OBS.search(obs).group(0)}）且源码校验器无上界",
        }

    return {"verdict_B": "NOT_TRIGGERED", "objective_constraint_class": None, "trigger": None}


def scan_all(root: Path) -> dict:
    out = {}
    for chain_path in sorted(root.rglob("evidence_chain/*.json")):
        if chain_path.name.endswith(".done"):
            continue
        try:
            chain = json.loads(chain_path.read_text(encoding="utf-8", errors="replace"))
        except json.JSONDecodeError:
            continue
        out[chain_path.stem] = judge_physical(chain)
    return out


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("chain", nargs="?")
    ap.add_argument("--all", dest="all_mode", action="store_true")
    args = ap.parse_args()
    if args.all_mode:
        root = Path(args.chain or r"C:/Users/11428/Desktop/tvdb_sessions")
        print(json.dumps(scan_all(root), ensure_ascii=False, indent=1))
        return 0
    if not args.chain:
        print("Usage: check_physical_constraints.py <chain.json> | --all <root>", file=sys.stderr)
        return 2
    chain = json.loads(Path(args.chain).read_text(encoding="utf-8", errors="replace"))
    print(json.dumps(judge_physical(chain), ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
