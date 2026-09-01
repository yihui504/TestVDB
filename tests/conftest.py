"""Shared fixtures + sys.path setup for TestVDB test suite."""
import sys
from pathlib import Path
import json

import pytest

# 让测试能 import scripts/ 下的模块（如 _session_utils, validate_api_format）
SCRIPTS_DIR = Path(__file__).resolve().parent.parent / "scripts"
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))


@pytest.fixture
def tmp_session_dir(tmp_path):
    """临时 session 目录（pipeline v3 布局: results/<target>/<version>/<timestamp>）。"""
    session = tmp_path / "results" / "testdb" / "1.0" / "20260614T000000"
    session.mkdir(parents=True)
    return session


@pytest.fixture
def tmp_version_dir(tmp_path):
    """临时 version 目录（契约所在: results/<target>/<version>/）。"""
    version = tmp_path / "results" / "testdb" / "1.0"
    version.mkdir(parents=True)
    return version


@pytest.fixture
def make_contract(tmp_path):
    """工厂 fixture：造一个 structured_contract.json，返回 (version_dir, contract_dict)。"""
    def _make(target="testdb", endpoints=None, **overrides):
        version = tmp_path / "results" / target / "1.0"
        version.mkdir(parents=True)
        contract = {
            "target": target,
            "version": "1.0",
            "api_endpoints": endpoints or [
                {"path": "objects+create", "method": "POST", "category": "objects",
                 "source_url": "https://example.test/docs/a", "doc_version": "1.0",
                 "parameters": []},
            ],
            "data_types": [{"name": "vector", "type": "array"}],
        }
        contract.update(overrides)
        (version / "structured_contract.json").write_text(
            json.dumps(contract), encoding="utf-8")
        return version, contract
    return _make
