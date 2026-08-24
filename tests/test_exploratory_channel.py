# -*- coding: utf-8 -*-
"""ADR-0009 §5 exploratory 候选通道（TDD：测试先行，RED -> GREEN）。

设计输入：docs/adr/0009-two-phase-exploration-and-exploratory-channel.md
- verdict 保持二值（strict 判定层零改动）；新增 candidate_class 三态标注
- 三条件准入：has_claim + has_inferential_support（三形态）+ below_strict
- 机械辅助：check_physical_constraints 规则5 近似形态 -> exploratory_signal
  （不参与 verdict_B，仅作 auditor 通道标注提示）
- 排除项：violates=false 且无主张零信号旧链 -> rejected（不由通道兜底）
"""
import json
import os
import sys

import pytest

PLUGIN_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(PLUGIN_ROOT, "scripts"))

from exploratory_channel import classify_candidate_class, consistency_error  # noqa: E402

AUDITOR_SPEC = os.path.join(PLUGIN_ROOT, "agents", "chain-auditor.md")


# ── candidate_class 纯逻辑 ──────────────────────────────────────────────

class TestClassifyCandidateClass:
    def test_defect_verdict_is_strict(self):
        assert classify_candidate_class(
            verdict="DEFECT", has_claim=True, form=None, below_strict=False
        ) == "strict_defect"

    def test_not_defect_with_three_conditions_is_exploratory(self):
        assert classify_candidate_class(
            verdict="NOT_DEFECT", has_claim=True,
            form="inference_consistency", below_strict=True,
        ) == "exploratory_candidate"

    def test_missing_claim_is_rejected(self):
        # 排除项：violates 族零信号无主张 -> rejected
        assert classify_candidate_class(
            verdict="NOT_DEFECT", has_claim=False,
            form=None, below_strict=True,
        ) == "rejected"

    def test_missing_form_is_rejected(self):
        assert classify_candidate_class(
            verdict="NOT_DEFECT", has_claim=True,
            form=None, below_strict=True,
        ) == "rejected"

    def test_mechanically_decided_not_defect_needs_form(self):
        # below_strict=False（机械定案）+ 无形态 -> rejected；
        # 机械定案 NOT_DEFECT 不靠推断证据翻案，通道不标注
        assert classify_candidate_class(
            verdict="NOT_DEFECT", has_claim=True,
            form=None, below_strict=False,
        ) == "rejected"

    @pytest.mark.parametrize("form", [
        "inference_consistency", "competing_explanation", "behavioral_anomaly",
    ])
    def test_all_three_forms_accepted(self, form):
        assert classify_candidate_class(
            verdict="NOT_DEFECT", has_claim=True, form=form, below_strict=True,
        ) == "exploratory_candidate"

    def test_unknown_form_rejected(self):
        with pytest.raises(ValueError):
            classify_candidate_class(
                verdict="NOT_DEFECT", has_claim=True,
                form="gut_feeling", below_strict=True,
            )


class TestConsistencyError:
    def test_defect_with_exploratory_class_is_error(self):
        err = consistency_error(verdict="DEFECT", candidate_class="exploratory_candidate")
        assert err is not None

    def test_defect_with_strict_class_ok(self):
        assert consistency_error(verdict="DEFECT", candidate_class="strict_defect") is None

    def test_not_defect_with_strict_class_is_error(self):
        assert consistency_error(verdict="NOT_DEFECT", candidate_class="strict_defect") is not None

    def test_valid_pairs_ok(self):
        for cls in ("exploratory_candidate", "rejected"):
            assert consistency_error(verdict="NOT_DEFECT", candidate_class=cls) is None


# ── 规则5 近似形态机械信号（不改变 verdict_B）───────────────────────────

def _grey_chain(obs_lines):
    """构造灰区链：quote 为空（机械 A 无断言可核）+ 物理观测行（judge_physical 的真实读取路径）。"""
    return {
        "steps": {
            "contract_grounding": {"assertion_text_quoted": ""},  # 规则1/2/3 不触发
            "execution_evidence": {
                "log_pattern": obs_lines[0],
                "secondary_observations": obs_lines[1:],
            },
        },
    }


class TestRule5ApproxExploratorySignal:
    def test_sibling_reject_plus_silent_accept_yields_signal(self):
        # 033 型：同族对照被拒 + 本参数静默接受（无服务器替换自述）
        from check_physical_constraints import judge_physical
        lines = [
            'http=200 code=0 dataType=BINARY_VECTOR resp="ok"',            # 静默接受（acc）
            'http=400 code=1804 msg="dataType unsupported, rejected"',      # 同族拒绝对照
        ]
        out = judge_physical(_grey_chain(lines))
        sig = out.get("exploratory_signal")
        assert sig is not None and sig.get("form") == "inference_consistency"
        # 关键：不改变 verdict_B（strict 零改动）
        assert out.get("verdict_B") != "CONFIRMED"

    def test_no_sibling_reject_no_signal(self):
        from check_physical_constraints import judge_physical
        lines = ['http=200 code=0 dataType=BINARY_VECTOR resp="ok"']
        out = judge_physical(_grey_chain(lines))
        assert out.get("exploratory_signal") is None

    def test_strict_rule5_hit_no_signal_needed(self):
        # 服务器自述替换 -> 规则5 正常 CONFIRMED，无需近似信号
        from check_physical_constraints import judge_physical
        lines = [
            'http=200 code=0 dbName="" substituted=default_db server_note="default applied"',
            'http=400 code=100 msg="collectionName empty rejected"',
        ]
        out = judge_physical(_grey_chain(lines))
        assert out.get("verdict_B") == "CONFIRMED"
        assert out.get("exploratory_signal") is None


# ── 规范契约（防 chain-auditor.md 漂移）────────────────────────────────

class TestAuditorSpecContract:
    def test_spec_mentions_channel_fields(self):
        with open(AUDITOR_SPEC, encoding="utf-8") as f:
            spec = f.read()
        for kw in ("candidate_class", "inference_consistency",
                   "competing_explanation", "behavioral_anomaly"):
            assert kw in spec, f"chain-auditor.md 缺关键字 {kw}"
