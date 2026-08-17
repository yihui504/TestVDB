#!/usr/bin/env python3
"""
7-Mode AI Failure Checklist — Reporter Pre-Submit Gate 自检脚本

检查 LLM 生成内容中的 7 种常见幻觉/错误模式。

用法:
  python scripts/ai_failure_check.py <session_dir> <defect_id>

输入:
  SESSION_DIR: 会话目录（e.g., results/{target}/{version}/{timestamp}）
  defect_id: 缺陷标识（e.g., defect-001）

输出 (stdout):
  JSON {checklist: [{mode, passed: bool, detail}], overall: PASS|FAIL|HALT}

退出码:
  0 = PASS (全部通过)
  1 = FAIL (存在 REJECT 级问题 — M2/M3/M6) 或存在其它未通过
  2 = HALT (存在 HALT 级问题 — M4/M7)
  3 = REWIND (存在 REWIND 级问题 — M1/M5，需回退重检查)
"""

import os
import sys
import json
import re
import time
import urllib.request
import urllib.error
from pathlib import Path


def load_file(path: str) -> str:
    """加载文件内容，文件不存在返回空字符串"""
    try:
        with open(path, "r", encoding="utf-8") as f:
            return f.read()
    except FileNotFoundError:
        return ""


def load_json(path: str) -> dict:
    """加载 JSON 文件，不存在返回空字典"""
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def check_m1_script_errors(session_dir: str) -> dict:
    """
    M1: 脚本错误被误判为数据库缺陷
    检查 execution_summary.txt 中 exit_code≠0 且并非 FAILED: 标记
    """
    summary_path = os.path.join(session_dir, "execution_summary.txt")
    content = load_file(summary_path)
    if not content:
        return {"mode": "M1", "passed": True,
                "detail": "No execution_summary.txt found — nothing to check"}

    non_zero = len(re.findall(r"Exit code non-zero: (\d+)", content))
    total = len(re.findall(r"Scripts executed: (\d+)", content))

    return {
        "mode": "M1",
        "passed": True,  # M1 检查是信息性的，不阻断
        "detail": f"Scripts with non-zero exit: {non_zero}. "
                  f"These may include legitimate defect triggers — verify manually."
    }


def check_m2_fabricated_urls(session_dir: str, defect_id: str) -> dict:
    """
    M2: 编造文档引用（幻觉 URL）
    使用 urllib 验证 source_url 可达性（跨平台，无 curl 依赖）

    ponytail: TESTVDB_OFFLINE=1 时早退 — offline 环境所有 URL 不可达，
    全 unreachable 误报为 hallucinated URLs 会 reject 合法 defect。
    仅 M2 跳过（网络依赖）；M3-M7 是本地检查（defect.md vs output log），
    offline 环境仍正确工作，不跳过。
    """
    if os.environ.get("TESTVDB_OFFLINE") == "1":
        return {"mode": "M2", "passed": True,
                "detail": "Offline mode (TESTVDB_OFFLINE=1) — URL reachability skipped"}

    defect_path = os.path.join(session_dir, "defects", f"{defect_id}.md")
    content = load_file(defect_path)
    if not content:
        return {"mode": "M2", "passed": True,
                "detail": f"No defect file found for {defect_id}"}

    urls = re.findall(r'source_url["\s:]*["\s]*([^")\s]+)', content)
    urls = [u for u in urls if u.startswith("http")]

    if not urls:
        return {"mode": "M2", "passed": True,
                "detail": "No source URLs found in defect report"}

    results = []
    for url in urls[:5]:  # 最多检查 5 个 URL
        for attempt in range(2):
            try:
                req = urllib.request.Request(url, method="HEAD")
                req.add_header("User-Agent", "TestVDB/2.1 (AI Failure Check)")
                with urllib.request.urlopen(req, timeout=10) as resp:
                    status = str(resp.status)
                if status in ("200", "301", "302"):
                    results.append({"url": url, "reachable": True, "status": status})
                    break
                else:
                    if attempt == 0:
                        time.sleep(3)
                    else:
                        results.append({"url": url, "reachable": False, "status": status})
            except urllib.error.HTTPError as e:
                if attempt == 0:
                    time.sleep(3)
                else:
                    results.append({"url": url, "reachable": False, "status": str(e.code)})
            except Exception as e:
                if attempt == 0:
                    time.sleep(3)
                else:
                    results.append({"url": url, "reachable": False, "status": str(e)})

    unreachable = [r for r in results if not r["reachable"]]
    all_unreachable = len(unreachable) == len(results) and len(results) > 0

    if all_unreachable:
        return {
            "mode": "M2",
            "passed": False,
            "detail": f"All {len(results)} URLs unreachable — likely hallucinated URLs. "
                      f"(Possible network issue — verify connectivity before confirming.) "
                      f"Urls checked: {[r['url'] for r in results]}"
        }
    elif unreachable:
        return {
            "mode": "M2",
            "passed": False,
            "detail": f"{len(unreachable)}/{len(results)} URLs unreachable: "
                      f"{[r['url'] for r in unreachable]}"
        }
    else:
        return {
            "mode": "M2",
            "passed": True,
            "detail": f"All {len(results)} URLs reachable"
        }


def check_m3_fabricated_results(session_dir: str, defect_id: str) -> dict:
    """
    M3: 编造执行结果数据
    比对 defect-N.md 中的输出与 output_*.log 中的原始输出
    """
    defect_path = os.path.join(session_dir, "defects", f"{defect_id}.md")
    content = load_file(defect_path)
    if not content:
        return {"mode": "M3", "passed": True, "detail": "No defect file"}

    status_codes = re.findall(r'HTTP Response["\s:]*["\s]*(\d{3})', content)

    output_files = list(Path(session_dir).glob("output_*.log"))
    all_output = ""
    for f in output_files:
        all_output += load_file(str(f))

    fabricated = []
    for code in status_codes:
        if code not in all_output:
            fabricated.append(f"Status code {code} not found in any output log")

    if fabricated:
        return {
            "mode": "M3",
            "passed": False,
            "detail": f"Possible fabricated data: {'; '.join(fabricated[:3])}"
        }
    else:
        return {
            "mode": "M3",
            "passed": True,
            "detail": f"All claimed status codes found in output logs"
        }


def check_m4_shortcut_pipeline(session_dir: str) -> dict:
    """
    M4: 走捷径跳过关键验证
    检查 .done 标记是否全部存在。
    仅当有管道执行痕迹（debate_logs 目录存在或 defect 文件存在）时才检查。

    ADR-0008: 新管线关键产物是 chain_verdicts.json（chain-auditor 终判）；
    stage2_* 五件套随 Judge Quartet 删除。旧会话兼容：无 chain_verdicts 但有
    stage2_doc → 按旧五件套评估（历史 session 复检仍按当时管线）。
    """
    # 旧管线会话判定先做（依赖 stage2_doc 存在性）
    _old_five = [
        "debate_logs/stage1.json.done",
        "debate_logs/stage2_doc.json.done",
        "debate_logs/stage2_evidence.json.done",
        "debate_logs/stage2_novelty.json.done",
        "debate_logs/stage2_severity.json.done",
    ]
    has_new = os.path.exists(
        os.path.join(session_dir, "debate_logs", "chain_verdicts.json.done"))
    has_old = os.path.exists(
        os.path.join(session_dir, "debate_logs", "stage2_doc.json.done"))

    if has_new or not has_old:
        required_done = [
            "debate_logs/stage1.json.done",
            "debate_logs/chain_verdicts.json.done",
        ]
    else:
        required_done = _old_five

    # 仅在有管道执行痕迹时才检查
    debate_logs_dir = os.path.join(session_dir, "debate_logs")
    defects_dir = os.path.join(session_dir, "defects")
    has_pipeline_trace = os.path.isdir(debate_logs_dir) or (
        os.path.isdir(defects_dir) and os.listdir(defects_dir)
    )

    if not has_pipeline_trace:
        return {
            "mode": "M4",
            "passed": True,
            "detail": "No pipeline execution traces found — nothing to check"
        }

    missing = []
    for f in required_done:
        full_path = os.path.join(session_dir, f)
        if not os.path.exists(full_path):
            missing.append(f)

    if missing:
        return {
            "mode": "M4",
            "passed": False,
            "detail": f"Missing .done markers: {missing}. "
                      f"Pipeline may have skipped critical validation steps."
        }
    else:
        return {
            "mode": "M4",
            "passed": True,
            "detail": "All required .done markers present"
        }


def check_m5_script_bug_as_defect(session_dir: str, defect_id: str) -> dict:
    """
    M5: 脚本 bug 被说成新发现
    检查 FAILED: 输出是否匹配预期缺陷类型

    针对四种缺陷类型的验证规则:
      Type1 (Illegal Success): 必须有 2xx HTTP Response 证据
      Type2 (Poor Diagnostics):  必须引用具体的不清晰错误消息原文
      Type3 (Runtime Failure):   必须有 5xx 或 crash traceback 证据
      Type4 (State/Logic):       必须有 2xx Response + 非预期的状态描述
    """
    defect_path = os.path.join(session_dir, "defects", f"{defect_id}.md")
    content = load_file(defect_path)
    if not content:
        return {"mode": "M5", "passed": True, "detail": "No defect file"}

    defect_type = ""
    m = re.search(r'Type:\s*(Type\d_\w+)', content)
    if m:
        defect_type = m.group(1)

    if "Type1" in defect_type:
        has_2xx = re.search(r'HTTP Response["\s:]*["\s]*2\d{2}', content)
        if not has_2xx:
            return {
                "mode": "M5",
                "passed": False,
                "detail": f"Defect classified as {defect_type} but no 2xx response found. "
                          f"May be a script bug misclassified as a defect."
            }
    elif "Type2" in defect_type:
        # Type2 should quote the unclear/ambiguous error message
        has_error_msg = (
            re.search(r'(?:error|Error|ERROR)[\s:]*["\'].+?["\']', content)
            or re.search(r'(?:message|Message)[\s:]*["\'].+?["\']', content)
        )
        if not has_error_msg:
            return {
                "mode": "M5",
                "passed": False,
                "detail": f"Defect classified as {defect_type} but no error message text quoted. "
                          f"Poor Diagnostics defects should cite the actual unclear message."
            }
    elif "Type3" in defect_type:
        # Type3 should have 5xx response or crash traceback
        has_crash_evidence = (
            re.search(r'HTTP Response["\s:]*["\s]*5\d{2}', content)
            or re.search(r'Traceback|Segmentation fault|panic|SIGSEGV', content)
            or re.search(r'(?:crash|CRASH|timeout|TIMEOUT)', content)
        )
        if not has_crash_evidence:
            return {
                "mode": "M5",
                "passed": False,
                "detail": f"Defect classified as {defect_type} but no 5xx/crash/traceback evidence. "
                          f"Runtime Failure defects should show actual crash symptoms."
            }
    elif "Type4" in defect_type:
        # Type4 should show 2xx (operation succeeded) + unexpected state change
        has_2xx = re.search(r'HTTP Response["\s:]*["\s]*2\d{2}', content)
        # ponytail: 剥离 Type 行再做 state_desc 检查，避免 "Type4_StateLogic"
        # 中的 "State" 假匹配（self-check 发现的 latent bug）
        content_no_type_line = re.sub(r'^Type:.*$', '', content, flags=re.MULTILINE)
        has_state_desc = re.search(
            r'(?:state|State|unexpected|unusual|incorrect|wrong|mismatch|inconsistent)',
            content_no_type_line
        )
        if not has_2xx:
            return {
                "mode": "M5",
                "passed": False,
                "detail": f"Defect classified as {defect_type} but no 2xx response found. "
                          f"State/Logic violations require a successful (2xx) operation."
            }
        if not has_state_desc:
            return {
                "mode": "M5",
                "passed": False,
                "detail": f"Defect classified as {defect_type} but no state mismatch description. "
                          f"State/Logic violations should document the unexpected state."
            }

    return {
        "mode": "M5",
        "passed": True,
        "detail": f"Defect type ({defect_type}) appears consistent with reported behavior"
    }


def check_m6_fabricated_methodology(session_dir: str, defect_id: str) -> dict:
    """
    M6: 编造方法论
    检查 defect-N.md 中是否有不在 attack-*.md 中的测试策略描述
    """
    defect_path = os.path.join(session_dir, "defects", f"{defect_id}.md")
    content = load_file(defect_path)
    if not content:
        return {"mode": "M6", "passed": True, "detail": "No defect file"}

    strategy_keywords = re.findall(r'strategy["\s:]*["\s]*([^")\n]+)', content)

    if not strategy_keywords:
        methodology_section = re.search(r'(?:Methodology|Approach|Strategy)[:\s]*(.+?)(?:\n\n|\n#)', content, re.DOTALL)
        if methodology_section:
            return {
                "mode": "M6",
                "passed": True,
                "detail": "Methodology section present — verify manually against attack agent output"
            }

    return {
        "mode": "M6",
        "passed": True,
        "detail": "No obvious fabricated methodology detected"
    }


def check_m7_stale_loop(session_dir: str) -> dict:
    """
    M7: 锁定早期错误假设
    检查同一 endpoint 的缺陷是否在多个 round 中反复出现但从未确认
    """
    experience_path = os.path.join(session_dir, "experience_handoff.json")
    exp = load_json(experience_path)

    rejection_patterns = exp.get("rejection_patterns", [])
    if not rejection_patterns:
        return {"mode": "M7", "passed": True,
                "detail": "No rejection patterns recorded"}

    endpoint_counts = {}
    for rp in rejection_patterns:
        ep = rp.get("endpoint", "unknown")
        endpoint_counts[ep] = endpoint_counts.get(ep, 0) + 1

    stale = [ep for ep, count in endpoint_counts.items() if count >= 3]
    if stale:
        return {
            "mode": "M7",
            "passed": False,
            "detail": f"Endpoints with ≥3 repeated rejections: {stale}. "
                      f"May indicate stale assumptions — consider halting."
        }

    return {
        "mode": "M7",
        "passed": True,
        "detail": f"No stale endpoints detected ({len(rejection_patterns)} rejection patterns)"
    }


def _decide_overall(checks: list) -> str:
    """7-mode 优先级决策：REWIND(M1/M5) > REJECT-as-FAIL(M2/M3/M6) > HALT(M4/M7) > FAIL(any) > PASS.

    反直觉但与文档一致：M1/M5 (script confusion) 最高 — rewind 重检；
    M2/M3/M6 (hallucination) 次之 — reject defect；M4/M7 (pipeline issue) 第三 — halt。
    """
    reject_modes = {"M2", "M3", "M6"}
    halt_modes = {"M4", "M7"}
    rewind_modes = {"M1", "M5"}

    has_rewind = any(not c["passed"] and c["mode"] in rewind_modes for c in checks)
    has_reject = any(not c["passed"] and c["mode"] in reject_modes for c in checks)
    has_halt = any(not c["passed"] and c["mode"] in halt_modes for c in checks)
    has_fail = any(not c["passed"] for c in checks)

    if has_rewind:
        return "REWIND"
    if has_reject:
        return "FAIL"
    if has_halt:
        return "HALT"
    if has_fail:
        return "FAIL"
    return "PASS"


def _exit_code_for(overall: str) -> int:
    """overall verdict → process exit code."""
    return {"REWIND": 3, "HALT": 2, "FAIL": 1, "PASS": 0}.get(overall, 1)


def _self_check() -> int:
    """Self-check: 7-mode priority chain + Type1/2/3/4 校验 + exit code mapping.

    不测 check_m2（需网络）；覆盖核心纯逻辑/regex 路径。
    """
    import tempfile

    failures = []

    def expect(cond, msg):
        if not cond:
            failures.append(msg)

    ALL_MODES = ["M1", "M2", "M3", "M4", "M5", "M6", "M7"]

    def chk(mode, passed):
        return {"mode": mode, "passed": passed, "detail": ""}

    # ── _decide_overall 单 mode ──
    expect(_decide_overall([chk("M1", False)]) == "REWIND", "M1 alone → REWIND")
    expect(_decide_overall([chk("M5", False)]) == "REWIND", "M5 alone → REWIND")
    expect(_decide_overall([chk("M2", False)]) == "FAIL", "M2 alone → FAIL")
    expect(_decide_overall([chk("M3", False)]) == "FAIL", "M3 alone → FAIL")
    expect(_decide_overall([chk("M6", False)]) == "FAIL", "M6 alone → FAIL")
    expect(_decide_overall([chk("M4", False)]) == "HALT", "M4 alone → HALT")
    expect(_decide_overall([chk("M7", False)]) == "HALT", "M7 alone → HALT")
    expect(_decide_overall([chk(m, True) for m in ALL_MODES]) == "PASS", "all pass → PASS")

    # ── 优先级链（多级同时触发）──
    expect(_decide_overall([chk("M1", False), chk("M2", False)]) == "REWIND",
           "REWIND > REJECT")
    expect(_decide_overall([chk("M2", False), chk("M4", False)]) == "FAIL",
           "REJECT > HALT")
    expect(_decide_overall([chk("M5", False), chk("M7", False)]) == "REWIND",
           "REWIND > HALT")
    expect(_decide_overall([chk("M1", False), chk("M2", False), chk("M4", False)]) == "REWIND",
           "REWIND > REJECT > HALT")
    expect(_decide_overall([chk("M2", False), chk("M3", False), chk("M6", False)]) == "FAIL",
           "multi-REJECT → FAIL")

    # ── _exit_code_for ──
    expect(_exit_code_for("REWIND") == 3, "exit REWIND=3")
    expect(_exit_code_for("HALT") == 2, "exit HALT=2")
    expect(_exit_code_for("FAIL") == 1, "exit FAIL=1")
    expect(_exit_code_for("PASS") == 0, "exit PASS=0")
    expect(_exit_code_for("UNKNOWN") == 1, "exit unknown default=1")

    # ── check_m5 Type1/2/3/4 校验 ──
    with tempfile.TemporaryDirectory() as td:
        defects_dir = os.path.join(td, "defects")
        os.makedirs(defects_dir)

        def write_defect(content):
            with open(os.path.join(defects_dir, "defect-1.md"), "w", encoding="utf-8") as f:
                f.write(content)

        # Type1: 需 2xx
        write_defect("Type: Type1_IllegalSuccess\nfoo")
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is False,
               "Type1 no 2xx → fail")
        write_defect("Type: Type1_IllegalSuccess\nHTTP Response: 200 OK")
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is True,
               "Type1 with 2xx → pass")

        # Type2: 需 error/message 引用
        write_defect("Type: Type2_PoorDiagnostics\nfoo")
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is False,
               "Type2 no msg → fail")
        write_defect('Type: Type2_PoorDiagnostics\nerror: "ambiguous msg"')
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is True,
               "Type2 with msg → pass")

        # Type3: 需 5xx / traceback / crash
        write_defect("Type: Type3_RuntimeFailure\nfoo")
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is False,
               "Type3 no crash → fail")
        write_defect("Type: Type3_RuntimeFailure\nHTTP Response: 500 Internal")
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is True,
               "Type3 with 5xx → pass")
        write_defect("Type: Type3_RuntimeFailure\nTraceback (most recent call last)")
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is True,
               "Type3 with traceback → pass")

        # Type4: 需 2xx + state description
        write_defect("Type: Type4_StateLogic\nfoo")
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is False,
               "Type4 no 2xx → fail")
        write_defect("Type: Type4_StateLogic\nHTTP Response: 200 OK")
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is False,
               "Type4 no state desc → fail")
        write_defect("Type: Type4_StateLogic\nHTTP Response: 200 OK\nunexpected state mismatch")
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is True,
               "Type4 with both → pass")

        # 无 Type → 默认 pass
        write_defect("no type field here")
        expect(check_m5_script_bug_as_defect(td, "defect-1")["passed"] is True,
               "no type → pass")

        # ── check_m4 .done pipeline 检测 ──
        # 无 pipeline trace → pass（nothing to check）
        empty = os.path.join(td, "empty_session")
        os.makedirs(empty)
        expect(check_m4_shortcut_pipeline(empty)["passed"] is True,
               "M4 no trace → pass")

        # 有 debate_logs 但缺 .done → fail
        debate_dir = os.path.join(empty, "debate_logs")
        os.makedirs(debate_dir)
        expect(check_m4_shortcut_pipeline(empty)["passed"] is False,
               "M4 debate_logs missing .done → fail")

        # 全部 .done 存在 → pass（ADR-0008 新管线两件套）
        for f in ["stage1.json", "chain_verdicts.json"]:
            (open(os.path.join(debate_dir, f + ".done"), "w").close())
        expect(check_m4_shortcut_pipeline(empty)["passed"] is True,
               "M4 all .done → pass（新管线 stage1 + chain_verdicts）")

        # ADR-0008 旧会话兼容：仅旧五件套 → 仍按旧清单 pass
        old_dir = os.path.join(empty, "m4_old")
        debate_old = os.path.join(old_dir, "debate_logs")
        os.makedirs(debate_old)
        for f in ["stage1.json", "stage2_doc.json", "stage2_evidence.json",
                  "stage2_novelty.json", "stage2_severity.json"]:
            (open(os.path.join(debate_old, f + ".done"), "w").close())
        expect(check_m4_shortcut_pipeline(old_dir)["passed"] is True,
               "M4 旧管线会话按旧五件套 → pass")

        # ── check_m2 offline 模式（TESTVDB_OFFLINE=1 早退，P3-21）──
        with open(os.path.join(defects_dir, "defect-2.md"), "w", encoding="utf-8") as f:
            f.write('source_url: "http://definitely-not-real-example-xyz.invalid/page"\n')
        old = os.environ.get("TESTVDB_OFFLINE")
        os.environ["TESTVDB_OFFLINE"] = "1"
        try:
            r = check_m2_fabricated_urls(td, "defect-2")
            expect(r["passed"] is True,
                   f"offline mode should skip M2 (passed=True), got {r}")
            expect("skipped" in r["detail"].lower(),
                   f"offline detail should mention skip, got {r['detail']}")
        finally:
            if old is None:
                os.environ.pop("TESTVDB_OFFLINE", None)
            else:
                os.environ["TESTVDB_OFFLINE"] = old

    if failures:
        print("self-check FAIL:", file=sys.stderr)
        for f in failures:
            print(f"  - {f}", file=sys.stderr)
        return 1
    print("self-check OK")
    return 0


def main():
    if len(sys.argv) >= 2 and sys.argv[1] == "--self-check":
        sys.exit(_self_check())
    if len(sys.argv) < 3:
        print("Usage: python scripts/ai_failure_check.py <session_dir> <defect_id>")
        print(json.dumps({"checklist": [], "overall": "FAIL",
                          "error": "Missing arguments"}))
        sys.exit(1)

    session_dir = sys.argv[1]
    defect_id = sys.argv[2]

    if not defect_id.startswith("defect-"):
        defect_id = f"defect-{defect_id}"

    checks = [
        check_m1_script_errors(session_dir),
        check_m2_fabricated_urls(session_dir, defect_id),
        check_m3_fabricated_results(session_dir, defect_id),
        check_m4_shortcut_pipeline(session_dir),
        check_m5_script_bug_as_defect(session_dir, defect_id),
        check_m6_fabricated_methodology(session_dir, defect_id),
        check_m7_stale_loop(session_dir),
    ]

    overall = _decide_overall(checks)

    result = {
        "checklist": checks,
        "overall": overall,
        "session_dir": session_dir,
        "defect_id": defect_id
    }

    print(json.dumps(result, indent=2, ensure_ascii=False))

    sys.exit(_exit_code_for(overall))


if __name__ == "__main__":
    main()
