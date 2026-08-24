# -*- coding: utf-8 -*-
"""ADR-0009 §6 confirm_per_round 配置开关（TDD：测试先行，RED -> GREEN）。

设计输入：docs/adr/0009-two-phase-exploration-and-exploratory-channel.md
- settings.json 新增 mining.confirm_per_round（默认 true = 产品行为不变）
- contracts/settings_schema.json 同步 mining 节（布尔类型校验）
- scripts/get_setting.py 提供 dot-path 机械查询（orchestrator Bash 调用，
  确定性读取开关，替代 LLM 目测 settings）
"""
import json
import os
import subprocess
import sys

import pytest

PLUGIN_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SETTINGS_PATH = os.path.join(PLUGIN_ROOT, "settings.json")
SCHEMA_PATH = os.path.join(PLUGIN_ROOT, "contracts", "settings_schema.json")
GET_SETTING = os.path.join(PLUGIN_ROOT, "scripts", "get_setting.py")


def _load(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


class TestMiningConfigSchema:
    def test_settings_default_confirm_per_round_true(self):
        settings = _load(SETTINGS_PATH)
        mining = settings.get("mining")
        assert isinstance(mining, dict), "settings.json 缺 mining 节"
        assert mining.get("confirm_per_round") is True, (
            "默认必须为 true（产品行为：轮内 confirmation 不变）"
        )

    def test_schema_declares_mining_confirm_per_round_bool(self):
        schema = _load(SCHEMA_PATH)
        props = schema.get("properties", {})
        assert "mining" in props, "schema 缺 mining 节"
        mining_props = props["mining"].get("properties", {})
        assert "confirm_per_round" in mining_props
        assert mining_props["confirm_per_round"].get("type") == "boolean"

    def test_schema_validates_settings(self):
        jsonschema = pytest.importorskip("jsonschema")
        jsonschema.validate(_load(SETTINGS_PATH), _load(SCHEMA_PATH))

    def test_schema_rejects_non_bool_confirm_per_round(self):
        jsonschema = pytest.importorskip("jsonschema")
        schema = _load(SCHEMA_PATH)
        bad = _load(SETTINGS_PATH)
        bad["mining"]["confirm_per_round"] = "false"
        with pytest.raises(jsonschema.ValidationError):
            jsonschema.validate(bad, schema)


class TestGetSettingScript:
    def _run(self, *args):
        return subprocess.run(
            [sys.executable, GET_SETTING, *args],
            capture_output=True, text=True, encoding="utf-8",
        )

    def test_script_exists(self):
        assert os.path.exists(GET_SETTING), "scripts/get_setting.py 不存在"

    def test_dotpath_query_returns_true(self):
        result = self._run("mining.confirm_per_round")
        assert result.returncode == 0
        assert json.loads(result.stdout) is True

    def test_missing_path_returns_null_exit_0(self):
        result = self._run("mining.nonexistent_key")
        assert result.returncode == 0
        assert json.loads(result.stdout) is None

    def test_no_args_prints_usage_exit_2(self):
        result = self._run()
        assert result.returncode == 2
