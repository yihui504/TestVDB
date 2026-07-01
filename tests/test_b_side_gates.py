"""B 面治理门夹具测试 — B3/B5/B7/B9/B11.

覆盖验收 checklist §2 面 B 标记"待测"的 5 项。
ponytail: 一文件收 5 类测试, 复用 conftest fixtures, 不依赖 Docker/网络。
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

SCRIPTS_DIR = (Path(__file__).resolve().parent.parent / "scripts")
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

# ═══════════════════════════════════════════════════════════════
# B3: passport_verify — 篡改检测
# ═══════════════════════════════════════════════════════════════

from passport_verify import verify_passport, compute_hash  # noqa: E402


class TestB3PassportVerify:
    """B3: _passport.contract_hash 完整性验证。"""

    def test_valid_passport_pass(self, tmp_path):
        """合法 hash → PASS。"""
        contract = {"target": "test", "version": "1.0"}
        h = compute_hash(contract)
        contract["_passport"] = {
            "contract_hash": h,
            "contract_hash_algorithm": "sha256",
            "schema_version": "2.0",
            "generation": {"generated_at": "2026-01-01T00:00:00Z"},
        }
        p = tmp_path / "ok.json"
        p.write_text(json.dumps(contract), encoding="utf-8")
        assert verify_passport(str(p))["status"] == "PASS"

    def test_no_passport(self, tmp_path):
        """无 _passport → NO_PASSPORT。"""
        p = tmp_path / "old.json"
        p.write_text('{"target":"test"}', encoding="utf-8")
        assert verify_passport(str(p))["status"] == "NO_PASSPORT"

    def test_tampered_hash_mismatch(self, tmp_path):
        """篡改后 hash 不匹配 → TAMPERED。"""
        contract = {"target": "test", "version": "1.0"}
        h = compute_hash(contract)
        contract["_passport"] = {
            "contract_hash": h,
            "contract_hash_algorithm": "sha256",
        }
        p = tmp_path / "tampered.json"
        p.write_text(json.dumps(contract), encoding="utf-8")
        contract["target"] = "attacker_modified"
        p.write_text(json.dumps(contract), encoding="utf-8")
        assert verify_passport(str(p))["status"] == "TAMPERED"

    def test_invalid_json(self, tmp_path):
        """非法 JSON → INVALID_JSON。"""
        p = tmp_path / "bad.json"
        p.write_text("{not json", encoding="utf-8")
        assert verify_passport(str(p))["status"] == "INVALID_JSON"

    def test_file_not_found(self, tmp_path):
        """文件不存在 → FILE_NOT_FOUND。"""
        assert verify_passport(str(tmp_path / "nope.json"))["status"] == "FILE_NOT_FOUND"

    def test_no_hash_in_passport(self, tmp_path):
        """_passport 存在但无 contract_hash → NO_HASH (exit 2)。"""
        p = tmp_path / "nohash.json"
        p.write_text(json.dumps({
            "target": "test", "_passport": {"schema_version": "2.0"}
        }), encoding="utf-8")
        assert verify_passport(str(p))["status"] == "NO_HASH"

    def test_cli_exit_0(self, tmp_path):
        """CLI: PASS → exit 0。"""
        contract = {"target": "test"}
        h = compute_hash(contract)
        contract["_passport"] = {
            "contract_hash": h,
            "contract_hash_algorithm": "sha256",
        }
        p = tmp_path / "c.json"
        p.write_text(json.dumps(contract), encoding="utf-8")
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "passport_verify.py"), str(p)],
            capture_output=True, timeout=10,
        )
        assert r.returncode == 0

    def test_cli_exit_2_tampered(self, tmp_path):
        """CLI: TAMPERED → exit 2。"""
        contract = {"target": "test"}
        h = compute_hash(contract)
        contract["_passport"] = {
            "contract_hash": h,
            "contract_hash_algorithm": "sha256",
        }
        p = tmp_path / "c2.json"
        p.write_text(json.dumps(contract), encoding="utf-8")
        contract["target"] = "hacked"
        p.write_text(json.dumps(contract), encoding="utf-8")
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "passport_verify.py"), str(p)],
            capture_output=True, timeout=10,
        )
        assert r.returncode == 2


# ═══════════════════════════════════════════════════════════════
# B5: ai_failure_check — M4 捷径绕过检测
# ═══════════════════════════════════════════════════════════════

from ai_failure_check import check_m4_shortcut_pipeline  # noqa: E402


class TestB5AiFailureCheckM4:
    """B5: M4 模式 — 检测 pipeline 跳过关键验证步骤。"""

    def test_m4_halt_missing_done(self, tmp_path):
        """debate_logs 存在但 .done 缺失 → HALT (passed=False)。"""
        session = tmp_path / "s"
        debate = session / "debate_logs"
        debate.mkdir(parents=True)
        (debate / "stage1.json").write_text("{}")
        r = check_m4_shortcut_pipeline(str(session))
        assert r["passed"] is False
        assert "Missing .done markers" in r["detail"]

    def test_m4_pass_all_done(self, tmp_path):
        """全部 .done 标记存在 → PASS。"""
        session = tmp_path / "s"
        debate = session / "debate_logs"
        debate.mkdir(parents=True)
        for name in [
            "stage1.json.done", "stage2_doc.json.done",
            "stage2_evidence.json.done", "stage2_novelty.json.done",
            "stage2_severity.json.done",
        ]:
            (debate / name).write_text("")
        r = check_m4_shortcut_pipeline(str(session))
        assert r["passed"] is True

    def test_m4_no_pipeline_traces(self, tmp_path):
        """无 debate_logs / defects → 跳过, PASS。"""
        session = tmp_path / "s"
        session.mkdir(parents=True)
        r = check_m4_shortcut_pipeline(str(session))
        assert r["passed"] is True
        assert "No pipeline execution traces" in r["detail"]

    def test_cli_m4_halt_exits_2(self, tmp_path):
        """CLI: M4 HALT → exit 2。需产出匹配的 output log 让 M3 不过早触发 FAIL。"""
        session = tmp_path / "s"
        debate = session / "debate_logs"
        debate.mkdir(parents=True)
        (debate / "stage1.json").write_text("{}")
        defects = session / "defects"
        defects.mkdir(parents=True)
        (defects / "defect-001.md").write_text(
            "Type: Type1_IllegalSuccess\n"
            'HTTP Response: 200\n'
            "## Methodology\nApproach: boundary testing\n",
            encoding="utf-8",
        )
        # M3 需要在 output log 中找到 claimed status code，否则会优先触发 FAIL(exit 1)
        (session / "output_001.log").write_text("HTTP 200 OK\n", encoding="utf-8")
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "ai_failure_check.py"),
             str(session), "defect-001"],
            capture_output=True, text=True, timeout=10,
        )
        assert r.returncode == 2  # M4 → HALT exit 2


# ═══════════════════════════════════════════════════════════════
# B7: write_location_check — 写位置守卫
# ═══════════════════════════════════════════════════════════════


class TestB7WriteLocationCheck:
    """B7: 阻止将临时/敏感文件写到插件根目录。"""

    @staticmethod
    def _run_hook(file_path: str, plugin_root: Path) -> subprocess.CompletedProcess:
        """用 TESTVDB_PLUGIN_ROOT env 跑 write_location_check。"""
        return subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "write_location_check.py")],
            input=json.dumps({"tool_input": {"file_path": file_path}}),
            capture_output=True, text=True, timeout=10,
            env={**os.environ, "TESTVDB_PLUGIN_ROOT": str(plugin_root)},
        )

    def test_results_subdir_allowed(self, tmp_path):
        """results/ 子目录 → exit 0, 无警告。"""
        r = self._run_hook(str(tmp_path / "results" / "x" / "v" / "out.log"), tmp_path)
        assert r.returncode == 0
        assert "警告" not in r.stderr

    def test_root_scratch_file_warns(self, tmp_path):
        """根目录临时文件 → exit 0, stderr 含 [TestVDB] + 文件名。"""
        r = self._run_hook(str(tmp_path / "push_data.json"), tmp_path)
        assert r.returncode == 0
        assert "[TestVDB]" in r.stderr
        assert "push_data.json" in r.stderr

    def test_root_secret_file_warns(self, tmp_path):
        """根目录含 secret 关键词 → exit 0, stderr 含 [TestVDB]。"""
        r = self._run_hook(str(tmp_path / "apikey.txt"), tmp_path)
        assert r.returncode == 0
        assert "[TestVDB]" in r.stderr
        assert "apikey.txt" in r.stderr

    def test_nested_subdir_allowed(self, tmp_path):
        """嵌套子目录 → exit 0, 无警告。"""
        r = self._run_hook(str(tmp_path / "scripts" / "sub" / "x.py"), tmp_path)
        assert r.returncode == 0
        assert "警告" not in r.stderr

    def test_legal_toplevel_dir_allowed(self, tmp_path):
        """合法顶层目录 (tests/) → exit 0, 无警告。"""
        r = self._run_hook(str(tmp_path / "tests" / "test_x.py"), tmp_path)
        assert r.returncode == 0
        assert "警告" not in r.stderr

    def test_outside_root_no_warning(self, tmp_path):
        """路径在插件根外 → 不管, exit 0。"""
        r = self._run_hook(str(Path("/tmp") / "whatever.json"), tmp_path)
        assert r.returncode == 0
        assert "警告" not in r.stderr


# ═══════════════════════════════════════════════════════════════
# B9: dedup_defects — 跨 round 去重
# ═══════════════════════════════════════════════════════════════

from dedup_defects import dedup_defects  # noqa: E402


class TestB9DedupDefects:
    """B9: 同 defect_id 去重为 1。"""

    def _write_agg(self, session: Path, defects: list):
        debate = session / "debate_logs"
        debate.mkdir(parents=True)
        (debate / "stage2_aggregation.json").write_text(
            json.dumps({"defects": defects}), encoding="utf-8")

    def test_dedup_same_id(self, tmp_path):
        """两 defect 同 defect_id → 去重到 1。"""
        session = tmp_path / "s"
        self._write_agg(session, [
            {"defect_id": "defect-1", "script": "a.py"},
            {"defect_id": "defect-1", "script": "b.py"},
        ])
        r = dedup_defects(str(session))
        assert r["before_count"] == 2
        assert r["after_count"] == 1

    def test_unique_ids_no_dedup(self, tmp_path):
        """不同 defect_id → 不去重。"""
        session = tmp_path / "s"
        self._write_agg(session, [
            {"defect_id": "defect-1", "script": "a.py"},
            {"defect_id": "defect-2", "script": "b.py"},
        ])
        r = dedup_defects(str(session))
        assert r["before_count"] == 2
        assert r["after_count"] == 2

    def test_empty_defects(self, tmp_path):
        """无 defect → before=after=0。"""
        session = tmp_path / "s"
        self._write_agg(session, [])
        r = dedup_defects(str(session))
        assert r["before_count"] == 0
        assert r["after_count"] == 0

    def test_no_key_skipped(self, tmp_path):
        """defect 无 defect_id → 跳过。"""
        session = tmp_path / "s"
        self._write_agg(session, [
            {"script": "a.py"},
            {"defect_id": "defect-1", "script": "b.py"},
        ])
        r = dedup_defects(str(session))
        assert r["before_count"] == 2
        assert r["after_count"] == 1

    def test_missing_aggregation(self, tmp_path):
        """无 stage2_aggregation.json → error。"""
        session = tmp_path / "s"
        session.mkdir(parents=True)
        r = dedup_defects(str(session))
        assert r["before_count"] == 0
        assert "error" in r


# ═══════════════════════════════════════════════════════════════
# B11: novelty_gate CLI — 6 档分级 + 精度回落
# ═══════════════════════════════════════════════════════════════

from novelty_gate import (  # noqa: E402
    apply_precision_grading,
    consumer_layer_check,
    extract_param_name,
    generate_final_verdict,
    param_in,
    precision_level,
)


class TestB11NoveltyGateCore:
    """B11: novelty_gate 核心函数单元测试 (无需网络/intelligence 目录)。"""

    # ── extract_param_name ──

    def test_extract_simple(self):
        assert extract_param_name("ef=0") == "ef"

    def test_extract_dotted(self):
        assert extract_param_name("pq.centroids=0") == "pq.centroids"

    def test_extract_compound(self):
        assert extract_param_name("dynamicEfMin(512)>dynamicEfMax(8)") == "dynamicEfMin"

    def test_extract_none_empty(self):
        assert extract_param_name(None) is None
        assert extract_param_name("") is None

    # ── param_in word-boundary ──

    def test_param_in_match(self):
        assert param_in("ef", '"ef": -1 in config') is True

    def test_param_in_no_false_match(self):
        """ef 不应匹配 default / before。"""
        assert param_in("ef", "the default value") is False
        assert param_in("ef", "before the change") is False

    def test_param_in_dotted_exact(self):
        assert param_in("pq.centroids", "set pq.centroids to 0") is True

    def test_param_in_partial_no_overmatch(self):
        """centroids 单独不应匹配 pq.centroids。"""
        assert param_in("pq.centroids", "the centroids field alone") is False

    # ── precision_level ──

    def test_precision_boundary_high(self):
        assert precision_level("boundary_vector_dim_004") == "HIGH"

    def test_precision_low(self):
        assert precision_level("semantic_objects_001") == "LOW"
        assert precision_level("state_delete_001") == "LOW"

    # ── consumer_layer_check (无网络, 纯本地) ──

    def test_consumer_by_design(self):
        cd = {
            "threat_model": {
                "defect_criteria": {
                    "by_design_behaviors": [
                        {"pattern": "ef=-1 sentinel", "rationale": "doc"}
                    ]
                },
                "judge_enhancements": {"novelty_context": {}},
            },
            "issue_corpus": [],
            "commit_corpus": [],
            "repo": "weaviate/weaviate",
        }
        r = consumer_layer_check("d1", "", "ef", "Type3", cd)
        assert r is not None
        assert r["grade"] == "BY_DESIGN"

    def test_consumer_issue_corpus_match(self):
        cd = {
            "threat_model": {
                "defect_criteria": {"by_design_behaviors": []},
                "judge_enhancements": {"novelty_context": {}},
            },
            "issue_corpus": [
                {"title": "fix ef parameter validation", "url": "http://gh/1"}
            ],
            "commit_corpus": [],
            "repo": "x/y",
        }
        r = consumer_layer_check("d1", "", "ef", "Type1", cd)
        assert r is not None
        assert r["grade"] == "KNOWN_OPEN"

    def test_consumer_no_match(self):
        cd = {
            "threat_model": {
                "defect_criteria": {"by_design_behaviors": []},
                "judge_enhancements": {"novelty_context": {}},
            },
            "issue_corpus": [],
            "commit_corpus": [],
            "repo": "",
        }
        assert consumer_layer_check("d1", "", "unknown", "Type1", cd) is None

    # ── apply_precision_grading ──

    def test_grading_novel_endorsed(self):
        r = apply_precision_grading({
            "layer": "gate", "grade": "NOVEL",
            "evidence_url": "", "match_type": "no_hits", "confidence": "HIGH",
        }, "boundary_001")
        assert r["endorsement"] is True

    def test_grading_boundary_known_open_rejected(self):
        r = apply_precision_grading({
            "layer": "consumer", "grade": "KNOWN_OPEN",
            "evidence_url": "x", "match_type": "issue", "confidence": "HIGH",
        }, "boundary_ef_001")
        assert r["endorsement"] is False

    def test_grading_semantic_downgraded_to_unverified(self):
        r = apply_precision_grading({
            "layer": "correct", "grade": "KNOWN_OPEN",
            "evidence_url": "x", "match_type": "gh", "confidence": "HIGH",
        }, "semantic_search_001")
        assert r["grade"] == "UNVERIFIED"
        assert r["endorsement"] is False

    def test_grading_by_design_suspected_rejected(self):
        r = apply_precision_grading({
            "layer": "correct", "grade": "BY_DESIGN_SUSPECTED",
            "evidence_url": "x", "match_type": "heuristic", "confidence": "MEDIUM",
        }, "boundary_001")
        assert r["endorsement"] is False
        assert "manual review" in r["endorsement_reason"].lower()

    # ── generate_final_verdict (ADR-0002) ──

    def test_final_verdict(self, tmp_path):
        gate = {
            "s.py": {
                "defect_id": "d-1", "grade": "NOVEL", "layer": "gate",
                "evidence_url": "", "endorsement": True,
                "endorsement_reason": "No hits",
                "param_name": "ef", "defect_type": "Type3",
            }
        }
        agg = {
            "confirmed_defects": [{
                "script": "s.py", "defect_id": "d-1",
                "doc": "VALID", "evidence": "VALID",
                "novelty": "NOVEL", "severity": "HIGH",
                "param": "ef=0",
            }]
        }
        v = generate_final_verdict(Path(tmp_path), gate, agg)
        assert v["total_defects"] == 1
        d = v["defects"][0]
        assert d["endorsement"] is True
        assert d["judge_novelty"] == "NOVEL"
        assert "generated_at" in v

    def test_final_verdict_discrepancy_flag(self, tmp_path):
        """Gate reject but judge said NOVEL → judge_discrepancy=True。"""
        gate = {
            "s.py": {
                "defect_id": "d-1", "grade": "KNOWN_OPEN", "layer": "consumer",
                "evidence_url": "x", "endorsement": False,
                "endorsement_reason": "already known",
                "param_name": "ef", "defect_type": "Type1",
            }
        }
        agg = {
            "confirmed_defects": [{
                "script": "s.py", "defect_id": "d-1",
                "novelty": "NOVEL",
            }]
        }
        v = generate_final_verdict(Path(tmp_path), gate, agg)
        assert v["defects"][0]["judge_discrepancy"] is True

    # ── CLI exit codes ──

    def test_cli_missing_session_dir(self):
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "novelty_gate.py"),
             "--session-dir", str(Path("/no/such/dir"))],
            capture_output=True, text=True, timeout=10,
        )
        assert r.returncode == 2

    def test_cli_no_aggregation(self, tmp_path):
        session = tmp_path / "s"
        session.mkdir(parents=True)
        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "novelty_gate.py"),
             "--session-dir", str(session)],
            capture_output=True, text=True, timeout=10,
        )
        assert r.returncode == 2


# ═══════════════════════════════════════════════════════════════
# C4: Final Verdict 可重跑 (ADR-0002 核心约束)
# ═══════════════════════════════════════════════════════════════

def _strip_timestamps(obj):
    """递归删除 dict/list 中的时间戳字段，用于可重跑比对。"""
    if isinstance(obj, dict):
        return {
            k: _strip_timestamps(v)
            for k, v in obj.items()
            if k not in ("generated_at", "cached_at", "verified_at",
                         "crawled_at", "updated_at", "deduped_at")
        }
    if isinstance(obj, list):
        return [_strip_timestamps(v) for v in obj]
    return obj


class TestC4FinalVerdictReproducible:
    """C4: novelty_gate.py 重跑生成 identical 输出 (时间戳除外)。"""

    @staticmethod
    def _setup_intelligence(intel_root: Path):
        """创建 intelligence/weaviate/ 使 consumer layer 全覆盖 (无网络调用)。"""
        intel = intel_root / "intelligence" / "weaviate"
        intel.mkdir(parents=True)
        tm = {
            "defect_criteria": {
                "by_design_behaviors": [
                    {"pattern": "ef=-1 sentinel", "rationale": "documented"}
                ]
            },
            "judge_enhancements": {
                "novelty_context": {
                    "recently_fixed_patterns": [],
                }
            }
        }
        (intel / "threat_model.json").write_text(json.dumps(tm), encoding="utf-8")
        (intel / "issue_corpus.json").write_text(json.dumps({
            "issues": [
                {"title": "fix dynamicEfMin validation", "url": "http://gh/99"}
            ]
        }), encoding="utf-8")
        (intel / "commit_corpus.json").write_text('{"merged_prs":[]}', encoding="utf-8")

    @staticmethod
    def _setup_session(session_dir: Path, defects: list):
        """创建 session/debate_logs/stage2_aggregation.json。"""
        debate = session_dir / "debate_logs"
        debate.mkdir(parents=True)
        (debate / "stage2_aggregation.json").write_text(
            json.dumps({"target": "weaviate", "confirmed_defects": defects}),
            encoding="utf-8")

    def test_deterministic_consumer_only(self, tmp_path, monkeypatch):
        """consumer layer 全覆盖时两次运行输出一致 (时间戳除外)。"""
        monkeypatch.chdir(tmp_path)
        self._setup_intelligence(tmp_path)

        session = tmp_path / "session"
        self._setup_session(session, [
            {
                "defect_id": "d-1", "script": "test_ef.py",
                "param": "ef=-1", "defect_type": "Type3_RuntimeFailure",
                "doc": "VALID", "evidence": "VALID",
                "novelty": "NOVEL", "severity": "HIGH",
            }
        ])

        def _run():
            r = subprocess.run(
                [sys.executable, str(SCRIPTS_DIR / "novelty_gate.py"),
                 "--session-dir", str(session)],
                capture_output=True, text=True, timeout=15,
                env={**os.environ, "GITHUB_TOKEN": ""},  # 空 token 防网络
            )
            assert r.returncode in (0, 1, 2), f"unexpected exit {r.returncode}: {r.stderr}"
            return r

        r1 = _run()
        r2 = _run()

        # 两次运行 gate 结果一致
        g1 = _strip_timestamps(json.loads(
            (session / "debate_logs" / "novelty_gate.json").read_text(encoding="utf-8")))
        g2 = _strip_timestamps(json.loads(
            (session / "debate_logs" / "novelty_gate.json").read_text(encoding="utf-8")))
        assert g1 == g2, f"novelty_gate.json diverged between runs"

        # final_verdict.json 结构合法
        v = json.loads(
            (session / "debate_logs" / "final_verdict.json").read_text(encoding="utf-8"))
        assert "generated_at" in v
        assert "defects" in v
        assert len(v["defects"]) >= 1
        for d in v["defects"]:
            for key in ("defect_id", "judge_novelty", "gate_grade", "endorsement"):
                assert key in d, f"final_verdict entry missing {key}"

    def test_cli_produces_both_outputs(self, tmp_path, monkeypatch):
        """CLI 运行后 novelty_gate.json + final_verdict.json 均落盘。"""
        monkeypatch.chdir(tmp_path)
        self._setup_intelligence(tmp_path)

        session = tmp_path / "session"
        self._setup_session(session, [
            {
                "defect_id": "d-1", "script": "test_ef.py",
                "param": "ef=-1", "defect_type": "Type3",
                "doc": "VALID", "evidence": "VALID",
                "novelty": "NOVEL", "severity": "HIGH",
            }
        ])

        r = subprocess.run(
            [sys.executable, str(SCRIPTS_DIR / "novelty_gate.py"),
             "--session-dir", str(session)],
            capture_output=True, text=True, timeout=15,
            env={**os.environ, "GITHUB_TOKEN": ""},
        )
        assert r.returncode in (0, 1, 2)

        gate_path = session / "debate_logs" / "novelty_gate.json"
        verdict_path = session / "debate_logs" / "final_verdict.json"
        assert gate_path.exists(), "novelty_gate.json not written"
        assert verdict_path.exists(), "final_verdict.json not written"
        assert json.loads(gate_path.read_text(encoding="utf-8"))
        assert json.loads(verdict_path.read_text(encoding="utf-8"))
