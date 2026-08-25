"""tests/test_runtime_qdrant.py — qdrant runtime 单元测试。

mock `req` 验证 path_params 替换 + setup_default 失败 + PATHS 模板完整性。
不连真实 qdrant。
"""
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent.parent / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import pytest  # noqa: E402

from runtime import _common, qdrant  # noqa: E402


@pytest.mark.unit
class TestPathsTemplates:
    def test_all_paths_contain_name_except_nameless(self):
        for k, v in qdrant.PATHS.items():
            if k in qdrant._NAMELESS:
                assert "{name}" not in v
            else:
                assert "{name}" in v, f"{k}={v!r} 缺 {{name}}"

    def test_required_keys_present(self):
        must = {"create_collection", "drop_collection", "upsert_points",
                "search", "query", "count"}
        assert must <= set(qdrant.PATHS), f"missing: {must - set(qdrant.PATHS)}"

    def test_distance_map(self):
        assert qdrant.DISTANCE_MAP["Cosine"] == "Cosine"
        assert qdrant.DISTANCE_MAP["Euclid"] == "Euclidean"
        assert qdrant.DISTANCE_MAP["Dot"] == "Dot"


@pytest.mark.unit
class TestRequestPathParams:
    def test_bad_path_key_raises(self):
        with pytest.raises(KeyError, match="path_key"):
            qdrant.request("POST", "nonexistent_key", {})

    def test_path_params_replaces_name(self, monkeypatch):
        captured = {}

        def fake_req(base, method, path, body=None, timeout=30):
            captured["path"] = path
            captured["method"] = method
            captured["body"] = body
            return 200, '{"result":[]}'

        monkeypatch.setattr("runtime.qdrant.req", fake_req)
        qdrant.request("POST", "search",
                       {"vector": [0.1] * 4, "limit": 3},
                       path_params={"name": "test_coll"})
        assert captured["path"] == "/collections/test_coll/points/search"
        assert captured["method"] == "POST"

    def test_missing_path_param_raises(self, monkeypatch):
        def fake_req(*a, **k):
            return 200, '{}'

        monkeypatch.setattr("runtime.qdrant.req", fake_req)
        with pytest.raises(KeyError):
            qdrant.request("POST", "search", {},
                           path_params={"wrong_param": "x"})


@pytest.mark.unit
class TestSetupDefault:
    def test_unsupported_metric_returns_false(self):
        ok, err = qdrant.setup_default("c", 128, metric="INVALID")
        assert ok is False
        assert "unsupported metric" in err

    def test_create_fail_returns_false(self, monkeypatch):
        def fake_req(base, method, path, body=None, timeout=30):
            return 500, '{"status":"error","detail":"internal"}'

        monkeypatch.setattr("runtime.qdrant.req", fake_req)
        ok, err = qdrant.setup_default("c", 128)
        assert ok is False
        assert "create 500" in err

    def test_create_success(self, monkeypatch):
        captured = {}

        def fake_req(base, method, path, body=None, timeout=30):
            captured["body"] = body
            return 200, '{}'

        monkeypatch.setattr("runtime.qdrant.req", fake_req)
        ok, err = qdrant.setup_default("c", 128, metric="Cosine")
        assert ok is True and err == ""
        assert captured["body"]["vectors"]["size"] == 128
        assert captured["body"]["vectors"]["distance"] == "Cosine"

    def test_409_idempotent(self, monkeypatch):
        def fake_req(base, method, path, body=None, timeout=30):
            return 409, '{"status":"already exists"}'

        monkeypatch.setattr("runtime.qdrant.req", fake_req)
        ok, _ = qdrant.setup_default("c", 128)
        assert ok is True


@pytest.mark.unit
class TestDropCollection:
    def test_never_raises_on_error(self, monkeypatch):
        def boom(*a, **k):
            raise RuntimeError("network gone")

        monkeypatch.setattr("runtime.qdrant.req", boom)
        qdrant.drop_collection("any")


@pytest.mark.unit
class TestCommonExpectRecords:
    def test_qdrant_style_result_list(self):
        assert _common.expect_records(200, '{"result":[1,2,3]}', expected_min=3) == "NO_DEFECT"
        assert _common.expect_records(200, '{"result":[]}', expected_min=1) == "DEFECT_FOUND"
        assert _common.expect_records(200, '{"result":[1]}', expected_min=2) == "DEFECT_FOUND"

    def test_4xx_is_script_error(self):
        assert _common.expect_records(400, '{"status":"error"}', expected_min=1) == "SCRIPT_ERROR"
        assert _common.expect_records(422, '', expected_min=1) == "SCRIPT_ERROR"

    def test_5xx_and_setup_fail_are_script_error(self):
        assert _common.expect_records(500, '', expected_min=1) == "SCRIPT_ERROR"
        assert _common.expect_records(200, '{"result":[1]}', expected_min=1, setup_ok=False) == "SCRIPT_ERROR"

    def test_non_json_is_script_error(self):
        assert _common.expect_records(200, "not json", expected_min=1) == "SCRIPT_ERROR"

    def test_200_no_list_field_is_no_defect(self):
        assert _common.expect_records(200, '{"status":"ok"}', expected_min=1) == "NO_DEFECT"


@pytest.mark.unit
class TestGetRuntimeDispatch:
    def test_qdrant_dispatch(self, monkeypatch):
        monkeypatch.setenv("TESTVDB_TARGET", "qdrant")
        from runtime import get_runtime
        rt = get_runtime()
        assert rt.__name__ == "runtime.qdrant"
        for attr in ("PATHS", "request", "setup_default", "drop_collection",
                     "judge_4xx", "judge_200", "expect_records", "expect_rejected"):
            assert hasattr(rt, attr), f"qdrant runtime missing {attr}"

    def test_unsupported_target_lists_qdrant(self, monkeypatch):
        monkeypatch.setenv("TESTVDB_TARGET", "postgres")
        from runtime import get_runtime
        with pytest.raises(RuntimeError, match="implemented: milvus, qdrant"):
            get_runtime()


@pytest.mark.unit
class TestJudgeSchemaAttackQdrant:
    """judge_schema_attack qdrant 版 — describe_collection 回读三态判定。
    跨 target 一致接口（同 weaviate.judge_schema_attack）；qdrant 适配 {result:{...}} 嵌套。"""

    def _stub_request(self, monkeypatch, describe_status=200, describe_body=None):
        import json as _j
        def fake_req(method, path_key, body=None, path_params=None, timeout=30):
            return describe_status, _j.dumps(describe_body) if describe_body else "{}"
        monkeypatch.setattr("runtime.qdrant.request", fake_req)

    def test_type1_persisted(self, monkeypatch):
        self._stub_request(monkeypatch, describe_body={"result": {"opt": -1}})
        v = qdrant.judge_schema_attack(200, "", "X", ["opt"], -1)
        assert v == "DEFECT_FOUND"

    def test_silent_drop_no_defect(self, monkeypatch):
        # 字段被 silent-drop（None）→ NO_DEFECT，避免 false positive
        self._stub_request(monkeypatch, describe_body={"result": {}})
        v = qdrant.judge_schema_attack(200, "", "X", ["opt"], -1)
        assert v == "NO_DEFECT"

    def test_correct_reject_422(self, monkeypatch):
        v = qdrant.judge_schema_attack(422, "", "X", ["opt"], -1)
        assert v == "NO_DEFECT"

    def test_describe_failure_script_error(self, monkeypatch):
        self._stub_request(monkeypatch, describe_status=500, describe_body={})
        v = qdrant.judge_schema_attack(200, "", "X", ["opt"], -1)
        assert v == "SCRIPT_ERROR"

    def test_nested_path_persisted(self, monkeypatch):
        self._stub_request(monkeypatch, describe_body={"result": {"a": {"b": 0}}})
        v = qdrant.judge_schema_attack(200, "", "X", ["a", "b"], 0)
        assert v == "DEFECT_FOUND"

    def test_result_missing_script_error(self, monkeypatch):
        # describe 返回无 result 字段 → SCRIPT_ERROR
        self._stub_request(monkeypatch, describe_body={"foo": "bar"})
        v = qdrant.judge_schema_attack(200, "", "X", ["opt"], -1)
        assert v == "SCRIPT_ERROR"
