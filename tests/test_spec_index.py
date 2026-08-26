"""tests/test_spec_index.py — spec 索引单测（D3b ground truth，v3.4）。

用最小内联 spec 夹具验证：$ref/anyOf/items 递归、required_paths 扁平化、
lattice 生成、端点匹配、判别键推导、深度截断防御、确定性。
不依赖真实 qdrant spec（集成验证走 CLI + .sourcedeps 真实文件）。
"""
import json
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent.parent / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import pytest  # noqa: E402

import spec_index as si  # noqa: E402


@pytest.fixture
def mini_spec(tmp_path):
    """覆盖 qdrant 两靶标形态的最小 spec。"""
    spec = {
        "openapi": "3.0.1",
        "paths": {
            "/collections/{collection_name}/exists": {
                "get": {
                    "operationId": "collection_exists",
                    "responses": {"200": {"content": {"application/json": {"schema": {
                        "type": "object",
                        "properties": {
                            "result": {"$ref": "#/components/schemas/CollectionExistence"},
                            "status": {"type": "string"},
                        },
                    }}}}},
                }
            },
            "/collections/{collection_name}/points": {
                "put": {
                    "operationId": "upsert_points",
                    "requestBody": {"content": {"application/json": {"schema": {
                        "$ref": "#/components/schemas/PointInsertOperations"}}}},
                    "responses": {"200": {"content": {"application/json": {"schema": {
                        "type": "object", "properties": {"result": {"type": "object"}}}}}}},
                }
            },
        },
        "components": {"schemas": {
            "CollectionExistence": {
                "type": "object", "required": ["exists"],
                "properties": {"exists": {"type": "boolean"}},
            },
            "PointInsertOperations": {
                "anyOf": [{"$ref": "#/components/schemas/PointsList"},
                          {"$ref": "#/components/schemas/Batch"}],
            },
            "PointsList": {
                "type": "object", "required": ["points"],
                "properties": {"points": {"type": "array", "items": {
                    "$ref": "#/components/schemas/PointStruct"}}},
            },
            "Batch": {"type": "object", "required": ["ids", "vectors"],
                      "properties": {"ids": {"type": "array"},
                                     "vectors": {"type": "array"}}},
            "PointStruct": {
                "type": "object", "required": ["id", "vector"],
                "properties": {"id": {"type": "integer"},
                               "vector": {"type": "object"},
                               "payload": {"type": "object"}},
            },
        }},
    }
    p = tmp_path / "openapi.json"
    p.write_text(json.dumps(spec), encoding="utf-8")
    return p


@pytest.mark.unit
class TestBuildIndex:
    def test_exists_lattice(self, mini_spec):
        idx = si.build_index(str(mini_spec), db="t", version="v1")
        lat = idx["endpoints"]["GET /collections/{collection_name}/exists"] \
            ["responses"]["200"]["shape_lattice"]
        assert lat["result"] == "object"
        assert lat["result.exists"] == "boolean"

    def test_upsert_required_paths_nested(self, mini_spec):
        idx = si.build_index(str(mini_spec), db="t", version="v1")
        rp = idx["endpoints"]["PUT /collections/{collection_name}/points"] \
            ["request"]["required_paths"]
        # semantic_004 靶标：points[].id / points[].vector（2 层 $ref + anyOf + items）
        assert "points[].id" in rp and "points[].vector" in rp

    def test_discriminator_keys(self, mini_spec):
        idx = si.build_index(str(mini_spec), db="t", version="v1")
        tree = idx["endpoints"]["PUT /collections/{collection_name}/points"] \
            ["request"]["required_tree"]
        alts = tree["alternatives"]
        sel = {a.get("selector_key") for a in alts}
        assert sel == {"points", "ids"}  # 各分支 required 独有键即判别键

    def test_determinism(self, mini_spec):
        a = si.build_index(str(mini_spec), db="t", version="v1")
        b = si.build_index(str(mini_spec), db="t", version="v1")
        assert json.dumps(a, sort_keys=True) == json.dumps(b, sort_keys=True)


@pytest.mark.unit
class TestMatchEndpoint:
    def test_literal_and_var_segments(self, mini_spec):
        idx = si.build_index(str(mini_spec), db="t", version="v1")
        k = si.match_endpoint("GET", "/collections/mycoll/exists", idx)
        assert k == "GET /collections/{collection_name}/exists"
        # f-string 变量段同样可绑
        k2 = si.match_endpoint("PUT", f"/collections/{'c1'}/points", idx)
        assert k2 == "PUT /collections/{collection_name}/points"

    def test_normalize_strips_host_query(self, mini_spec):
        idx = si.build_index(str(mini_spec), db="t", version="v1")
        k = si.match_endpoint("GET", "http://localhost:6333/collections/x/exists?timeout=5", idx)
        assert k == "GET /collections/{collection_name}/exists"

    def test_no_match_returns_none(self, mini_spec):
        idx = si.build_index(str(mini_spec), db="t", version="v1")
        assert si.match_endpoint("POST", "/nope", idx) is None


@pytest.mark.unit
class TestDefenses:
    def test_depth_cap(self, tmp_path):
        # 自递归 schema（Filter 内嵌 Filter）——必须截断不崩溃
        spec = {
            "openapi": "3.0.1",
            "paths": {"/f": {"post": {
                "requestBody": {"content": {"application/json": {"schema": {
                    "$ref": "#/components/schemas/Node"}}}}}}},
            "components": {"schemas": {"Node": {
                "type": "object", "required": ["v"],
                "properties": {"v": {"type": "integer"},
                               "child": {"$ref": "#/components/schemas/Node"}}}}},
        }
        p = tmp_path / "openapi.json"
        p.write_text(json.dumps(spec), encoding="utf-8")
        idx = si.build_index(str(p), db="t", version="v1")
        tree = idx["endpoints"]["POST /f"]["request"]["required_tree"]
        assert tree["depth_capped"] or "recursive" in tree.get("children", {}).get("child", {})

    def test_empty_spec(self, tmp_path):
        p = tmp_path / "openapi.json"
        p.write_text("{}", encoding="utf-8")
        idx = si.build_index(str(p), db="t", version="v1")
        assert idx["endpoints"] == {}
