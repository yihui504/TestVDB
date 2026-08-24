#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""ADR-0009 §5 exploratory 候选通道——candidate_class 判定与一致性校验。

verdict 保持二值（strict 判定层零改动）；candidate_class 是附加标注三态：
  strict_defect / exploratory_candidate / rejected

三条件准入（全部满足才标 exploratory_candidate）：
  ① has_claim：链内有明确缺陷主张
  ② has_inferential_support：三形态之一有可指认证据
  ③ below_strict：机械 A/B 未定案（灰区路径）

排除项（设计决策，非默认行为）：violates=false 且无机械信号且链内无主张的
旧链 -> rejected（零信号≠低强度；此类交端到端重挖，不由通道兜底）。
"""
from __future__ import annotations

VALID_FORMS = frozenset({
    "inference_consistency",    # 同族/接口面对称的推断性不一致
    "competing_explanation",    # 主张与源码级 by-design 平行解释并存
    "behavioral_anomaly",       # 行为异常但契约无断言
})

VALID_CLASSES = frozenset({"strict_defect", "exploratory_candidate", "rejected"})


def classify_candidate_class(verdict: str, has_claim: bool,
                             form: str | None, below_strict: bool) -> str:
    """由判定要素推导 candidate_class（机械校验/统计复用；auditor 按规范同规则执行）。"""
    if verdict not in ("DEFECT", "NOT_DEFECT"):
        raise ValueError(f"verdict must be DEFECT|NOT_DEFECT, got {verdict!r}")
    if verdict == "DEFECT":
        return "strict_defect"
    if not has_claim:
        return "rejected"           # 排除项：无主张零信号
    if form is None:
        return "rejected"
    if form not in VALID_FORMS:
        raise ValueError(f"unknown form: {form!r}")
    if not below_strict:
        return "rejected"           # 机械定案案不经推断证据翻案
    return "exploratory_candidate"


def consistency_error(verdict: str, candidate_class: str) -> str | None:
    """verdict × candidate_class 一致性校验（防标注自相矛盾）。"""
    if verdict not in ("DEFECT", "NOT_DEFECT"):
        return f"invalid verdict: {verdict!r}"
    if candidate_class not in VALID_CLASSES:
        return f"invalid candidate_class: {candidate_class!r}"
    if verdict == "DEFECT" and candidate_class != "strict_defect":
        return (f"verdict=DEFECT requires candidate_class=strict_defect, "
                f"got {candidate_class!r}")
    if verdict == "NOT_DEFECT" and candidate_class == "strict_defect":
        return "verdict=NOT_DEFECT cannot carry candidate_class=strict_defect"
    return None
