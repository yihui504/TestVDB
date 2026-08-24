#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ADR-0009 §2 两阶段调度——阶段切换与探索僵局判定（纯函数，机械可测）。

调度语义（规范定义在 orchestrator.md 探索模式段，本模块是其机械落实）：
- 阶段一（shape-driven 枚举）：契约分块派发照旧；原"轮数>块数则循环重扫"
  改为触发阶段二切换评估。
- 切换条件（满足其一）：
  ① 契约块耗尽：round > num_chunks
  ② 平台期：连续 ≥2 轮无新缺陷且 Δcoverage <= 0
- 进入探索后不回退（状态机由 mine_state.phase 管理；重复评估返回 False）。
- 探索僵局：连续 K 轮（默认 3，settings mining.exploration.stall_rounds）
  零信号命中 → 会话终止（探索不是永动机）。
"""
from __future__ import annotations

PLATEAU_MIN_ROUNDS = 2  # 平台期条件②的连续无新缺陷轮数下限


def should_enter_exploration(round_no: int, num_chunks: int,
                             consecutive_no_defect_rounds: int,
                             coverage_delta: float,
                             current_phase: str = "enum") -> tuple[bool, str]:
    """评估是否从枚举阶段切换到探索阶段。返回 (是否切换, 理由)。"""
    if current_phase == "exploration":
        return False, "already in exploration (no re-entry)"
    if num_chunks > 0 and round_no > num_chunks:
        return True, "contract chunks exhausted"
    if (consecutive_no_defect_rounds >= PLATEAU_MIN_ROUNDS
            and coverage_delta <= 0):
        return True, (f"plateau: {consecutive_no_defect_rounds} rounds no new "
                      f"defect and coverage delta {coverage_delta} <= 0")
    return False, ""


def exploration_stalled(consecutive_zero_signal_rounds: int, k: int = 3) -> bool:
    """探索阶段僵局判定：连续 k 轮零信号命中（含空探针批）→ 终止会话。"""
    return consecutive_zero_signal_rounds >= k


def main() -> int:
    """CLI：orchestrator/主进程经 Bash 确定性评估阶段切换与僵局。

    usage: exploration_phase.py switch --round R --chunks N --no-defect X
                                --cov-delta D [--phase enum|exploration]
           exploration_phase.py stalled --zero-rounds Z [--k K]
    输出 JSON：{"switch": bool, "reason": "..."} / {"stalled": bool}
    """
    import argparse
    import json as _json
    parser = argparse.ArgumentParser(prog="exploration_phase")
    sub = parser.add_subparsers(dest="cmd", required=True)
    p_sw = sub.add_parser("switch")
    p_sw.add_argument("--round", type=int, required=True)
    p_sw.add_argument("--chunks", type=int, required=True)
    p_sw.add_argument("--no-defect", type=int, required=True)
    p_sw.add_argument("--cov-delta", type=float, required=True)
    p_sw.add_argument("--phase", default="enum")
    p_st = sub.add_parser("stalled")
    p_st.add_argument("--zero-rounds", type=int, required=True)
    p_st.add_argument("--k", type=int, default=3)
    args = parser.parse_args()
    if args.cmd == "switch":
        ok, reason = should_enter_exploration(
            args.round, args.chunks, args.no_defect, args.cov_delta, args.phase)
        print(_json.dumps({"switch": ok, "reason": reason}))
    else:
        print(_json.dumps({"stalled": exploration_stalled(args.zero_rounds, args.k)}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
