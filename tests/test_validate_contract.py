"""validate_contract 通用契约验证器测试（批次 B2）。"""
import pytest

from validate_contract import validate_contract, get_endpoints, load_contract


def test_valid_contract_no_errors(make_contract):
    """完整 v2.0 契约 → 无 errors。"""
    _, contract = make_contract(target="weaviate")
    errors, _ = validate_contract(contract)
    assert errors == []


def test_missing_target_errors(make_contract):
    """缺 target → error。"""
    _, contract = make_contract(target="")
    errors, _ = validate_contract(contract)
    assert any("target" in e for e in errors)


def test_endpoint_missing_source_url_errors(make_contract):
    """endpoint 缺 source_url → error。"""
    _, contract = make_contract(endpoints=[
        {"path": "objects", "method": "POST", "category": "objects"},  # 缺 source_url
    ])
    errors, _ = validate_contract(contract)
    assert any("source_url" in e for e in errors)


def test_duplicate_constraint_ids_errors(make_contract):
    """重复 constraint_id → error。"""
    _, contract = make_contract(constraints={
        "type_constraints": [
            {"constraint_id": "c-1", "source_url": "u"},
            {"constraint_id": "c-1", "source_url": "u"},
        ]
    })
    errors, _ = validate_contract(contract)
    assert any("duplicate" in e.lower() for e in errors)


def test_missing_passport_warns_not_errors(make_contract):
    """无 _passport → warning 非 error（pre-v2.0 兼容）。"""
    _, contract = make_contract()
    errors, warnings = validate_contract(contract)
    assert errors == []
    assert any("_passport" in w for w in warnings)


def test_category_pollution_warns_for_weaviate(make_contract):
    """target=weaviate 但用 collections/points category → 警告（bug #3 检测侧）。"""
    _, contract = make_contract(target="weaviate", endpoints=[
        {"path": "objects", "method": "POST", "category": "objects",
         "source_url": "u", "doc_version": "1"},
        {"path": "coll", "method": "PUT", "category": "collections",
         "source_url": "u", "doc_version": "1"},
    ])
    errors, warnings = validate_contract(contract)
    assert errors == []
    assert any("污染" in w or "collections" in w for w in warnings)


def test_qdrant_category_not_pollution(make_contract):
    """target=qdrant 用 collections/points → 无污染警告（合法）。"""
    _, contract = make_contract(target="qdrant", endpoints=[
        {"path": "c", "method": "PUT", "category": "collections",
         "source_url": "u", "doc_version": "1"},
    ])
    _, warnings = validate_contract(contract)
    assert not any("污染" in w for w in warnings)


def test_legacy_single_api_endpoint_adapted():
    """legacy schema (api_endpoint 单数 list) 适配。"""
    contract = {
        "target": "weaviate", "version": "1.0",
        "api_endpoint": [{"path": "objects", "method": "POST", "category": "objects",
                          "source_url": "u", "doc_version": "1"}],
        "data_types": [{"name": "vector"}],
    }
    errors, _ = validate_contract(contract)
    assert errors == []


def test_get_endpoints_v2_and_legacy():
    """get_endpoints 适配 v2.0 (api_endpoints) + legacy (api_endpoint)。"""
    assert get_endpoints({"api_endpoints": [{"path": "a"}]}) == [{"path": "a"}]
    assert get_endpoints({"api_endpoint": [{"path": "b"}]}) == [{"path": "b"}]
    assert get_endpoints({}) == []


def test_load_contract_missing_file(tmp_path):
    """不存在文件 → (None, error)。"""
    contract, err = load_contract(str(tmp_path / "nope.json"))
    assert contract is None
    assert err is not None


def test_main_exit_codes(tmp_path):
    """main 退出码：pass=0 / errors=1 / 用法错=2。"""
    import sys
    from validate_contract import main

    # 用法错（argv 无 contract 路径）
    sys.argv = ["validate_contract.py"]
    assert main() == 2

    # pass（完整契约）
    good = tmp_path / "good.json"
    good.write_text('{"target":"weaviate","version":"1.0","api_endpoints":['
                    '{"path":"objects","method":"POST","category":"objects",'
                    '"source_url":"u","doc_version":"1"}],'
                    '"data_types":[{"name":"vector"}]}', encoding="utf-8")
    sys.argv = ["validate_contract.py", str(good)]
    assert main() == 0

    # errors（缺 target）
    bad = tmp_path / "bad.json"
    bad.write_text('{"version":"1.0"}', encoding="utf-8")
    sys.argv = ["validate_contract.py", str(bad)]
    assert main() == 1
