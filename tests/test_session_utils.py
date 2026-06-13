"""_session_utils 核心工具测试（7 个 hook/维护脚本共享）。"""
import json
import os

import pytest

from _session_utils import _plugin_root, find_session_id, is_session_locked, find_sessions_dir


def test_plugin_root_env_var_priority(monkeypatch, tmp_path):
    """TESTVDB_PLUGIN_ROOT env 优先于脚本位置推断。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    assert _plugin_root() == str(tmp_path)


def test_plugin_root_fallback_to_script_location(monkeypatch):
    """无 env 时回退到脚本位置推断（_session_utils.py 上两级 = 项目根）。"""
    monkeypatch.delenv("TESTVDB_PLUGIN_ROOT", raising=False)
    root = _plugin_root()
    assert "TestVDB" in root or "testvdb" in root.lower()


def test_find_session_id_env_priority(monkeypatch):
    """环境变量最优先。"""
    monkeypatch.setenv("TESTVDB_SESSION_ID", "sess-env-123")
    assert find_session_id() == "sess-env-123"


def test_find_session_id_from_env_file(monkeypatch, tmp_path):
    """env 缺失时读 .env 文件。"""
    monkeypatch.delenv("TESTVDB_SESSION_ID", raising=False)
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    (tmp_path / ".env").write_text('TESTVDB_SESSION_ID="sess-file-456"\n', encoding="utf-8")
    assert find_session_id() == "sess-file-456"


@pytest.mark.parametrize("quote", ['"', "'"])
def test_find_session_id_strips_surrounding_quotes(monkeypatch, tmp_path, quote):
    """.env 值带单/双引号应剥离（v2.2 L-N04 修复项的回归保护）。"""
    monkeypatch.delenv("TESTVDB_SESSION_ID", raising=False)
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    (tmp_path / ".env").write_text(
        f"TESTVDB_SESSION_ID={quote}q-sess{quote}\n", encoding="utf-8")
    assert find_session_id() == "q-sess"


def test_find_session_id_empty_when_no_source(monkeypatch, tmp_path):
    """无任何来源 → 空串。"""
    monkeypatch.delenv("TESTVDB_SESSION_ID", raising=False)
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))  # tmp_path 无 .env/settings.json
    assert find_session_id() == ""


def test_is_session_locked_no_lock_file(tmp_path):
    """无 .session.lock → False。"""
    assert is_session_locked(str(tmp_path)) is False


def test_is_session_locked_active(tmp_path):
    """status=active → True。"""
    (tmp_path / ".session.lock").write_text(
        json.dumps({"status": "active"}), encoding="utf-8")
    assert is_session_locked(str(tmp_path)) is True


def test_is_session_locked_released(tmp_path):
    """status != active → False。"""
    (tmp_path / ".session.lock").write_text(
        json.dumps({"status": "released"}), encoding="utf-8")
    assert is_session_locked(str(tmp_path)) is False


def test_find_sessions_dir_exists(tmp_path):
    """results/ 存在 → 返回路径。"""
    (tmp_path / "results").mkdir()
    assert find_sessions_dir(str(tmp_path)) == os.path.join(str(tmp_path), "results")


def test_find_sessions_dir_missing(tmp_path):
    """results/ 不存在 → None。"""
    assert find_sessions_dir(str(tmp_path)) is None
