"""ADR-0008 证据链双 Agent 架构测试.

覆盖:
- extract_candidates: 候选机械提取（DEFECT_FOUND/SCRIPT_ERROR/constraint 提取）
- verify_live_l1: L1 前移读 candidates.jsonl + 旧 schema fallback
- novelty_gate.load_chain_verdicts: DEFECT 过滤 + 优先级 + fallback
- pipeline_state: EVIDENCE_BUILD/CHAIN_AUDIT 改名后转换与自检
- _classify_script_errors + _apply_script_retry: retry 子循环（checklist 1.2 补课）
"""
from __future__ import annotations

import json
import sys
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parent.parent / "scripts"
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

from extract_candidates import extract_candidates  # noqa: E402
from novelty_gate import load_chain_verdicts, load_candidates_source  # noqa: E402


# ═══════════════════════════════════════════════════════════════
# extract_candidates
# ═══════════════════════════════════════════════════════════════

class TestExtractCandidates:
    def _mk_session(self, tmp_path, logs: dict[str, str], scripts: dict[str, str] | None = None):
        sd = tmp_path / "session"
        (sd / "debate_logs").mkdir(parents=True)
        for name, text in logs.items():
            (sd / f"output_{name}.log").write_text(text, encoding="utf-8")
        for sid, text in (scripts or {}).items():
            (sd / "debate_logs" / f"{sid}.py").write_text(text, encoding="utf-8")
        return sd

    def test_defect_found_extracted_with_constraint(self, tmp_path):
        sd = self._mk_session(
            tmp_path,
            logs={"b_001": "Status: 200\nBody: ok\nVERDICT: DEFECT_FOUND (Type1) — x"},
            scripts={"b_001": "#!/usr/bin/env python3\n'''\nConstraint: t_range_001\nAttack: boundary (strategy 1) — x\n'''"},
        )
        cands = extract_candidates(sd)
        assert len(cands) == 1
        c = cands[0]
        assert c["defect_id"] == "b_001"
        assert c["constraint_id"] == "t_range_001"
        assert c["attack"].startswith("boundary")
        assert "DEFECT_FOUND" in c["claim_hint"]

    def test_script_error_excluded(self, tmp_path):
        sd = self._mk_session(tmp_path, logs={
            "ok_001": "VERDICT: DEFECT_FOUND (Type1)",
            "bad_001": "VERDICT: SCRIPT_ERROR — TESTVDB_DB_URL not set",
        })
        cands = extract_candidates(sd)
        assert [c["defect_id"] for c in cands] == ["ok_001"]

    def test_no_defect_log_excluded(self, tmp_path):
        sd = self._mk_session(tmp_path, logs={"pass_001": "VERDICT: NO_DEFECT"})
        assert extract_candidates(sd) == []

    def test_missing_script_yields_empty_constraint(self, tmp_path):
        sd = self._mk_session(tmp_path, logs={"orphan_001": "VERDICT: DEFECT_FOUND (Type1)"})
        cands = extract_candidates(sd)
        assert cands[0]["constraint_id"] == ""

    def test_claim_hint_truncated(self, tmp_path):
        long_claim = "VERDICT: DEFECT_FOUND — " + "x" * 500
        sd = self._mk_session(tmp_path, logs={"b_002": long_claim})
        c = extract_candidates(sd)[0]
        assert len(c["claim_hint"]) <= 201  # 200 + ellipsis


# ═══════════════════════════════════════════════════════════════
# verify_live_l1 前移（B1）
# ═══════════════════════════════════════════════════════════════

class TestL1CandidatesSource:
    def test_reads_candidates_jsonl_first(self, tmp_session_dir):
        from verify_live_l1 import _load_candidates_l1
        (tmp_session_dir / "candidates.jsonl").write_text(
            json.dumps({"defect_id": "d1", "script": "s1"}) + "\n", encoding="utf-8")
        dl = tmp_session_dir / "debate_logs"
        dl.mkdir()
        (dl / "stage2_aggregation.json").write_text(
            json.dumps({"confirmed_defects": [{"defect_id": "old"}]}), encoding="utf-8")
        cands = _load_candidates_l1(tmp_session_dir)
        assert [c["defect_id"] for c in cands] == ["d1"]

    def test_fallback_to_stage2_aggregation(self, tmp_session_dir):
        from verify_live_l1 import _load_candidates_l1
        dl = tmp_session_dir / "debate_logs"
        dl.mkdir()
        (dl / "stage2_aggregation.json").write_text(
            json.dumps({"confirmed_defects": [{"defect_id": "legacy_1"}]}), encoding="utf-8")
        cands = _load_candidates_l1(tmp_session_dir)
        assert [c["defect_id"] for c in cands] == ["legacy_1"]

    def test_no_source_returns_empty(self, tmp_session_dir):
        from verify_live_l1 import _load_candidates_l1
        assert _load_candidates_l1(tmp_session_dir) == []


# ═══════════════════════════════════════════════════════════════
# novelty_gate.load_chain_verdicts（B4）
# ═══════════════════════════════════════════════════════════════

class TestLoadChainVerdicts:
    def _mk(self, tmp_path, verdicts):
        sd = tmp_path / "s"
        (sd / "debate_logs").mkdir(parents=True)
        (sd / "debate_logs" / "chain_verdicts.json").write_text(
            json.dumps({"auditor": "chain-auditor", "target": "qdrant",
                        "version": "v1.18.3", "verdicts": verdicts}), encoding="utf-8")
        return sd

    def test_only_defect_verdicts_kept(self, tmp_path):
        sd = self._mk(tmp_path, [
            {"defect_id": "a", "verdict": "DEFECT"},
            {"defect_id": "b", "verdict": "NOT_DEFECT"},
            {"defect_id": "c", "verdict": "NEEDS_MORE_EVIDENCE"},
        ])
        src = load_chain_verdicts(sd)
        assert [d["defect_id"] for d in src["confirmed_defects"]] == ["a"]
        assert src["source"] == "chain_verdicts"

    def test_priority_over_stage2(self, tmp_path):
        sd = self._mk(tmp_path, [{"defect_id": "a", "verdict": "DEFECT"}])
        (sd / "debate_logs" / "stage2_aggregation.json").write_text(
            json.dumps({"confirmed_defects": [{"defect_id": "old"}]}), encoding="utf-8")
        src = load_candidates_source(sd)
        assert src["source"] == "chain_verdicts"

    def test_fallback_when_no_chain_verdicts(self, tmp_path):
        sd = tmp_path / "s2"
        (sd / "debate_logs").mkdir(parents=True)
        (sd / "debate_logs" / "stage2_aggregation.json").write_text(
            json.dumps({"confirmed_defects": [{"defect_id": "old2"}]}), encoding="utf-8")
        src = load_candidates_source(sd)
        assert src is not None and src["confirmed_defects"][0]["defect_id"] == "old2"

    def test_missing_file_returns_none(self, tmp_path):
        sd = tmp_path / "s3"
        (sd / "debate_logs").mkdir(parents=True)
        assert load_chain_verdicts(sd) is None


# ═══════════════════════════════════════════════════════════════
# pipeline_state ADR-0008 改名（B5）
# ═══════════════════════════════════════════════════════════════

class TestPipelineStateRenamed:
    def test_new_phases_in_order(self):
        from pipeline_state import PHASE_ORDER
        assert "EVIDENCE_BUILD" in PHASE_ORDER
        assert "CHAIN_AUDIT" in PHASE_ORDER
        assert "DEBATE_S2" not in PHASE_ORDER
        assert "VERIFY_LIVE" not in PHASE_ORDER
        i_eb = PHASE_ORDER.index("EVIDENCE_BUILD")
        i_ca = PHASE_ORDER.index("CHAIN_AUDIT")
        i_rep = PHASE_ORDER.index("REPORTING")
        assert i_eb < i_ca < i_rep

    def test_full_walkthrough(self, tmp_path):
        from pipeline_state import PipelineState
        st = PipelineState.create(target="t", version="v1", max_rounds=1,
                                  min_defects=0, session_dir=str(tmp_path))
        for p in ["ATTACK_GEN", "DEBATE_S1", "EXECUTION",
                  "EVIDENCE_BUILD", "CHAIN_AUDIT", "REPORTING", "DEFECT_REVIEW"]:
            st.advance(p)
        assert "CHAIN_AUDIT" in st._data["phases_completed"]

    def test_old_phase_rejected(self, tmp_path):
        from pipeline_state import PipelineState, InvalidTransition
        st = PipelineState.create(target="t", version="v1", max_rounds=1,
                                  min_defects=0, session_dir=str(tmp_path))
        st.advance("ATTACK_GEN")
        st.advance("DEBATE_S1")
        st.advance("EXECUTION")
        try:
            st.advance("DEBATE_S2")  # 旧 phase 名应被拒
            assert False, "旧 phase 名应抛 InvalidTransition"
        except InvalidTransition:
            pass


# ═══════════════════════════════════════════════════════════════
# retry 子循环（checklist 1.2 补课：分类器 + apply-retry）
# ═══════════════════════════════════════════════════════════════

BAD_SCRIPTS = {
    # 1. syntax_error — 缺右括号
    "syn_001.py": 'def f(:\n    pass\n',
    # 2. bare_json_chain
    "bare_001.py": (
        'import requests\n'
        'def safe_request(*a, **k):\n    return requests.get(*a, **k)\n'
        'def run():\n'
        '    d = safe_request("GET", "/x").json()["k"]\n'
        '    print("VERDICT: DEFECT_FOUND")\n'
    ),
    # 3. safe_request_unused（定义不调用）
    "unused_001.py": (
        'import requests\n'
        'def safe_request(*a, **k):\n    return requests.get(*a, **k)\n'
        'def run():\n'
        '    r = requests.get("http://x")\n'
        '    print("VERDICT: NO_DEFECT")\n'
    ),
    # 4. cleanup_unwrapped（drop 未包 try）
    "clean_001.py": (
        'import requests\n'
        'def safe_request(*a, **k):\n    return requests.get(*a, **k)\n'
        'def run():\n'
        '    safe_request("DELETE", "/c/x")\n'
        '    drop("x")\n'
        '    print("VERDICT: NO_DEFECT")\n'
    ),
    # 5. verdict_missing
    "noverd_001.py": (
        'import requests\n'
        'def safe_request(*a, **k):\n    return requests.get(*a, **k)\n'
        'def run():\n'
        '    safe_request("GET", "/x")\n'
    ),
    # 干净脚本 — 不应报错
    "ok_001.py": (
        'import requests\n'
        'def safe_request(*a, **k):\n    return requests.get(*a, **k)\n'
        'def run():\n'
        '    try:\n'
        '        safe_request("DELETE", "/c/x")\n'
        '    except Exception:\n'
        '        pass\n'
        '    print("VERDICT: NO_DEFECT")\n'
    ),
}


class TestClassifyScriptErrors:
    def _mk(self, tmp_path):
        sd = tmp_path / "session"
        scripts = sd / "boundary_scripts"
        scripts.mkdir(parents=True)
        for name, text in BAD_SCRIPTS.items():
            (scripts / name).write_text(text, encoding="utf-8")
        return sd

    def test_all_five_classes_detected(self, tmp_path):
        import subprocess
        sd = self._mk(tmp_path)
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "_classify_script_errors.py"), str(sd)],
            capture_output=True, text=True, encoding="utf-8", timeout=60)
        assert r.returncode == 1, f"应检出错误 exit 1，stdout={r.stdout}"
        # CLI stdout 是人类可读报告；JSON 在 script_errors.json
        data = json.loads((sd / "script_errors.json").read_text(encoding="utf-8"))
        by_id = {e["script_id"]: set(e["error_classes"]) for e in data["errors"]}
        assert "syntax_error" in by_id.get("syn_001", set())
        assert "bare_json_chain" in by_id.get("bare_001", set())
        assert "safe_request_unused" in by_id.get("unused_001", set())
        assert "cleanup_unwrapped" in by_id.get("clean_001", set())
        assert "verdict_missing" in by_id.get("noverd_001", set())
        assert "ok_001" not in by_id

    def test_clean_session_exit_zero(self, tmp_path):
        import subprocess
        sd = tmp_path / "session"
        (sd / "boundary_scripts").mkdir(parents=True)
        (sd / "boundary_scripts" / "ok.py").write_text(BAD_SCRIPTS["ok_001.py"], encoding="utf-8")
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "_classify_script_errors.py"), str(sd)],
            capture_output=True, text=True, encoding="utf-8", timeout=60)
        assert r.returncode == 0


class TestApplyScriptRetry:
    def _mk_with_errors(self, tmp_path):
        import subprocess
        sd = tmp_path / "session"
        (sd / "boundary_scripts").mkdir(parents=True)
        for name in ("syn_001.py", "ok_001.py"):
            (sd / "boundary_scripts" / name).write_text(
                BAD_SCRIPTS[name], encoding="utf-8")
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "_classify_script_errors.py"), str(sd)],
            capture_output=True, text=True, encoding="utf-8", timeout=60)
        assert r.returncode == 1
        return sd

    def test_counter_and_feedback_written(self, tmp_path):
        import subprocess
        sd = self._mk_with_errors(tmp_path)
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "_apply_script_retry.py"), str(sd)],
            capture_output=True, text=True, encoding="utf-8", timeout=60)
        assert r.returncode == 0
        out = json.loads(r.stdout)
        assert any(e["script_id"] == "syn_001" for e in out["regen"])
        retry = json.loads((sd / "script_retry.json").read_text(encoding="utf-8"))
        assert retry.get("syn_001") == 1
        fb = sd / "boundary_scripts" / "syn_001.retry_feedback.json"
        assert fb.exists(), "retry_feedback.json 应已写入"

    def test_exhausted_deletes_and_archives(self, tmp_path):
        import subprocess
        sd = self._mk_with_errors(tmp_path)
        # 预置 counter 已达上限 → 应走超限降级（删脚本 + 记 exhausted）
        (sd / "script_retry.json").write_text(
            json.dumps({"syn_001": 2}), encoding="utf-8")
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "_apply_script_retry.py"), str(sd)],
            capture_output=True, text=True, encoding="utf-8", timeout=60)
        out = json.loads(r.stdout)
        assert any(e["script_id"] == "syn_001" for e in out["exhausted"])
        assert not (sd / "boundary_scripts" / "syn_001.py").exists(), "超限脚本应被删除"
        ex_doc = json.loads((sd / "script_retry_exhausted.json").read_text(encoding="utf-8"))
        assert any(e["script_id"] == "syn_001" for e in ex_doc["exhausted"])
