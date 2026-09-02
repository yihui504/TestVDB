# -*- coding: utf-8 -*-
"""ADR-0009 §2-§4 两阶段探索调度（TDD：测试先行，RED -> GREEN）。

设计输入：docs/adr/0009-two-phase-exploration-and-exploratory-channel.md
- 阶段切换：块耗尽（round > len(chunks)）OR 连续 2 轮无新缺陷且 Δcoverage<=0
- 进入探索后不回退（重复触发返回 False）
- 探索僵局：连续 K=3 轮零信号命中 → 会话终止
- 批量探针预算：N<=8/批、M=4 批/轮（settings mining.exploration）
- 规范契约：orchestrator 含探索段；三 attack agent 含探索模式段
"""
import json
import os
import sys

import pytest

PLUGIN_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(PLUGIN_ROOT, "scripts"))

from exploration_phase import (  # noqa: E402
    should_enter_exploration,
    exploration_stalled,
)

ORCH = os.path.join(PLUGIN_ROOT, "agents", "orchestrator.md")
MINE = os.path.join(PLUGIN_ROOT, "commands", "mine.md")
ATTACK_SPECS = [os.path.join(PLUGIN_ROOT, "agents", f"attack-{a}.md")
                for a in ("boundary", "state", "semantic")]


# ── 阶段切换纯逻辑 ──────────────────────────────────────────────────────

class TestShouldEnterExploration:
    def test_chunks_exhausted_triggers(self):
        ok, reason = should_enter_exploration(
            round_no=6, num_chunks=5, consecutive_no_defect_rounds=0,
            coverage_delta=0.0, current_phase="enum")
        assert ok is True and "chunk" in reason.lower()

    def test_plateau_triggers(self):
        ok, reason = should_enter_exploration(
            round_no=3, num_chunks=5, consecutive_no_defect_rounds=2,
            coverage_delta=0.0, current_phase="enum")
        assert ok is True

    def test_plateau_requires_both_conditions(self):
        # 有新缺陷增长 OR 覆盖仍在涨 -> 不切
        for cnd, cd in ((1, 0.0), (2, 5.0)):
            ok, _ = should_enter_exploration(
                round_no=3, num_chunks=5, consecutive_no_defect_rounds=cnd,
                coverage_delta=cd, current_phase="enum")
            assert ok is False, (cnd, cd)

    def test_not_yet_exhausted_no_plateau_stays_enum(self):
        ok, _ = should_enter_exploration(
            round_no=2, num_chunks=5, consecutive_no_defect_rounds=0,
            coverage_delta=0.0, current_phase="enum")
        assert ok is False

    def test_no_reentry_from_exploration(self):
        # 已在探索阶段 -> 不重复触发（不回退由状态机保证）
        ok, _ = should_enter_exploration(
            round_no=9, num_chunks=5, consecutive_no_defect_rounds=9,
            coverage_delta=0.0, current_phase="exploration")
        assert ok is False


class TestExplorationStalled:
    def test_stall_at_default_k(self):
        assert exploration_stalled(3) is True
        assert exploration_stalled(2) is False

    def test_custom_k(self):
        assert exploration_stalled(2, k=2) is True
        assert exploration_stalled(1, k=2) is False


# ── 探索预算配置 ────────────────────────────────────────────────────────

class TestExplorationSettings:
    def test_mining_exploration_defaults(self):
        with open(os.path.join(PLUGIN_ROOT, "settings.json"), encoding="utf-8") as f:
            settings = json.load(f)
        exp = settings["mining"]["exploration"]
        assert exp["probe_batch_size"] == 8      # N<=8/批
        assert exp["batches_per_round"] == 4     # M=4 批/轮
        assert exp["stall_rounds"] == 3          # K=3 僵局终止

    def test_schema_declares_exploration(self):
        jsonschema = pytest.importorskip("jsonschema")
        with open(os.path.join(PLUGIN_ROOT, "settings.json"), encoding="utf-8") as f:
            settings = json.load(f)
        with open(os.path.join(PLUGIN_ROOT, "contracts", "settings_schema.json"),
                  encoding="utf-8") as f:
            schema = json.load(f)
        jsonschema.validate(settings, schema)
        props = schema["properties"]["mining"]["properties"]
        assert "exploration" in props
        exp_props = props["exploration"]["properties"]
        for key in ("probe_batch_size", "batches_per_round", "stall_rounds"):
            assert exp_props[key]["type"] == "integer"


# ── 规范契约（防文档漂移）───────────────────────────────────────────────

class TestSpecContract:
    def test_orchestrator_has_exploration_phase_section(self):
        with open(ORCH, encoding="utf-8") as f:
            spec = f.read()
        for kw in ("Exploration-mode dispatch", "Anomalous-response tracing", "Parameter-space combinatorial perturbation",
                   "State-sequence perturbation", "Behavioral-consistency comparison", "batch probe protocol"):
            assert kw in spec, f"orchestrator.md 缺探索调度关键字 {kw}"

    def test_mine_cmd_mentions_exploration(self):
        with open(MINE, encoding="utf-8") as f:
            assert "exploration" in f.read()

    def test_attack_agents_have_exploration_mode(self):
        for path in ATTACK_SPECS:
            with open(path, encoding="utf-8") as f:
                spec = f.read()
            assert "Exploration mode" in spec, f"{os.path.basename(path)} 缺探索模式段"
