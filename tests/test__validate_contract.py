"""_validate_contract 的 evidence_tier 一致性检查测试（R16 机制补洞 2026-09-02）。

背景：批 1 REFACTOR 声称 "残余前缀滑档交机械 gate"，但全仓无脚本检查
evidence_tier 枚举 / inferred: 前缀——空头承诺。本测试锁定 check_tier_consistency。
"""
import pytest

from _validate_contract import (
    check_tier_consistency,
    TIER_GROUP_NAMES,
)


def _constraint(tier="explicit", desc="limit must be positive", cid="qdrant_range_search_001"):
    return {"constraint_id": cid, "description": desc, "evidence_tier": tier}


def _assertion(tier="inferred", desc="inferred: empty collection behavior", aid="qdrant_behavioral_search_001"):
    return {"assertion_id": aid, "description": desc, "evidence_tier": tier}


@pytest.fixture
def full_contract():
    """六组 + assertions 全覆盖的合规契约。"""
    return {
        "target": "qdrant", "version": "1.18.0",
        "constraints": {
            "type_constraints": [_constraint(cid="qdrant_type_upsert_001")],
            "range_constraints": [_constraint(cid="qdrant_range_search_001")],
            "state_constraints": [_constraint(tier="inferred", desc="inferred: delete-gone visibility",
                                              cid="qdrant_state_delete_001")],
            "resource_bound_constraints": [_constraint(tier="inferred",
                                                       desc="inferred: server must gracefully handle any spec-legal shard_number",
                                                       cid="qdrant_resource_shard_number_001")],
            "doc_consistency_constraints": [_constraint(cid="qdrant_doccons_indexing_001")],
            "other_constraints": [_constraint(tier="inferred", desc="inferred: response ids strictly increasing",
                                              cid="qdrant_other_scroll_001")],
        },
        "assertions": [_assertion()],
    }


def test_all_compliant_no_failures(full_contract):
    """六组 + assertions 全合规 → 零 failure。"""
    assert check_tier_consistency(full_contract) == []


def test_tier_missing_fails(full_contract):
    """缺 evidence_tier → tier_missing（schema required）。"""
    c = full_contract
    c["constraints"]["range_constraints"][0].pop("evidence_tier")
    fails = check_tier_consistency(c)
    assert len(fails) == 1
    assert fails[0]["check"] == "tier_missing"
    assert fails[0]["constraint_id"] == "qdrant_range_search_001"


def test_legacy_tier_value_rejected(full_contract):
    """旧值 inferred_from_behavior（chroma/legacy 产物仍在产出）→ 打回。"""
    c = full_contract
    c["constraints"]["state_constraints"][0]["evidence_tier"] = "inferred_from_behavior"
    fails = check_tier_consistency(c)
    assert len(fails) == 1
    assert fails[0]["check"] == "tier_value_invalid"
    assert "inferred_from_behavior" in fails[0]["detail"]


def test_inferred_without_prefix_fails(full_contract):
    """批 1 实测残余滑档：tier=inferred 但 description 无 inferred: 前缀 → 打回。"""
    c = full_contract
    c["constraints"]["state_constraints"][0]["description"] = "delete-gone visibility"
    fails = check_tier_consistency(c)
    assert len(fails) == 1
    assert fails[0]["check"] == "missing_inferred_prefix"


def test_inferred_prefix_without_content_fails(full_contract):
    """description 恰为 "inferred:"（前缀后无内容）→ 打回。"""
    c = full_contract
    c["constraints"]["state_constraints"][0]["description"] = "inferred:"
    fails = check_tier_consistency(c)
    assert len(fails) == 1
    assert fails[0]["check"] == "missing_inferred_prefix"


def test_explicit_with_stray_prefix_fails(full_contract):
    """反向漂移：tier=explicit 但 description 以 inferred: 开头（降级后忘改 tier）→ 打回。"""
    c = full_contract
    c["constraints"]["range_constraints"][0]["description"] = "inferred: limit positivity"
    fails = check_tier_consistency(c)
    assert len(fails) == 1
    assert fails[0]["check"] == "stray_inferred_prefix"


def test_assertions_checked_too(full_contract):
    """assertions 数组同样受检（Rule 3 覆盖 assertions）。"""
    c = full_contract
    c["assertions"][0]["description"] = "no prefix here"
    fails = check_tier_consistency(c)
    assert len(fails) == 1
    assert fails[0]["check"] == "missing_inferred_prefix"
    assert fails[0]["constraint_id"] == "qdrant_behavioral_search_001"


def test_every_failure_carries_reference_ids(full_contract):
    """每个 failure 都带 constraint_id（供失败列表定位）。"""
    c = full_contract
    c["constraints"]["type_constraints"][0]["evidence_tier"] = "made_up"
    c["constraints"]["range_constraints"][0].pop("evidence_tier")
    c["constraints"]["state_constraints"][0]["description"] = "no prefix"
    c["constraints"]["resource_bound_constraints"][0]["description"] = "only prefix once"  # inferred 无前缀
    c["assertions"][0]["evidence_tier"] = "explicit"  # 降级后忘改 tier 的反向漂移
    c["assertions"][0]["description"] = "inferred: stray on explicit"
    fails = check_tier_consistency(c)
    assert len(fails) == 5
    for f in fails:
        assert f["constraint_id"]


def test_group_names_cover_all_six():
    """TIER_GROUP_NAMES 覆盖六组——防未来加组忘同步检查。"""
    assert set(TIER_GROUP_NAMES) == {
        "type_constraints", "range_constraints", "state_constraints",
        "resource_bound_constraints", "doc_consistency_constraints", "other_constraints",
    }
