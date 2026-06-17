import json, os, sys, pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "scripts"))
import _entry_dispatch as ed


def _make_session(root, target, version, turn_type, phase, sid="s1", completed=False):
    """在 timestamp 级目录造一个 pipeline_state.json。"""
    sd = os.path.join(root, "results", target, version, f"20260617T100000-{sid}")
    os.makedirs(sd, exist_ok=True)
    ps = {
        "session_id": sid, "target": target, "version_target": version,
        "turn_type": turn_type, "phase": phase, "current_round": 1, "max_rounds": 5,
        "phases_completed": [], "global_state": {"total_defects_confirmed": 0},
    }
    with open(os.path.join(sd, "pipeline_state.json"), "w", encoding="utf-8") as f:
        json.dump(ps, f)
    return sd


def test_setup_interruption_is_resumable(tmp_path, monkeypatch):
    """Bug ①: turn_type=setup 中断应被 RESUME（旧行为只认 loop）。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "weaviate", "v1.38.0", "setup", "EXECUTION")
    d = ed.dispatch("weaviate", "v1.38.0")
    assert d["decision"] == "RESUME"
    assert "EXECUTION" in (d.get("phase", "") + d.get("reason", ""))


def test_target_filter_prevents_wrong_resume(tmp_path, monkeypatch):
    """Bug ②: /mine weaviate 不能 RESUME 到 qdrant 中断（即使 mtime 更新）。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "qdrant", "v1.18.2", "loop", "ATTACK_GEN", sid="q1")
    d = ed.dispatch("weaviate", "v1.38.0")
    assert d["decision"] == "FRESH_START"


def test_loop_resume_not_regressed(tmp_path, monkeypatch):
    """回归: turn_type=loop 中断仍被 RESUME。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "weaviate", "v1.38.0", "loop", "ROUND_START")
    assert ed.dispatch("weaviate", "v1.38.0")["decision"] == "RESUME"


def test_done_phase_skipped(tmp_path, monkeypatch):
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "weaviate", "v1.38.0", "done", "DONE")
    assert ed.dispatch("weaviate", "v1.38.0")["decision"] == "FRESH_START"


def test_find_incomplete_excludes_done(tmp_path, monkeypatch):
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "weaviate", "v1.38.0", "loop", "ATTACK_GEN", sid="running")
    _make_session(tmp_path, "weaviate", "v1.38.0", "done", "DONE", sid="finished")
    inc = ed.find_incomplete(str(tmp_path))
    ids = [i["session_id"] for i in inc]
    assert "running" in ids and "finished" not in ids


def test_dispatch_no_target_scans_all(tmp_path, monkeypatch):
    """入口判断无 target（Loop Turn 扫描场景）应续最新未完成，跨 target，返回其 target/version。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "qdrant", "v1.18.2", "loop", "ATTACK_GEN", sid="q1")
    d = ed.dispatch("", "")
    assert d["decision"] == "RESUME"
    assert d["target"] == "qdrant"
    assert d["version"] == "v1.18.2"


def test_resume_target_priority_and_consumed(tmp_path, monkeypatch):
    """.resume_target 标记优先于扫描，且 RESUME 后被 consume（一次性）。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    _make_session(tmp_path, "qdrant", "v1.18.2", "loop", "ATTACK_GEN", sid="q-scan")
    sd_marked = _make_session(tmp_path, "weaviate", "v1.38.0", "setup", "EXECUTION", sid="w-marked")
    ed.write_resume_target(str(tmp_path), sd_marked, "weaviate", "v1.38.0")
    d = ed.dispatch("", "")
    assert d["decision"] == "RESUME"
    assert d["session_dir"] == sd_marked  # 标记优先，非扫描的 qdrant
    assert ed.read_resume_target(str(tmp_path)) is None  # RESUME 后标记已 consume


def test_force_new_consumes_resume_target(tmp_path, monkeypatch):
    """force_new 应 FRESH_START 且清残留 .resume_target（防下次误 RESUME）。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    sd = _make_session(tmp_path, "weaviate", "v1.38.0", "setup", "EXECUTION", sid="w1")
    ed.write_resume_target(str(tmp_path), sd, "weaviate", "v1.38.0")
    d = ed.dispatch("weaviate", "v1.38.0", force_new=True)
    assert d["decision"] == "FRESH_START"
    assert ed.read_resume_target(str(tmp_path)) is None  # 标记已清，不留残留


def test_find_by_session_id(tmp_path, monkeypatch):
    """resume 命令按 session_id 定位 session_dir（timestamp 级，未找到返回 None）。"""
    monkeypatch.setenv("TESTVDB_PLUGIN_ROOT", str(tmp_path))
    sd = _make_session(tmp_path, "weaviate", "v1.38.0", "setup", "EXECUTION", sid="w-find")
    assert ed.find_by_session_id(str(tmp_path), "w-find") == sd
    assert ed.find_by_session_id(str(tmp_path), "nonexistent") is None

