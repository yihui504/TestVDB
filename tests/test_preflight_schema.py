"""preflight check_settings_schema 测试（批次 C [2]，review L-N12）。"""
import json

import preflight


def test_check_settings_schema_valid(tmp_path, monkeypatch, capsys):
    """合法 settings（含 required key）+ schema → OK。"""
    (tmp_path / "settings.json").write_text(json.dumps({"docker": {}, "pipeline": {}}), encoding="utf-8")
    (tmp_path / "contracts").mkdir()
    (tmp_path / "contracts" / "settings_schema.json").write_text(
        json.dumps({"required": ["docker", "pipeline"]}), encoding="utf-8")
    monkeypatch.setattr(preflight, "_plugin_root", lambda: str(tmp_path))
    preflight.check_settings_schema()
    out = capsys.readouterr().out
    assert "OK" in out


def test_check_settings_schema_missing_required_key(tmp_path, monkeypatch, capsys):
    """缺 required key → WARNING（配置错误不再静默）。"""
    (tmp_path / "settings.json").write_text(json.dumps({"docker": {}}), encoding="utf-8")  # 缺 pipeline
    (tmp_path / "contracts").mkdir()
    (tmp_path / "contracts" / "settings_schema.json").write_text(
        json.dumps({"required": ["docker", "pipeline"]}), encoding="utf-8")
    monkeypatch.setattr(preflight, "_plugin_root", lambda: str(tmp_path))
    preflight.check_settings_schema()
    out = capsys.readouterr().out
    assert "pipeline" in out  # 报出缺失的 key
    assert "WARNING" in out or "missing" in out.lower()


def test_check_settings_schema_skip_when_no_schema(tmp_path, monkeypatch, capsys):
    """无 schema 文件 → SKIP（不报错）。"""
    monkeypatch.setattr(preflight, "_plugin_root", lambda: str(tmp_path))
    preflight.check_settings_schema()
    out = capsys.readouterr().out
    assert "SKIP" in out
