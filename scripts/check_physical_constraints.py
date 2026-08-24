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
import os
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

# 规则5 同族同处置（2026-08-23 一致性元规则）：静默替换语义 + 观测行级拒绝标记
# 设计依据 = 挖掘侧 generalization_shapes（参数族同处置）的判定侧镜像；
# 信息源为链内对照观测，不经过被测案 issue/GT。测试先行：
# tests/test_adr0008_pipeline.py::TestMechanicalBConsistencyRules
SUBSTITUTE_SEMANTICS = re.compile(
    r"substitut|coerce|fallback|replaced with default|default value", re.IGNORECASE)
REJECT_MARK = re.compile(r"rejected|denied|http=4\d\d|http=5\d\d|unsupported", re.IGNORECASE)

# 规则6 接口面对称（2026-08-23）：接口面标记（builder 对照取证义务保证行内可得）
FACE_RE = re.compile(r"\b(grpc|rest|sdk)\b", re.IGNORECASE)

# 观测行参数名提取（排除协议字段，供规则5/6 对照判定）
_OBS_PARAM_RE = re.compile(r"\b([A-Za-z_]\w*)\s*=")
_OBS_PARAM_STOP = {"http", "code", "status", "port", "req", "resp"}


def _line_params(line: str) -> set[str]:
    """提取观测行内的参数名（小写，排除协议字段）。"""
    return {m.group(1).lower() for m in _OBS_PARAM_RE.finditer(line)
            if m.group(1).lower() not in _OBS_PARAM_STOP}


# 规则4 源码 grep：校验注解模式（跨语言）
MIN_ANNOT_RE = re.compile(r"range\s*\(\s*min|validate.*min|binding.*\bgt|binding.*\bgte", re.IGNORECASE)
MAX_ANNOT_RE = re.compile(r"range\s*\(\s*[^)]*max|validate.*max|binding.*\blt|binding.*\blte", re.IGNORECASE)


def _grep_bound_in_source(src_dir, files, param_names=None):
    """读 files（相对 src_dir 的路径），grep 校验注解确认有无下界/上界。

    param_names 提供时：只看目标参数字段定义行上方 3 行内的注解（精准，
    不被同文件里其他参数的 max 注解污染）。未提供时退回全文 grep（粗）。
    返回 (has_min, has_max, hit_files)。机械判定——不依赖 builder 措辞。
    """
    has_min = has_max = False
    hit = 0
    for rel in files:
        p = os.path.join(src_dir, str(rel))
        if not os.path.isfile(p):
            continue
        try:
            txt = open(p, encoding="utf-8", errors="replace").read()
        except OSError:
            continue
        hit += 1
        lines = txt.splitlines()
        if not param_names:
            if MIN_ANNOT_RE.search(txt):
                has_min = True
            if MAX_ANNOT_RE.search(txt):
                has_max = True
            continue
        # 有参数名：定位字段定义行（行含参数名 + ':'），看上方 3 行注解
        for i, line in enumerate(lines):
            if ":" not in line:
                continue
            if not any(re.search(rf"\b{re.escape(pn)}\b", line) for pn in param_names):
                continue
            ctx = "\n".join(lines[max(0, i - 3):i + 1])
            if MIN_ANNOT_RE.search(ctx):
                has_min = True
            if MAX_ANNOT_RE.search(ctx):
                has_max = True
    return has_min, has_max, hit


def judge_physical(chain: dict, src_dir=None) -> dict:
    """视角 B 机械判定。返回 verdict_B / objective_constraint_class / evidence，或 NOT_TRIGGERED。

    src_dir（可选）：目标 DB 源码 clone 根目录。提供时规则4 直接 grep 源码确认
    "校验器有下界无上界"（机械，不依赖 builder 的 source_excerpt 措辞）；未提供时
    回退到旧 excerpt 逻辑（向后兼容）。
    """
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
    hang_match = HANG_OBS.search(obs)
    if hang_match:
        sg = (chain.get("steps") or {}).get("source_grounding") or {}
        if src_dir:
            # 机械路径：直接 grep 源码确认"有 min 无 max"（不依赖 builder 措辞）
            files = sg.get("files_examined") or []
            # 从 obs 提取参数名（精准定位字段定义，避免其他参数的 max 注解污染）
            _stop = {"http", "code", "status", "req", "resp", "port", "id", "count", "limit", "size"}
            param_names = [w for w in re.findall(r"(\w+)\s*[:=]\s*-?[\d.]+", obs)
                           if w.lower() not in _stop]
            param_names = list(dict.fromkeys(param_names))
            has_min, has_max, hit = _grep_bound_in_source(src_dir, files, param_names or None)
            if has_min and not has_max and hit > 0:
                return {
                    "verdict_B": "CONFIRMED",
                    "objective_constraint_class": "资源边界",
                    "trigger": f"合法值触发挂起（{hang_match.group(0)}）且源码 grep 证实 {param_names[:2]} 校验器有 min 无 max（{hit} 文件）",
                }
            # 源码未证实 → 不触发（保持机械性，不回退 excerpt）
        else:
            # 向后兼容：src_dir 未提供时用旧 excerpt 逻辑
            excerpt = str(sg.get("source_excerpt") or "")
            if NO_UPPER_BOUND.search(excerpt) or "lower-bound" in excerpt.lower():
                return {
                    "verdict_B": "CONFIRMED",
                    "objective_constraint_class": "资源边界",
                    "trigger": f"合法值触发挂起（{hang_match.group(0)}）且源码校验器无上界（excerpt）",
                }

    # ── 规则5 同族同处置（2026-08-23）：静默替换接受 + 同族其他参数被拒对照 ──
    # 触发形态：同端点上同类违规值，一族被拒绝、另一族被静默替换为默认值且
    # 返回成功——处置不一致即缺陷信号（无契约背书也可判，同规则1b 哲学）。
    # 护栏：①接受侧行须含替换语义原文 + 接受标记（防误伤"非法值回退默认"
    # 整体风格——无同族拒绝对照即无"服务器认为该类值非法"的证据）；
    # ②拒绝对照行的参数须不同于接受侧（同参数一拒一收=间歇观测，非族）。
    obs_lines = [ln for ln in obs.splitlines() if ln.strip()]
    sub_accept = None  # (params, line)
    silent_accepts: list[tuple[set[str], str]] = []  # 规则5近似形态（ADR-0009 §5）
    reject_pool: list[tuple[set[str], str]] = []
    for ln in obs_lines:
        acc = ACCEPT_MARK.search(ln)
        ps = _line_params(ln)
        if acc and SUBSTITUTE_SEMANTICS.search(ln):
            sub_accept = (ps, ln)
        elif acc and ps:
            silent_accepts.append((ps, ln))
        if REJECT_MARK.search(ln) and not acc:
            reject_pool.append((ps, ln))
    if sub_accept:
        for rps, rln in reject_pool:
            extra = rps - sub_accept[0]
            if rps and extra:  # 对照参数不同（族不同处置）
                return {
                    "verdict_B": "CONFIRMED",
                    "objective_constraint_class": "同族不一致",
                    "trigger": f"{sorted(sub_accept[0])} 静默替换被接受 vs 同族 {sorted(extra)} 被拒"
                               f"（{rln.strip()[:80]}）",
                }

    # ── 规则6 接口面对称（2026-08-23）：同参数跨接口面一拒一收 ──
    # 触发形态：同参数同违规值，A 面拒绝、B 面接受（方向无关）——面间不对称
    # 即缺陷信号。契约明示的面间差异不触发（qdrant_010 payload-only 先例）——
    # 该护栏由 auditor 复核（机械层纯现象，trigger 内提示）。
    # 靶面依赖 builder 对照取证义务（观测行须含 grpc/rest/sdk 面标记原文）。
    face_lines = []
    for ln in obs_lines:
        faces = frozenset(m.group(1).lower() for m in FACE_RE.finditer(ln))
        if faces:
            face_lines.append((ln, faces, _line_params(ln)))
    for aln, afaces, aps in face_lines:
        if not aps or not ACCEPT_MARK.search(aln):
            continue
        for rln, rfaces, rps in face_lines:
            if rln is aln or afaces == rfaces or not (aps & rps):
                continue
            if REJECT_MARK.search(rln):
                return {
                    "verdict_B": "CONFIRMED",
                    "objective_constraint_class": "接口不对称",
                    "trigger": f"{sorted(aps & rps)} 于 {sorted(afaces)} 被接受但 {sorted(rfaces)} "
                               f"被拒（{rln.strip()[:80]}）；契约明示面间差异由 auditor 复核",
                }

    # ── 规则5 近似形态（ADR-0009 §5）：同族对照拒绝 + 无自述静默接受 ──
    # 触发形态：接受侧行无服务器替换自述（严格规则5 条件①不满足），但同族
    # 对照行有拒绝对照——"前后对照推断替换/静默回退"形态（milvus_033 型）。
    # 输出 exploratory_signal 提示 auditor 标注通道；**不参与 verdict_B**
    # （strict 判定零改动；词表形态学通用，非单案回调）。
    result = {"verdict_B": "NOT_TRIGGERED", "objective_constraint_class": None,
              "trigger": None}
    for aps, aln in silent_accepts:
        for rps, rln in reject_pool:
            fam = (aps & rps) or {p for p in aps if p in rln.lower()}
            if fam:
                result["exploratory_signal"] = {
                    "form": "inference_consistency",
                    "detail": (f"{sorted(aps)} 静默接受（无服务器替换自述）vs 同族 "
                               f"{sorted(fam)} 对照被拒（{rln.strip()[:80]}）"),
                }
                return result
    return result


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


def check_violates_consistency(chain: dict) -> dict:
    """violates 机械复核（2026-08-18 E5 后改进 2）。

    builder 声明 api_violates_assertion=False（不违反）但链内有强违规观测时——
    violates 判定存疑，应打回 builder 复核（rework 通道）。
    模式：quote 含约束声称（must/should/>=/<）且 obs 有"值被接受"观测
    （参数=值 + http=200/code=0），此时 violates=False 与观测矛盾。

    返回 {"suspicious": bool, "evidence": str}。
    """
    cg = (chain.get("steps") or {}).get("contract_grounding") or {}
    if cg.get("api_violates_assertion"):  # True 方向不复核（机械 A 已采信）
        return {"suspicious": False, "evidence": None}
    quote = str(cg.get("assertion_text_quoted") or "")
    obs = _observations(chain)
    has_claim = bool(re.search(r"must|should|>=|<=|<|>|valid|required|match", quote, re.IGNORECASE))
    # 值被接受观测：参数=值 且同段含 200/code:0/success/accepted
    accepted = re.search(
        r"(\w+)\s*=\s*[^\s,)]+[^\n]*(?:http=200|code=0|success|accepted)",
        obs, re.IGNORECASE)
    if has_claim and accepted:
        return {"suspicious": True,
                "evidence": f"violates=False 但观测显示值被接受（{accepted.group(0)[:70]}）而 quote 含约束声称（{quote[:50]}）"}
    return {"suspicious": False, "evidence": None}
