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


def test_non_standard_category_warns(make_contract):
    """非通用词表 category（如 collections/points）→ 警告，target 无关（bug #3 检测侧）。"""
    _, contract = make_contract(target="weaviate", endpoints=[
        {"path": "objects", "method": "POST", "category": "data",
         "source_url": "u", "doc_version": "1"},
        {"path": "coll", "method": "PUT", "category": "collections",  # 非通用词
         "source_url": "u", "doc_version": "1"},
    ])
    errors, warnings = validate_contract(contract)
    assert errors == []
    assert any("非通用词表" in w or "collections" in w for w in warnings)


def test_qdrant_non_standard_category_also_warns(make_contract):
    """新逻辑：collections 是 DB 资源名非功能 category，即使 target=qdrant 也警告。"""
    _, contract = make_contract(target="qdrant", endpoints=[
        {"path": "c", "method": "PUT", "category": "collections",  # 非通用词
         "source_url": "u", "doc_version": "1"},
    ])
    _, warnings = validate_contract(contract)
    assert any("非通用词表" in w or "collections" in w for w in warnings)


def test_standard_categories_no_category_warning(make_contract):
    """全通用词表 category（schema/data/...）→ 无 category 警告。"""
    _, contract = make_contract(target="qdrant", endpoints=[
        {"path": "c", "method": "PUT", "category": "schema",
         "source_url": "u", "doc_version": "1"},
        {"path": "p", "method": "POST", "category": "data",
         "source_url": "u", "doc_version": "1"},
    ])
    errors, warnings = validate_contract(contract)
    assert errors == []
    assert not any("非通用词表" in w for w in warnings)


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


def test_endpoint_completeness_warns_when_few(tmp_path):
    """[5] 契约端点少 vs raw_knowledge 路径多 → 警告。"""
    from validate_contract import check_endpoint_completeness
    raw = tmp_path / "raw_knowledge.md"
    raw.write_text(
        "POST /v1/objects\nGET /v1/schema\nPOST /v1/graphql\nDELETE /v1/objects/{id}\n",
        encoding="utf-8")
    # 契约 1 端点 vs raw 多路径 → 1 < N*0.5 → warning
    warn = check_endpoint_completeness(1, str(raw))
    assert warn is not None
    assert "完整度" in warn


def test_endpoint_completeness_ok_when_sufficient(tmp_path):
    """[5] 契约端点 >= raw 路径*0.5 → 无警告。"""
    from validate_contract import check_endpoint_completeness
    raw = tmp_path / "raw_knowledge.md"
    raw.write_text("POST /v1/objects\nGET /v1/schema\n", encoding="utf-8")
    # 契约 2 端点 vs raw 2 路径 → 2 >= 1 → None
    warn = check_endpoint_completeness(2, str(raw))
    assert warn is None


def test_endpoint_completeness_skip_when_no_raw(tmp_path):
    """[5] 无 raw_knowledge → None（跳过，不报错）。"""
    from validate_contract import check_endpoint_completeness
    warn = check_endpoint_completeness(10, str(tmp_path / "nope.md"))
    assert warn is None


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
                    '{"path":"objects+create","method":"POST","category":"objects",'
                    '"source_url":"u","doc_version":"1"}],'
                    '"data_types":[{"name":"vector"}]}', encoding="utf-8")
    sys.argv = ["validate_contract.py", str(good)]
    assert main() == 0

    # errors（缺 target）
    bad = tmp_path / "bad.json"
    bad.write_text('{"version":"1.0"}', encoding="utf-8")
    sys.argv = ["validate_contract.py", str(bad)]
    assert main() == 1


# ═══════════════════════════════════════════════════════════════
# 规则 2.10（2026-09-01）：功能点 ID 粒度与形式
# ═══════════════════════════════════════════════════════════════

def _ep(path, method="GET", category="data"):
    return {"path": path, "method": method, "category": category,
            "source_url": "https://example.test/docs/a", "doc_version": "1.0"}


def test_path_form_rule210_valid_forms(make_contract):
    """规则 2.10:合规形式(资源型多段 + 根级运维单段)→ 无 2.10 error。"""
    _, contract = make_contract(endpoints=[
        _ep("points+recommend", "POST", "search"),
        _ep("points+query+groups", "POST", "search"),
        _ep("healthz", "GET", "admin"),
    ])
    errors, _ = validate_contract(contract)
    assert not any("规则 2.10" in e for e in errors)


def test_path_form_rule210_invalid_forms(make_contract):
    """规则 2.10:下划线连接/斜杠/大写/路径参数 → 全部 error(实验中 A 系列漂移形态)。"""
    _, contract = make_contract(endpoints=[
        _ep("points_recommend"),       # 下划线连接(A 系列实测漂移形态)
        _ep("collections/create"),     # 斜杠
        _ep("Points+Get"),             # 大写
        _ep("points/{id}"),            # 路径参数残留
    ])
    errors, _ = validate_contract(contract)
    hits = [e for e in errors if "规则 2.10" in e]
    assert hits and "4 个" in hits[0]


def test_ref_contract_gone_ids_errors(tmp_path):
    """规则 2.10 G-d:--ref-contract 旧 ID 消失(改名/删除)→ error。"""
    import json as _json
    import sys
    from validate_contract import main

    old = tmp_path / "old.json"
    old.write_text(_json.dumps({
        "target": "t", "version": "1.0",
        "api_endpoints": [
            {"path": "points+search", "method": "POST", "category": "search", "source_url": "u"},
            {"path": "points+scroll", "method": "POST", "category": "search", "source_url": "u"},
        ],
        "data_types": [], "_passport": {}}), encoding="utf-8")
    new = tmp_path / "new.json"
    new.write_text(_json.dumps({
        "target": "t", "version": "1.1",
        "api_endpoints": [
            {"path": "points+query", "method": "POST", "category": "search", "source_url": "u"},
        ],
        "data_types": [], "_passport": {}}), encoding="utf-8")
    sys.argv = ["validate_contract.py", str(new), "--ref-contract", str(old)]
    assert main() == 1


def test_ref_contract_superset_passes(tmp_path):
    """规则 2.10 G-d:新增 ID 合法(旧 ID 全保留)→ 不因 ref 报 error。"""
    import json as _json
    import sys
    from validate_contract import main

    old = tmp_path / "old.json"
    old.write_text(_json.dumps({
        "target": "t", "version": "1.0",
        "api_endpoints": [
            {"path": "points+search", "method": "POST", "category": "search", "source_url": "u"},
        ],
        "data_types": [], "_passport": {}}), encoding="utf-8")
    new = tmp_path / "new.json"
    new.write_text(_json.dumps({
        "target": "t", "version": "1.1",
        "api_endpoints": [
            {"path": "points+search", "method": "POST", "category": "search", "source_url": "u"},
            {"path": "points+search+batch", "method": "POST", "category": "search", "source_url": "u"},
        ],
        "data_types": [], "_passport": {}}), encoding="utf-8")
    sys.argv = ["validate_contract.py", str(new), "--ref-contract", str(old)]
    assert main() == 0
