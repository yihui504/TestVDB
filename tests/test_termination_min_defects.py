"""min_defects 终止语义：0 = 无下限不触发（reconstruct_context）。

回归守护：确保 --min-defects 0 表示"无下限"，不会因 total=0 误终止。
"""
import json

from reconstruct_context import reconstruct


def _make_session(tmp_path, min_defects, total_defects=0, consecutive=0,
                  coverage=0.0, max_rounds=0):
    """造最小 session tree：results/testdb/1.0/ 含 pipeline_state + contract。"""
    ver = tmp_path / "results" / "testdb" / "1.0"
    ver.mkdir(parents=True)
    ps = {
        "version": 3, "session_id": "s1", "target": "testdb", "version_target": "1.0",
        "current_round": 1, "max_rounds": max_rounds, "min_defects": min_defects,
        "phase": "ROUND_START", "phases_completed": [],
        "project_root": str(tmp_path), "session_dir": "results/testdb/1.0",
        "global_state": {
            "total_defects_confirmed": total_defects,
            "consecutive_no_defect_rounds": consecutive,
            "overall_coverage_pct": coverage,
            "docker_container_running": True,
        },
    }
    (ver / "pipeline_state.json").write_text(json.dumps(ps), encoding="utf-8")
    contract = {
        "target": "testdb", "version": "1.0",
        "api_endpoints": [{"path": "/x", "method": "GET", "category": "data",
                           "source_url": "u", "parameters": []}],
        "data_types": [{"name": "vector"}],
    }
    (ver / "structured_contract.json").write_text(json.dumps(contract), encoding="utf-8")
    return str(ver)


def test_min_defects_zero_means_no_floor(tmp_path):
    """--min-defects 0 = 无下限：total=0 也不因 min_defects 终止。"""
    sd = _make_session(tmp_path, min_defects=0, total_defects=0)
    summary = reconstruct(sd)["summary"]
    assert summary["termination_reason"] == "", summary["termination_reason"]


def test_stalemate_still_terminates_with_zero_floor(tmp_path):
    """min_defects=0 时，僵局（consecutive>=5）仍硬终止。"""
    sd = _make_session(tmp_path, min_defects=0, consecutive=5)
    summary = reconstruct(sd)["summary"]
    assert "僵局" in summary["termination_reason"]
