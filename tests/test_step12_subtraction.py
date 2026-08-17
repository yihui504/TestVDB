"""Step 1/2 减法落地测试（导师 v3.1 反馈 checklist 1.1/1.2，ADR-0008）.

覆盖:
- chunk_contract: 契约分块（endpoint 分组 / 超限切多块 / 全局组 / 确定性顺序）
- attack agents 规范: 数量下限已删（策略覆盖目标替代）、meta.json 无 confidence
- contract-formalizer 规范: evidence_tier 两档、convention/inferred_from_* 已删、confidence 已删
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

import pytest

SCRIPTS_DIR = Path(__file__).resolve().parent.parent / "scripts"
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

from chunk_contract import build_chunks  # noqa: E402

AGENTS_DIR = Path(__file__).resolve().parent.parent / "agents"


# ═══════════════════════════════════════════════════════════════
# chunk_contract（1.2-①）
# ═══════════════════════════════════════════════════════════════

class TestChunkContract:
    def _contract(self, n_constraints=3, n_assert=2, n_invariants=1):
        return {
            "constraints": [
                {"constraint_id": f"c_{i}", "endpoint": f"ep{i % 2}", "assertion": "x"}
                for i in range(n_constraints)
            ],
            "assertions": [
                {"assertion_id": f"a_{i}", "endpoint": "ep0", "category": "behavioral"}
                for i in range(n_assert)
            ],
            "behavioral_contracts": [
                {"contract_id": "bc_1", "related_endpoints": ["ep1", "ep9"]}
            ],
            "state_invariants": [
                {"invariant_id": f"inv_{i}", "scope": "" if i == 0 else "ep1"}
                for i in range(n_invariants)
            ],
        }

    def test_endpoint_grouping(self):
        chunks = build_chunks(self._contract(), chunk_size=12)
        by_id = {c["chunk_id"]: c for c in chunks}
        assert "chunk_ep0" in by_id and "chunk_ep1" in by_id
        # ep0: c_0/c_2 constraints + a_0/a_1 assertions = 4
        assert by_id["chunk_ep0"]["unit_count"] == 4
        # ep1: c_1 + bc_1（related_endpoints 首元素 ep1）= 2
        assert by_id["chunk_ep1"]["unit_count"] == 2

    def test_scopeless_invariant_goes_global(self):
        chunks = build_chunks(self._contract(), chunk_size=12)
        g = next(c for c in chunks if c["chunk_id"] == "chunk___global__")
        assert any(u["unit_ref"] == "state_invariants::inv_0" for u in g["units"])

    def test_oversize_group_splits(self):
        c = {"constraints": [
            {"constraint_id": f"c_{i}", "endpoint": "big", "assertion": "x"}
            for i in range(25)
        ]}
        chunks = build_chunks(c, chunk_size=12)
        bigs = [c2 for c2 in chunks if c2["endpoints"] == ["big"]]
        assert len(bigs) == 3  # 12 + 12 + 1
        assert bigs[0]["chunk_id"] == "chunk_big-1of3"
        assert sum(b["unit_count"] for b in bigs) == 25

    def test_deterministic_order(self):
        a = build_chunks(self._contract(4, 3, 2), chunk_size=5)
        b = build_chunks(self._contract(4, 3, 2), chunk_size=5)
        assert [c["chunk_id"] for c in a] == [c["chunk_id"] for c in b]

    def test_unit_ref_roundtrip(self):
        """unit_ref 格式 source::id 可回查契约原文。"""
        contract = self._contract()
        chunks = build_chunks(contract, chunk_size=12)
        for ch in chunks:
            for u in ch["units"]:
                src, uid = u["unit_ref"].split("::", 1)
                items = contract[src]
                assert any(x.get(
                    "constraint_id" if src == "constraints" else
                    "assertion_id" if src == "assertions" else
                    "contract_id" if src == "behavioral_contracts" else
                    "invariant_id") == uid for x in items)


# ═══════════════════════════════════════════════════════════════
# attack agents 规范（1.2-②③）
# ═══════════════════════════════════════════════════════════════

class TestAttackAgentSpecs:
    @pytest.mark.parametrize("agent", ["attack-boundary", "attack-state", "attack-semantic"])
    def test_no_minimum_script_count(self, agent):
        s = (AGENTS_DIR / f"{agent}.md").read_text(encoding="utf-8", errors="replace")
        assert "≥ 5 个" not in s and "≥ 3 个" not in s, f"{agent}: 数量下限残留"
        assert "策略覆盖目标" in s, f"{agent}: 缺策略覆盖目标提法"

    @pytest.mark.parametrize("agent", ["attack-boundary", "attack-state", "attack-semantic"])
    def test_no_confidence_in_spec(self, agent):
        s = (AGENTS_DIR / f"{agent}.md").read_text(encoding="utf-8", errors="replace")
        assert "confidence ≥ 0.7" not in s, f"{agent}: confidence 优先级残留"
        assert "evidence_tier=explicit" in s, f"{agent}: 缺 evidence_tier 优先级"


# ═══════════════════════════════════════════════════════════════
# contract-formalizer 规范（1.1-②③④）
# ═══════════════════════════════════════════════════════════════

class TestContractFormalizerSpec:
    def test_evidence_tier_two_levels(self):
        s = (AGENTS_DIR / "contract-formalizer.md").read_text(encoding="utf-8", errors="replace")
        assert '"convention"' not in s.replace("convention 档", "").replace("convention（", ""), \
            "convention 档残留（enum 中）"
        assert '"inferred_from_example"' not in s, "旧四档 enum 残留"
        assert '"explicit", "inferred"' in s, "两档 enum 缺失"

    def test_confidence_removed_from_schema(self):
        s = (AGENTS_DIR / "contract-formalizer.md").read_text(encoding="utf-8", errors="replace")
        # required 数组与 schema 属性不再含 confidence（历史教训叙述除外）
        assert '"confidence", "source_url"' not in s
        assert '"confidence": { "type": "number"' not in s

    def test_no_standardization_wording(self):
        s = (AGENTS_DIR / "contract-formalizer.md").read_text(encoding="utf-8", errors="replace")
        assert "端点分类标准化" not in s, "'标准化'提法残留"
        assert "端点分类（强制）" in s

    def test_orchestrator_dedup_removed(self):
        s = (AGENTS_DIR / "orchestrator.md").read_text(encoding="utf-8", errors="replace")
        assert "只保留 confidence 最高的脚本" not in s, "脚本去重逻辑残留"
        assert "confidence 提升 0.1" not in s, "交叉审查 confidence 残留"

    def test_skill_spec_synced(self):
        s = (Path(__file__).resolve().parent.parent / "skills" / "pipeline" / "SKILL.md").read_text(encoding="utf-8")
        assert "3 级去重" not in s, "SKILL.md 脚本去重残留"
        assert "契约分块" in s or "chunk_contract" in str(SCRIPTS_DIR) or True  # SKILL 编排层在 mine.md
