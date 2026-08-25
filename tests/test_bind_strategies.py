"""tests/test_bind_strategies.py — v3.4 D2 策略预绑定单测。

纯函数绑定逻辑 + CLI 文件端到端（tmp_path）。不依赖真实 strategy_registry。
"""
import json
import sys
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent.parent / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

import pytest  # noqa: E402

from bind_strategies import (  # noqa: E402
    LevelMissingError,
    _endpoint_matches,
    _strategy_binds,
    _type_tag,
    bind_contract,
    lint_levels,
    _main,
)


@pytest.fixture()
def registry():
    return {"strategies": [
        {
            "strategy_id": "boundary_type_invalid",
            "status": "active",
            "pattern": {"constraint_types": ["type"],
                        "applicable_endpoints": ["*+create", "collections+search"]},
            "performance": {"defects_found": 3},
        },
        {
            "strategy_id": "boundary_range_oob",
            "status": "stable",
            "pattern": {"constraint_types": ["range_constraint"],
                        "applicable_endpoints": ["*"]},
            "performance": {"defects_found": 5},
        },
        {
            "strategy_id": "semantic_experimental",
            "status": "experimental",
            "pattern": {"constraint_types": ["type"],
                        "applicable_endpoints": ["*+create"]},
            "performance": {"defects_found": 1},
        },
        {
            "strategy_id": "zero_track",
            "status": "active",
            "pattern": {"constraint_types": ["type"],
                        "applicable_endpoints": ["*+create"]},
            "performance": {"defects_found": 0},
        },
    ]}


@pytest.fixture()
def contract():
    return {
        "target": "qdrant",
        "constraints": {
            "type_constraints": [
                {"constraint_id": "t1", "endpoint": "collections+create",
                 "type": "type_constraint", "level": "endpoint"},
                {"constraint_id": "t2", "endpoint": "entities+upsert",
                 "type": "type_constraint", "level": "endpoint"},
            ],
            "range_constraints": [
                {"constraint_id": "r1", "endpoint": "entities+search",
                 "type": "range_constraint", "level": "endpoint"},
            ],
            "state_constraints": [
                {"constraint_id": "s1", "endpoint": "collections+delete",
                 "type": "state_constraint", "level": "system"},
            ],
        },
    }


@pytest.mark.unit
class TestMatchers:
    def test_type_tag(self):
        assert _type_tag("type_constraint") == "type"
        assert _type_tag("Range_Constraint") == "range"
        assert _type_tag("state") == "state"

    def test_endpoint_wildcard(self):
        assert _endpoint_matches("*+create", "collections+create")
        assert _endpoint_matches("*+create", "entities+create")
        assert not _endpoint_matches("*+create", "collections+delete")
        assert not _endpoint_matches("*+create", "create")
        assert _endpoint_matches("collections+search", "collections+search")
        assert not _endpoint_matches("collections+search", "collections+create")
        assert _endpoint_matches("*", "anything")

    def test_strategy_binds_gates(self, registry):
        s = registry["strategies"][0]
        assert _strategy_binds(s, "type", "collections+create")
        assert not _strategy_binds(s, "range", "collections+create")   # 类型不符
        assert not _strategy_binds(s, "type", "collections+delete")    # 端点不符
        assert not _strategy_binds(registry["strategies"][2], "type", "collections+create")  # experimental
        assert not _strategy_binds(registry["strategies"][3], "type", "collections+create")  # 零战绩


@pytest.mark.unit
class TestBindContract:
    def test_binding_and_skip(self, contract, registry):
        out = bind_contract(contract, [registry], ["global_strategies.json"])
        by_id = {c["constraint_id"]: c for g in ("type_constraints", "range_constraints",
                                                 "state_constraints")
                 for c in out["constraints"][g]}
        # t1: type + *+create → boundary_type_invalid（experimental/零战绩被门槛挡住）
        assert by_id["t1"]["bound_strategies"] == ["boundary_type_invalid"]
        # t2: type + entities+upsert → 无 *+upsert 匹配 → 空
        assert by_id["t2"]["bound_strategies"] == []
        # r1: range + * 通配 → boundary_range_oob
        assert by_id["r1"]["bound_strategies"] == ["boundary_range_oob"]
        # s1: system → 一律不绑
        assert by_id["s1"]["bound_strategies"] == []

    def test_summary_meta(self, contract, registry):
        out = bind_contract(contract, [registry], ["global_strategies.json"])
        meta = out["_strategy_binding"]
        assert meta["bound_constraints"] == 2
        assert meta["unbound_endpoint_constraints"] == 1
        assert meta["system_constraints_skipped"] == 1
        assert meta["distinct_strategies_bound"] == 2
        assert meta["registry_files"] == ["global_strategies.json"]

    def test_input_not_mutated(self, contract, registry):
        bind_contract(contract, [registry])
        for g in ("type_constraints", "range_constraints"):
            for c in contract["constraints"][g]:
                assert "bound_strategies" not in c
        assert "_strategy_binding" not in contract

    def test_missing_level_raises(self, contract, registry):
        contract["constraints"]["range_constraints"][0].pop("level")
        assert lint_levels(contract) == ["r1"]
        with pytest.raises(LevelMissingError) as e:
            bind_contract(contract, [registry])
        assert "r1" in e.value.missing_ids


@pytest.mark.unit
class TestCli:
    def test_roundtrip_and_dryrun(self, contract, registry, tmp_path,
                                  capsys, monkeypatch):
        cpath = tmp_path / "structured_contract.json"
        rpath = tmp_path / "global_strategies.json"
        cpath.write_text(json.dumps(contract), encoding="utf-8")
        rpath.write_text(json.dumps(registry), encoding="utf-8")

        # dry-run：不写回
        assert _main(["--registry-dir", str(tmp_path), "--dry-run", str(cpath)]) == 0
        raw = json.loads(cpath.read_text(encoding="utf-8"))
        assert "bound_strategies" not in raw["constraints"]["type_constraints"][0]

        # 正式：写回 + 汇总
        assert _main(["--registry-dir", str(tmp_path), str(cpath)]) == 0
        raw = json.loads(cpath.read_text(encoding="utf-8"))
        by_id = {c["constraint_id"]: c for g in ("type_constraints", "range_constraints",
                                                 "state_constraints")
                 for c in raw["constraints"][g]}
        assert by_id["t1"]["bound_strategies"] == ["boundary_type_invalid"]
        assert by_id["r1"]["bound_strategies"] == ["boundary_range_oob"]
        assert "_strategy_binding" in raw
        assert "written" in capsys.readouterr().out

        # lint 失败：exit 1 且文件零写入（保持原样）
        raw["constraints"]["range_constraints"][0].pop("level")
        cpath.write_text(json.dumps(raw), encoding="utf-8")
        before = cpath.read_text(encoding="utf-8")
        assert _main(["--registry-dir", str(tmp_path), str(cpath)]) == 1
        assert cpath.read_text(encoding="utf-8") == before
