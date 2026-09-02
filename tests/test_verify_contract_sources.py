"""verify_contract_sources 的 verify_constraint 测试（六组覆盖补洞 2026-09-02）。

与 _validate_contract 同构空洞修复：resource_bound/doc_consistency/other 此前
不被复检；doc_consistency 双源模式 = 任一冲突侧命中即 verified。
"""
import pytest

from verify_contract_sources import verify_constraint


def _doccons(assertion="doc-internal conflict: spec says 10000, prose says 20000 "
                        "— behavior follows implementation, either side may be violated",
             cid="qdrant_doccons_indexing_threshold_001"):
    return {"constraint_id": cid, "description": "doc-internal conflict",
            "assertion": assertion, "evidence_tier": "explicit",
            "type": "doc_consistency"}


def test_numeric_all_found_verified():
    c = {"constraint_id": "qdrant_range_search_001", "description": "limit positive",
         "assertion": "limit <= 16384"}
    r = verify_constraint(c, "the limit cannot exceed 16384 entries")
    assert r["verified"] is True


def test_numeric_missing_false_strict():
    c = {"constraint_id": "qdrant_range_search_001", "description": "limit positive",
         "assertion": "limit <= 16384"}
    r = verify_constraint(c, "no numbers in this source")
    assert r["verified"] is False
    assert "16384" in r["reason"]


def test_unreachable_neutral_false():
    c = {"constraint_id": "qdrant_range_search_001", "description": "limit positive",
         "assertion": "limit <= 16384"}
    r = verify_constraint(c, None)
    assert r["verified"] is False
    assert "unreachable" in r["reason"]


def test_doccons_one_side_found_verified():
    """双源冲突:source 含一侧数值 → verified(防误报 hallucination)。"""
    r = verify_constraint(_doccons(), "default is 10000 per the spec comment",
                          allow_partial_numeric=True)
    assert r["verified"] is True


def test_doccons_other_side_found_verified():
    """另一侧数值在 source → verified。"""
    r = verify_constraint(_doccons(), "documented value reads 20000 here",
                          allow_partial_numeric=True)
    assert r["verified"] is True


def test_doccons_all_numeric_missing_false():
    """两侧数值都不在 source → 全部缺失 = hallucination 嫌疑。"""
    r = verify_constraint(_doccons(), "no numbers at all", allow_partial_numeric=True)
    assert r["verified"] is False
    assert "all_numeric_keywords_missing" in r["reason"]


def test_strict_mode_drops_partial_miss():
    """默认严格模式:doc_consistency 缺任一数值仍 False(行为不变)。"""
    r = verify_constraint(_doccons(), "default is 10000 only")
    assert r["verified"] is False


def test_no_keywords_neutral():
    c = {"constraint_id": "x", "description": "", "assertion": ""}
    r = verify_constraint(c, "whatever")
    assert r["verified"] is None
