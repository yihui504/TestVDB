"""preflight_contract_docs 纯函数分类测试（网络 IO 薄壳不测）。"""
import json

from preflight_contract_docs import (classify_http, classify_version,
                                     collect_urls, extract_version_token)


def test_version_token_slugs():
    assert extract_version_token(
        "https://api.qdrant.tech/v-1-18-x/api-reference/search/recommend-points.md") == (1, 18)
    assert extract_version_token(
        "https://milvus.io/api-reference/rest/v2.4.x/about.md") == (2, 4)
    # legacy 概念文档无版本路由 → None（no_version_in_url，PARTIAL 不判 FAIL）
    assert extract_version_token(
        "https://qdrant.tech/documentation/concepts/collections/") is None
    assert extract_version_token("") is None


def test_classify_http():
    assert classify_http(200) == "reachable"
    assert classify_http(301) == "reachable"
    assert classify_http(302) == "reachable"
    assert classify_http(404) == "dead"
    assert classify_http(410) == "dead"
    assert classify_http(500) == "unreachable"
    assert classify_http(403) == "unreachable"


def test_classify_version():
    assert classify_version((1, 18), "1.18.0") == "matched"
    assert classify_version((1, 18), "v1.18.0") == "matched"   # 真实契约带 v 前缀
    assert classify_version((1, 19), "1.18.0") == "mismatched"
    assert classify_version(None, "1.18.0") == "no_version_in_url"
    assert classify_version((1, 18), "") == "no_version_in_url"


def test_collect_urls_dedup_and_order():
    c = {"api_endpoints": [{"source_url": "https://a"}, {"source_url": "https://b"}],
         "constraints": {"type_constraints": [{"source_url": "https://a"}],
                          "state_constraints": [{"constraint_id": "x"}]}}
    assert collect_urls(c) == ["https://a", "https://b"]


def test_sidecar_roundtrip(tmp_path):
    sidecar = {"preflight_version": "P1.0",
               "results": {"https://a": {"http": "reachable",
                                          "version": "matched", "note": ""}}}
    p = tmp_path / "doc_preflight.json"
    p.write_text(json.dumps(sidecar, ensure_ascii=False, indent=1), encoding="utf-8")
    assert json.loads(p.read_text(encoding="utf-8"))["results"]["https://a"]["http"] == "reachable"
