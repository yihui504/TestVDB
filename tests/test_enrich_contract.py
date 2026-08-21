"""enrich_contract_from_spec 回归测试（rerun pilot 2026-08-21 机制化）.

覆盖:
- spec_op_for: 五类契约↔spec 匹配形态（动词后缀剥除/{param} 占位/段对齐）+ 反例
- enrich: 空参端点回填 / fill_missing_fields 补漏 / LLM 参数保留
- rehash: passport 重签
"""
from __future__ import annotations

import sys
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent.parent / "scripts"
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

from enrich_contract_from_spec import enrich, rehash, spec_op_for  # noqa: E402

SPEC_PATHS = {
    "/collections/{collection_name}/exists": {"get": {"parameters": [{"name": "collection_name"}]}},
    "/collections/aliases": {"post": {"requestBody": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/AliasOps"}}}}}},
    "/collections": {"get": {}},
    "/cluster/peer/{peer_id}": {"delete": {"parameters": [{"name": "peer_id"}]}},
    "/collections/{collection_name}/points/query": {"post": {"requestBody": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/QueryRequest"}}}}}},
    "/collections/{collection_name}/points/query/groups": {"post": {}},
}

SCHEMAS = {
    "AliasOps": {"properties": {"actions": {"type": "array"}}, "required": ["actions"]},
    "QueryRequest": {"properties": {
        "query": {"type": "object", "$ref": "#/components/schemas/Nested"},
        "limit": {"type": "integer"},
    }, "required": ["query"]},
    "Nested": {"properties": {"neerste": {"type": "integer"}, "farthest": {"type": "integer"}}},
}


class TestSpecOpFor:
    def test_plain_suffix(self):
        assert spec_op_for({"path": "points+query", "method": "POST"}, SPEC_PATHS) is not None

    def test_verb_suffix_stripped(self):
        """collections+list ↔ /collections GET（list 是消歧后缀，spec 无此段）。"""
        assert spec_op_for({"path": "collections+list", "method": "GET"}, SPEC_PATHS) is not None

    def test_verb_to_param_placeholder(self):
        """cluster+peer+delete ↔ /cluster/peer/{peer_id} DELETE。"""
        assert spec_op_for({"path": "cluster+peer+delete", "method": "DELETE"}, SPEC_PATHS) is not None

    def test_mid_path_verb(self):
        assert spec_op_for({"path": "collections+exists", "method": "GET"}, SPEC_PATHS) is not None

    def test_empty_op_not_matched_and_no_crash(self):
        """空 dict op（{}）不等于缺失——曾因 falsy 判定把所有 {} op 全跳过。"""
        paths = {"/x": {"post": {}}}
        assert spec_op_for({"path": "x", "method": "POST"}, paths) == {}

    def test_no_cross_match(self):
        """points+query 不得误配 query/groups；无关 path 不误配。"""
        assert spec_op_for({"path": "collections+facet", "method": "POST"},
                           {"/collections/{collection_name}/points/query": {"post": {}}}) is None


class TestEnrich:
    def test_backfill_empty_params(self):
        contract = {"api_endpoints": [
            {"path": "collections+aliases+update", "method": "POST", "parameters": []},
        ]}
        out, n_eps, n_fields = enrich(contract, {"paths": SPEC_PATHS, "components": {"schemas": SCHEMAS}})
        assert n_eps == 1
        names = [p["name"] for p in out["api_endpoints"][0]["parameters"]]
        assert "actions" in names

    def test_fill_missing_fields_appends_not_replaces(self):
        contract = {"api_endpoints": [
            {"path": "points+query", "method": "POST",
             "parameters": [{"name": "limit", "type": "int", "source": "doc"}]},
        ]}
        out, _, n_fields = enrich(contract, {"paths": SPEC_PATHS, "components": {"schemas": SCHEMAS}},
                                   fill_missing_fields=True)
        params = out["api_endpoints"][0]["parameters"]
        names = [p["name"] for p in params]
        assert names.count("limit") == 1  # 不重复
        assert "query" in names  # spec 补的
        assert next(p for p in params if p["name"] == "limit")["source"] == "doc"  # LLM 的保留

    def test_source_marked_mechanical(self):
        contract = {"api_endpoints": [{"path": "cluster+peer+delete", "method": "DELETE", "parameters": []}]}
        out, _, _ = enrich(contract, {"paths": SPEC_PATHS, "components": {"schemas": SCHEMAS}})
        p = out["api_endpoints"][0]["parameters"][0]
        assert p["source"] == "openapi (mechanical backfill)"

    def test_subfields_one_level(self):
        contract = {"api_endpoints": [{"path": "points+query", "method": "POST", "parameters": []}]}
        out, _, _ = enrich(contract, {"paths": SPEC_PATHS, "components": {"schemas": SCHEMAS}})
        q = next(p for p in out["api_endpoints"][0]["parameters"] if p["name"] == "query")
        assert "sub_fields" in q and "neerste" in q["sub_fields"]


class TestRehash:
    def test_rehash_matches_verify_algorithm(self):
        import hashlib, json
        contract = {"target": "t", "_passport": {"contract_hash": "sha256:old"}}
        rehash(contract)
        d = {k: v for k, v in contract.items() if k != "_passport"}
        expect = "sha256:" + hashlib.sha256(json.dumps(d, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        assert contract["_passport"]["contract_hash"] == expect
