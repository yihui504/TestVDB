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


# ═══════════════════════════════════════════════════════════════
# 视角 D（开发者认知）接入 — SOP 条款防回归（2026-08-18）
# ═══════════════════════════════════════════════════════════════

class TestPerspectiveD:
    """视角 D 灰区裁决接入（commit 计划见 plans/validated-riding-cat.md）。"""

    def test_data_access_includes_cognition(self):
        from pathlib import Path
        sop = Path(__file__).resolve().parent.parent / "agents" / "chain-auditor.md"
        s = sop.read_text(encoding="utf-8")
        assert "developer_cognition.json" in s, "dataAccess 缺认知材料"
        assert "不跨 vendor 引用" in s or "不跨 vendor" in s, "缺跨 vendor 禁令"

    def test_perspective_d_section_exists(self):
        from pathlib import Path
        sop = Path(__file__).resolve().parent.parent / "agents" / "chain-auditor.md"
        s = sop.read_text(encoding="utf-8")
        assert "视角 D" in s, "缺视角 D 段"
        for kw in ("blindspot_indicators", "by_design_patterns", "SUPPORTS_DEFECT", "NO_SIGNAL"):
            assert kw in s, f"视角 D 段缺 {kw}"

    def test_aggregation_grey_zone_rule(self):
        from pathlib import Path
        sop = Path(__file__).resolve().parent.parent / "agents" / "chain-auditor.md"
        s = sop.read_text(encoding="utf-8")
        # 灰区分支：D 只在 A/B 未定案时生效
        assert "D==SUPPORTS_DEFECT" in s and "D==SUPPORTS_NOT_DEFECT" in s, "缺 D 灰区分支"
        assert "维护者认知同样不能" in s, "缺认知不翻 A/B 案原则"

    def test_verdict_schema_cognition_field(self):
        from pathlib import Path
        sop = Path(__file__).resolve().parent.parent / "agents" / "chain-auditor.md"
        s = sop.read_text(encoding="utf-8")
        assert '"verdict_D": "SUPPORTS_DEFECT|SUPPORTS_NOT_DEFECT|NO_SIGNAL"' in s, "schema 缺 cognition 字段"

    def test_mine_dispatch_prompt_has_cognition(self):
        from pathlib import Path
        mine = Path(__file__).resolve().parent.parent / "commands" / "mine.md"
        s = mine.read_text(encoding="utf-8")
        assert "developer_cognition.json" in s, "auditor 派发 prompt 缺材料路径"


# ═══════════════════════════════════════════════════════════════
# 打回重审机制 + 全量取证 + HTTP 语义判定（2026-08-18）— 防回归
# ═══════════════════════════════════════════════════════════════

class TestReworkMechanism:
    """auditor 打回工单 + builder 全量取证 + 视角B第五类（plans/validated-riding-cat.md）。"""

    def test_auditor_correspondence_check(self):
        from pathlib import Path
        sop = Path(__file__).resolve().parent.parent / "agents" / "chain-auditor.md"
        s = sop.read_text(encoding="utf-8")
        assert "对应性核查" in s, "缺第 4 查（现象对应性核查）"
        assert "PHENOMENON_MISMATCH" in s and "EVIDENCE_GAP" in s and "SUSPECTED_HALLUCINATION" in s, "缺工单三 type"
        assert "rework_order" in s, "schema 缺 rework_order 字段"
        assert "最多 3 次" in s or "3 轮" in s, "缺 3 轮上限条款"

    def test_auditor_http_semantics_fifth_class(self):
        from pathlib import Path
        sop = Path(__file__).resolve().parent.parent / "agents" / "chain-auditor.md"
        s = sop.read_text(encoding="utf-8")
        assert "HTTP 语义恒真" in s, "视角 B 缺第五类"
        assert "两条件同时满足" in s, "缺强限定双条件"

    def test_builder_full_evidence_clauses(self):
        from pathlib import Path
        sop = Path(__file__).resolve().parent.parent / "agents" / "evidence-builder.md"
        s = sop.read_text(encoding="utf-8")
        assert "全量取证硬约束" in s, "缺全量取证条款"
        assert "claim_alignment" in s, "schema 缺 claim_alignment"
        assert "secondary_observations" in s, "schema 缺 secondary_observations"
        assert "http_semantics" in s, "schema 缺 http_semantics"
        assert "log 全文" in s, "缺读全文要求"

    def test_mine_rework_order_wiring(self):
        from pathlib import Path
        mine = Path(__file__).resolve().parent.parent / "commands" / "mine.md"
        s = mine.read_text(encoding="utf-8")
        assert "rework_order" in s and "rework_state" in s, "8e 缺打回工单消费说明/计数文件"
        assert "最多 3 轮" in s or "3 轮" in s, "缺 3 轮上限"


# ═══════════════════════════════════════════════════════════════
# 稳定化改进（E1 定稿 2026-08-18）— 机械视角A + 必读先验 + ≤12 分批 + 搜证自检
# ═══════════════════════════════════════════════════════════════

class TestStabilization:
    def test_check_chain_grounding_four_branches(self, tmp_path):
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
        from check_chain_grounding import judge_grounding
        ct = json.dumps({"constraints": [{"constraint_id": "c1", "assertion": "x >= 1"}]})
        # 1. 无引用
        r = judge_grounding({"steps": {"contract_grounding": {}}}, ct)
        assert r["verdict_A"] == "NEUTRAL" and r["reason"] == "no_reference"
        assert r["implied_verdict"] == "GREY_ZONE"
        # 2. id 不存在
        r = judge_grounding({"steps": {"contract_grounding": {"constraint_id": "cX"}}}, ct)
        assert r["verdict_A"] == "NEUTRAL" and r["reason"] == "constraint_absent"
        assert r["implied_verdict"] == "GREY_ZONE"
        # 3. id+quote_ok → violates 分支（implied_verdict 定案）
        ok = {"steps": {"contract_grounding": {"constraint_id": "c1", "assertion_text_quoted": "x >= 1", "api_violates_assertion": True}}}
        r = judge_grounding(ok, ct)
        assert r["verdict_A"] == "CONFIRMED" and r["implied_verdict"] == "DEFECT"
        ok2 = {"steps": {"contract_grounding": {**ok["steps"]["contract_grounding"], "api_violates_assertion": False}}}
        r = judge_grounding(ok2, ct)
        assert r["verdict_A"] == "REFUTED" and r["implied_verdict"] == "NOT_DEFECT"
        # 4. 引文不一致
        mm = {"steps": {"contract_grounding": {"constraint_id": "c1", "assertion_text_quoted": "完全不同的文字", "api_violates_assertion": True}}}
        r = judge_grounding(mm, ct)
        assert r["verdict_A"] == "NEUTRAL" and r["reason"] == "quote_mismatch"
        assert r["implied_verdict"] == "GREY_ZONE"

    def test_auditor_sop_mechanical_A(self):
        sop = (Path(__file__).resolve().parent.parent / "agents" / "chain-auditor.md").read_text(encoding="utf-8")
        assert "check_chain_grounding.py" in sop, "缺机械 A 脚本引用"
        assert "不得改判" in sop or "不得自行改判" in sop, "缺采信约束"
        assert "agent_suspects_contract_wrong: true" not in sop.split("例外条款已删除")[1][:500], "例外条款残留"

    def test_auditor_sop_batch_limit_and_prior(self):
        sop = (Path(__file__).resolve().parent.parent / "agents" / "chain-auditor.md").read_text(encoding="utf-8")
        assert "≤12 条链" in sop or "≤12" in sop, "缺分批硬上限"
        assert "必读先验" in sop, "缺认知必读先验"



    def test_grounding_conflict_state(self):
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
        from check_chain_grounding import judge_grounding
        ct = json.dumps({"constraints": [{"constraint_id": "c1", "assertion": "x must be valid"}]})
        # A=REFUTED + 机械 B=CONFIRMED（HTTP语义） → CONFLICT
        chain = {"steps": {
            "contract_grounding": {"constraint_id": "c1", "assertion_text_quoted": "x must be valid", "api_violates_assertion": False},
            "execution_evidence": {"log_pattern": "req x=bad -> http=200, code=65535 ('x is invalid')",
                                    "secondary_observations": [],
                                    "http_semantics": {"client_error_returned_as": "HTTP 2xx + 业务错误码"}},
        }}
        r = judge_grounding(chain, ct)
        assert r["implied_verdict"] == "CONFLICT" and r["reason"] == "id+quote_ok_conflict"
        # A=REFUTED 无冲突 → NOT_DEFECT 照旧
        chain2 = {"steps": {
            "contract_grounding": {"constraint_id": "c1", "assertion_text_quoted": "x must be valid", "api_violates_assertion": False},
            "execution_evidence": {"log_pattern": "req ok -> http=200", "secondary_observations": [], "http_semantics": {}},
        }}
        r2 = judge_grounding(chain2, ct)
        assert r2["implied_verdict"] == "NOT_DEFECT" and r2["reason"] == "id+quote_ok"

    def test_check_violates_consistency(self):
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
        from check_physical_constraints import check_violates_consistency
        # violates=False + quote 约束 + 接受观测 → suspicious
        chain = {"steps": {
            "contract_grounding": {"constraint_id": "x", "assertion_text_quoted": "data field types match schema", "api_violates_assertion": False},
            "execution_evidence": {"log_pattern": "insert int -> status=200, code=0 (accepted)", "secondary_observations": []},
        }}
        r = check_violates_consistency(chain)
        assert r["suspicious"] is True and "violates=False" in r["evidence"]
        # violates=True 不复核
        chain2 = {"steps": {"contract_grounding": {**chain["steps"]["contract_grounding"], "api_violates_assertion": True}}}
        assert check_violates_consistency(chain2)["suspicious"] is False
        # 无接受观测不 suspicious
        chain3 = {"steps": {
            "contract_grounding": chain["steps"]["contract_grounding"],
            "execution_evidence": {"log_pattern": "insert -> status=400 rejected", "secondary_observations": []},
        }}
        assert check_violates_consistency(chain3)["suspicious"] is False


    def test_anchor_conflict_fingerprints(self, tmp_path):
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
        import check_anchor_conflict as cac
        # 构造 cognition（fingerprints 显式声明）
        intel = tmp_path / "milvus"
        intel.mkdir(parents=True)
        json.dump({"by_design_patterns": [
            {"pattern": "shardsNum unconsumed top-level", "source_issues": [1], "fingerprints": ["shardsNum", "create"]},
            {"pattern": "old anchor no fingerprints", "source_issues": [2]},
        ]}, open(intel / "developer_cognition.json", "w", encoding="utf-8"))
        def chain(did, phen):
            return {"defect_id": did, "steps": {
                "execution_evidence": {"log_pattern": phen, "secondary_observations": []},
                "contract_grounding": {"assertion_text_quoted": ""}}}
        # 指纹全命中
        r = cac.detect_anchor_conflict(chain("milvus_x1", "create with shardsNum=0 -> http=200"), intel_root=tmp_path)
        assert r["anchored_conflict"] is True
        # 缺一指纹不命中
        r2 = cac.detect_anchor_conflict(chain("milvus_x2", "create ok"), intel_root=tmp_path)
        assert r2["anchored_conflict"] is False
        # 无 fingerprints 旧锚点不参与
        r3 = cac.detect_anchor_conflict(chain("milvus_x3", "old anchor"), intel_root=tmp_path)
        assert r3["anchored_conflict"] is False

    def test_auditor_sop_aggregation_mechanized(self):
        sop = (Path(__file__).resolve().parent.parent / "agents" / "chain-auditor.md").read_text(encoding="utf-8")
        assert "implied_verdict" in sop, "缺聚合机械化字段"
        assert "LLM 无权改写" in sop, "缺聚合改写禁令"
        assert "GREY_ZONE" in sop, "缺灰区三态"

    def test_builder_sop_sufficiency_check(self):
        sop = (Path(__file__).resolve().parent.parent / "agents" / "evidence-builder.md").read_text(encoding="utf-8")
        assert "搜证充分性自检" in sop, "缺充分性自检条款"
        assert "sufficiency_check" in sop, "缺自检留痕字段"


# ═══════════════════════════════════════════════════════════════
# 机械 B 判定（E3 后稳定化扩展，2026-08-18）— 数值下界/HTTP语义/类型恒真
# ═══════════════════════════════════════════════════════════════

class TestMechanicalB:
    def test_judge_physical_rules(self):
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
        from check_physical_constraints import judge_physical

        def chain(cg_quote, log_pattern, http_sem=None, api_viol=None):
            return {"steps": {
                "contract_grounding": {"constraint_id": "x", "assertion_text_quoted": cg_quote,
                                        "api_violates_assertion": api_viol},
                "execution_evidence": {"log_pattern": log_pattern,
                                        "secondary_observations": [],
                                        "http_semantics": http_sem or {}},
            }}

        # 规则1a：cg 断言 ef >= 1 + log ef=-1 被接受
        r = judge_physical(chain("ef >= 1 (HNSW search parameter)", "REQ: search with ef=-1 -> http=200, code=0 (returned 1 result)"))
        assert r["verdict_B"] == "CONFIRMED" and r["objective_constraint_class"] == "数值下界"
        # 规则1b：计数词典负值裸触发（无 cg 断言）
        r = judge_physical(chain("no assertion here", "POST schema with desiredCount=-1 -> http=200"))
        assert r["verdict_B"] == "CONFIRMED" and r["objective_constraint_class"] == "数值下界"
        # 规则2：HTTP 语义双条件
        r = judge_physical(chain("deleted count <= 10000", "DELETE batch -> http=500",
                                 http_sem={"client_error_returned_as": "HTTP 5xx",
                                            "note": "Validation errors return HTTP 500"}))
        assert r["verdict_B"] == "CONFIRMED" and r["objective_constraint_class"] == "HTTP语义恒真"
        # 规则3：null 被接受 + must be 声称
        r = judge_physical(chain("vectorIndexType must be one of allowed types",
                                 "POST schema with distance=null -> http=200, stored distance=cosine"))
        assert r["verdict_B"] == "CONFIRMED" and r["objective_constraint_class"] == "类型恒真"
        # 不触发：无违规观测
        r = judge_physical(chain("x >= 1", "POST ok -> http=200", api_viol=False))
        assert r["verdict_B"] == "NOT_TRIGGERED"
        # 不触发：ef=-1 sentinel（词典不含 ef；无 cg 下界断言）
        r = judge_physical(chain("ef is a sentinel default", "POST with ef=-1 -> http=200"))
        assert r["verdict_B"] == "NOT_TRIGGERED"


    def test_judge_physical_rule2_self_invalid_and_rule4(self):
        import sys
        sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
        from check_physical_constraints import judge_physical

        def chain(cg_quote, log_pattern, http_sem=None, excerpt=None):
            return {"steps": {
                "contract_grounding": {"constraint_id": "x", "assertion_text_quoted": cg_quote,
                                        "api_violates_assertion": False},
                "execution_evidence": {"log_pattern": log_pattern,
                                        "secondary_observations": [],
                                        "http_semantics": http_sem or {}},
                "source_grounding": {"source_excerpt": excerpt or ""},
            }}

        # A1 规则2第三分支：服务器自证（错误消息自证值非法 + 2xx 返回）
        r = judge_physical(chain("limit + offset < 16384",
                                 "search with limit=0 -> http=200, code=65535 ('topk [0] is invalid, it should be in range [1, 16384]')",
                                 http_sem={"client_error_returned_as": "HTTP 2xx + 业务错误码"}))
        assert r["verdict_B"] == "CONFIRMED" and r["objective_constraint_class"] == "HTTP语义恒真"
        # A2 规则4：挂起观测 + 源码无上界校验
        r = judge_physical(chain("no assertion",
                                 "PUT /collections payload={shard_number=2147483647} -> status=None (request timeout); GET / -> status=None",
                                 excerpt="Validator (lower-bound only, u32 type): pub struct CreateCollection"))
        assert r["verdict_B"] == "CONFIRMED" and r["objective_constraint_class"] == "资源边界"
        # 规则4 单条件不触发：挂起但源码有上界
        r = judge_physical(chain("no assertion", "PUT -> status=None (timeout)",
                                 excerpt="Validator with min=1 and max=65536 both bounds"))
        assert r["verdict_B"] == "NOT_TRIGGERED"

    def test_auditor_sop_mechanical_b(self):
        sop = (Path(__file__).resolve().parent.parent / "agents" / "chain-auditor.md").read_text(encoding="utf-8")
        assert "check_physical_constraints.py" in sop, "缺机械 B 脚本引用"
        assert "NOT_TRIGGERED" in sop, "缺机械 B 兜底分支"


# ═══════════════════════════════════════════════════════════════
# 机械 B 规则 5/6 — 一致性元规则（2026-08-23，测试先行）
#
# 设计依据（元逻辑，非 GT 驱动；先写测试锁死设计再实现）：
#   规则5 同族同处置 = 挖掘侧 generalization_shapes（参数族同处置）
#                     的判定侧镜像：同端点上同类违规值，一族被拒、
#                     另一族被静默替换接受 → 处置不一致即缺陷信号。
#   规则6 接口面对称 = 挖掘侧 interface_parity 策略的判定侧实体化：
#                     同参数同违规值，A 接口面拒绝、B 接口面接受
#                     → 面间不对称即缺陷信号（契约明示差异由 auditor
#                     复核，机械层纯现象）。
#   两规则的信息源均为链内对照观测（不经过被测案 issue/GT），
#   依赖 builder 对照取证义务（观测行须含面标记/替换语义原文）。
# ═══════════════════════════════════════════════════════════════

class TestMechanicalBConsistencyRules:
    def _chain(self, log_pattern, secondary=None):
        return {"steps": {
            "contract_grounding": {"constraint_id": "x", "assertion_text_quoted": "no assertion",
                                    "api_violates_assertion": False},
            "execution_evidence": {"log_pattern": log_pattern,
                                    "secondary_observations": secondary or [],
                                    "http_semantics": {}},
        }}

    # ── 规则5 同族同处置 ──

    def test_rule5_family_inconsistent_treatment(self):
        """同族枚举外值：一族被拒、另一族被静默替换接受 → CONFIRMED。

        原型形态（milvus_033 型现象结构）：idType 枚举外值 400 拒绝，
        vectorFieldType 枚举外值 200 且替换为默认值——同族不同处置。
        """
        from check_physical_constraints import judge_physical
        r = judge_physical(self._chain(
            "c1: vectorFieldType='not_an_enum' -> http=200, code=0 (silently substituted default value)",
            ["c2: idType='not_an_enum' -> http=400 rejected ('invalid idType value')"]))
        assert r["verdict_B"] == "CONFIRMED"
        assert r["objective_constraint_class"] == "同族不一致"

    def test_rule5_substitution_without_reject_control(self):
        """护栏：只有静默替换、无同族拒绝对照 → 不触发。

        防误伤'非法值回退默认'整体风格（若无同类参数被拒，
        无对照证据表明服务器认为该类值非法）。
        """
        from check_physical_constraints import judge_physical
        r = judge_physical(self._chain(
            "c1: vectorFieldType='not_an_enum' -> http=200 (substituted default)"))
        assert r["verdict_B"] == "NOT_TRIGGERED"

    def test_rule5_reject_without_substitution(self):
        """护栏：只有拒绝对照、无静默替换接受 → 不触发。"""
        from check_physical_constraints import judge_physical
        r = judge_physical(self._chain(
            "c1: idType='not_an_enum' -> http=400 rejected",
            ["c2: metric='L_x' -> http=400 rejected"]))
        assert r["verdict_B"] == "NOT_TRIGGERED"

    def test_rule5_same_param_intermittent(self):
        """护栏：同参数一拒一收 = 间歇观测，非同族不一致 → 不触发。"""
        from check_physical_constraints import judge_physical
        r = judge_physical(self._chain(
            "c1: vectorFieldType='foo' -> http=200 (substituted default)",
            ["c2: vectorFieldType='foo' -> http=400 rejected"]))
        assert r["verdict_B"] == "NOT_TRIGGERED"

    def test_rule5_substitution_without_accept_mark(self):
        """护栏：替换语义行无接受标记（非 2xx 成功）→ 不触发。"""
        from check_physical_constraints import judge_physical
        r = judge_physical(self._chain(
            "c1: vectorFieldType='foo' -> http=400 (substituted default, rejected)",
            ["c2: idType='foo' -> http=400 rejected"]))
        assert r["verdict_B"] == "NOT_TRIGGERED"

    # ── 规则6 接口面对称 ──

    def test_rule6_interface_parity_triggers(self):
        """同参数同值：gRPC 拒绝、REST 接受 → CONFIRMED。

        原型形态（milvus_038 型现象结构）：group_by_field 作用于
        向量字段，gRPC 返回 unsupported data type，REST 返回 200。
        """
        from check_physical_constraints import judge_physical
        r = judge_physical(self._chain(
            "c1: REST group_by_field='vector_col' -> http=200, code=0 accepted",
            ["c2: gRPC group_by_field='vector_col' -> rejected ('unsupported data type')"]))
        assert r["verdict_B"] == "CONFIRMED"
        assert r["objective_constraint_class"] == "接口不对称"

    def test_rule6_reverse_direction_triggers(self):
        """方向无关：REST 拒绝、gRPC 接受同样触发。"""
        from check_physical_constraints import judge_physical
        r = judge_physical(self._chain(
            "c1: gRPC consistencyLevel='Bogus' -> accepted (code=0)",
            ["c2: REST consistencyLevel='Bogus' -> http=422 rejected"]))
        assert r["verdict_B"] == "CONFIRMED"
        assert r["objective_constraint_class"] == "接口不对称"

    def test_rule6_same_face_no_trigger(self):
        """护栏：同面（都 REST）一拒一收 = 间歇/同族问题，非接口不对称。"""
        from check_physical_constraints import judge_physical
        r = judge_physical(self._chain(
            "c1: REST group_by_field='c1' -> http=200 accepted",
            ["c2: REST group_by_field='c1' -> http=422 rejected"]))
        assert r["verdict_B"] == "NOT_TRIGGERED"

    def test_rule6_no_face_marker_no_trigger(self):
        """护栏：无面标记（靶面缺失，builder 未做对照取证）→ 不触发。"""
        from check_physical_constraints import judge_physical
        r = judge_physical(self._chain(
            "c1: group_by_field='vector_col' -> http=200 accepted",
            ["c2: group_by_field='vector_col' -> rejected unsupported"]))
        assert r["verdict_B"] == "NOT_TRIGGERED"

    def test_rule6_different_params_cross_face_no_trigger(self):
        """护栏：面不同但参数不同 → 接口面功能差（合法），非同参数不对称。"""
        from check_physical_constraints import judge_physical
        r = judge_physical(self._chain(
            "c1: REST group_by_field='vector_col' -> http=200 accepted",
            ["c2: gRPC output_fields='secret' -> rejected unsupported"]))
        assert r["verdict_B"] == "NOT_TRIGGERED"
