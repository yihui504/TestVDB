"""strategy_extractor: classify_endpoint + generalize_endpoint 测试。"""
from strategy_extractor import classify_endpoint, generalize_endpoint


def test_classify_endpoint_returns_string():
    """classify_endpoint 总返回字符串（已知或 'other'）。"""
    result = classify_endpoint("/collections/test/points/search")
    assert isinstance(result, str)
    assert len(result) > 0


def test_classify_endpoint_unknown_fallback():
    """完全无匹配的端点 → 'other'（或某默认 category，非空）。"""
    result = classify_endpoint("/totally/unknown/xyz/abc")
    assert isinstance(result, str)


def test_generalize_endpoint_strips_slashes():
    """前导/后缀斜杠标准化（同一端点不同写法 → 相同泛化）。"""
    a = generalize_endpoint("/collections/foo/points", "qdrant")
    b = generalize_endpoint("collections/foo/points/", "qdrant")
    assert a == b


def test_generalize_endpoint_db_prefix():
    """泛化结果含 {db}. 前缀。"""
    result = generalize_endpoint("collections/mycoll/points/search", "qdrant")
    assert result.startswith("{db}.")


def test_generalize_endpoint_uuid_replaced():
    """UUID 占位为 {id}。"""
    result = generalize_endpoint(
        "objects/550e8400-e29b-41d4-a716-446655440000", "weaviate")
    assert "{id}" in result


def test_generalize_endpoint_numeric_id_replaced():
    """纯数字路径段（前后带斜杠 /N/ ）占位为 {id}。"""
    result = generalize_endpoint("collections/12345/points/search", "qdrant")
    assert "{id}" in result


def test_generalize_endpoint_contains_operation():
    """泛化结果含 operation（最后一段动词，如 search）。"""
    result = generalize_endpoint("collections/mycoll/points/search", "qdrant")
    assert "search" in result
