#!/usr/bin/env python3
"""
TestVDB Defect Verification Script — Post-Reporter Quality Gate

Reads defect-N.md files from a session directory and verifies:
1. Evidence chain completeness (Ring 1/2/3)
2. Script error false-positive detection
3. VERDICT consistency (log verdict vs report claim)

(Severity calibration removed 2026-09-02: claimed in orchestrator.md 8f.5 and
this docstring but never implemented — see orchestrator.md's note; severity
grading is the reporter's job at report time.)

Outputs defect-review.md with CONFIRMED / FALSE_POSITIVE / NEEDS_IMPROVEMENT.

Usage:
    python scripts/verify_defects.py <session_dir> [--target qdrant|milvus|weaviate|pgvector]
"""

import os
import sys
import json
import re
import glob

from _pipeline_utils import setup_encoding, read_json, find_logs

setup_encoding()

# ── helpers ──────────────────────────────────────────────

def safe_read(filepath):
    """Read file safely, return content or None."""
    try:
        with open(filepath, encoding="utf-8", errors="replace") as f:
            return f.read()
    except (FileNotFoundError, PermissionError):
        return None

def safe_read_json(filepath):
    """Read JSON file safely, return dict or None."""
    content = safe_read(filepath)
    if content is None:
        return None
    try:
        return json.loads(content)
    except json.JSONDecodeError:
        return None

def extract_verdict_from_log(log_path):
    """Extract VERDICT lines from execution log."""
    content = safe_read(log_path)
    if not content:
        return []
    verdicts = []
    for line in content.split("\n"):
        if "VERDICT" in line.upper() and ":" in line:
            verdicts.append(line.strip())
    return verdicts

def extract_script_errors_from_log(log_path):
    """Check if log contains SCRIPT_ERROR markers."""
    content = safe_read(log_path)
    if not content:
        return False
    markers = ["SCRIPT_ERROR", "TypeError:", "AttributeError:",
               "json.decoder.JSONDecodeError", "KeyError:", "IndexError:"]
    return any(m.lower() in content.lower() for m in markers)

def check_ring_completeness(defect_content):
    """Check if defect report has Ring 1, 2, 3 evidence."""
    rings = {
        "Ring 1": bool(re.search(r"Ring 1|Contract Clause|契约条款|constraint_id", defect_content, re.IGNORECASE)),
        "Ring 2": bool(re.search(r"Ring 2|Document Reference|文档引用|source_url", defect_content, re.IGNORECASE)),
        "Ring 3": bool(re.search(r"Ring 3|Actual Behavior|实际行为|HTTP Request|HTTP Response", defect_content, re.IGNORECASE)),
    }
    return rings

def find_script(session_dir, name):
    """在脚本子目录查找 name.py。"""
    if not name:
        return None
    for d in ("boundary_scripts", "state_scripts", "scripts", "debate_logs"):
        p = os.path.join(session_dir, d, name + ".py")
        if os.path.exists(p):
            return p
    return None

def check_methodology_pitfalls(script_content, log_path):
    """检测 SQL 测试方法论陷阱 — 脚本自身 bug 产生的伪 VERDICT（advisory）。
    源自 pgvector v0.8.3 实战：6 假阳性中 defect-1/2/5 属此类。
    返回命中描述列表；空 = 无命中。"""
    pitfalls = []
    log_c = (safe_read(log_path) or "").lower() if log_path else ""
    # ponytail: defect-1 事务中止遮蔽 — log 层最强信号
    if log_c and "current transaction is aborted" in log_c:
        pitfalls.append("log 含 'current transaction is aborted' — 脚本共享连接未隔离事务，首个错误后所有断言被遮蔽")
    sc = script_content or ""
    scl = sc.lower()
    # ponytail: defect-5 伪索引测试（全程顺序扫描）+ defect-2 浮点字符串比较 — 脚本层静态特征
    if "order by" in scl and "limit" in scl and "index" in scl and "create index" not in scl:
        pitfalls.append("脚本测索引语义但无 CREATE INDEX — '索引 vs 非索引'对比无效（两者都顺序扫描）")
    if "::text" in sc and (".6f" in sc or ".4f" in sc or "checksum" in scl):
        pitfalls.append("浮点值用 ::text vs f-string 字符串比较 — 格式表示差异伪不一致")
    return pitfalls

# ── main verification ──────────────────────────────────

def verify_session(session_dir, target="unknown"):
    """Verify all defect reports in a session directory."""
    defects_dir = os.path.join(session_dir, "defects")
    if not os.path.isdir(defects_dir):
        print(f"ERROR: defects/ directory not found in {session_dir}")
        return {"error": "defects/ not found"}

    # Find defect files (exclude -enhanced duplicates to avoid double-checking)
    defect_files = sorted(glob.glob(os.path.join(defects_dir, "defect-*.md")))
    defect_files = [f for f in defect_files if "-enhanced" not in os.path.basename(f)]

    if not defect_files:
        print(f"WARNING: No defect-*.md files found in {defects_dir}")
        return {"error": "no defect files"}

    results = []
    for defect_path in defect_files:
        basename = os.path.basename(defect_path)
        content = safe_read(defect_path)
        if not content:
            results.append({"file": basename, "status": "ERROR", "reason": "Cannot read file"})
            continue

        # 1. Evidence chain check
        rings = check_ring_completeness(content)
        missing_rings = [r for r, present in rings.items() if not present]

        # 2. 定位关联 log — 统一提取 script_name，用 find_logs 精确匹配
        # ponytail: 修复 reporter 引用名与实际 output_*.log 不匹配导致 5/5 误报 "No VERDICT line"（设计 §4.1）
        script_name = None
        for pat in (
            r"output_([\w-]+)\.log",                                      # 直接 output_*.log 引用
            r"Log:\s*`?output_([a-z_0-9]+)\.log",                         # "Log:" 字段
            r"debate_logs/([\w-]+)\.py",                                  # debate_logs/{name}.py
            r"(?:boundary_scripts|state_scripts|scripts)/([\w-]+)\.py",   # 其他脚本目录
        ):
            m = re.search(pat, content)
            if m:
                script_name = m.group(1)
                break

        script_error = False
        log_verdicts = []
        log_path = None
        if script_name:
            # find_logs 精确匹配 output_*{script_name}*.log，空 list 才算找不到
            candidates = find_logs(session_dir, script_name)
            if candidates:
                log_path = str(candidates[0])
                # 兼容 executor 的 output_*.log.done 命名（文件不存在时看 .done）
                if not os.path.exists(log_path) and os.path.exists(log_path + ".done"):
                    log_path = log_path + ".done"
            if log_path:
                script_error = extract_script_errors_from_log(log_path)
                log_verdicts = extract_verdict_from_log(log_path)

        # 2.5 方法论陷阱检测（pgvector v0.8.3 实战：defect-1/2/5 类假阳性根源）
        script_path = find_script(session_dir, script_name) if script_name else None
        # Task 4b fix: Handle None script_path safely
        script_content = safe_read(script_path) or "" if script_path else ""
        methodology = check_methodology_pitfalls(script_content, log_path)

        # 3. Classification
        if script_error:
            status = "NEEDS_IMPROVEMENT"
            reason = "Execution log contains Python errors — may co-occur with real DB defect, manual verification needed"
        elif methodology:
            status = "FALSE_POSITIVE"
            reason = f"方法论陷阱（脚本自身 bug 导致伪 VERDICT）: {methodology[0]}"
        elif missing_rings:
            status = "NEEDS_IMPROVEMENT"
            reason = f"Missing evidence rings: {', '.join(missing_rings)}"
        elif any("NO_DEFECT" in v for v in log_verdicts):
            status = "FALSE_POSITIVE"
            reason = f"Log VERDICT says NO_DEFECT but report claims defect"
        elif not log_verdicts:
            status = "NEEDS_IMPROVEMENT"
            reason = "No VERDICT line found in execution log"
        else:
            status = "CONFIRMED"
            reason = "Evidence complete, log verdicts consistent"

        results.append({
            "file": basename,
            "status": status,
            "reason": reason,
            "missing_rings": missing_rings,
            "script_error": script_error,
            "log_verdicts": log_verdicts[:3],  # first 3 verdicts for reference
        })

    return {"results": results, "total": len(results)}


def write_review_md(session_dir, verification_result, target):
    """Write defect-review.md to session directory."""
    results = verification_result.get("results", [])
    if not results:
        return

    confirmed = [r for r in results if r["status"] == "CONFIRMED"]
    false_pos = [r for r in results if r["status"] == "FALSE_POSITIVE"]
    needs_imp = [r for r in results if r["status"] == "NEEDS_IMPROVEMENT"]

    lines = [
        f"# Defect Review — {target}",
        "",
        f"**Session**: {os.path.basename(session_dir)}",
        f"**Verified**: {len(results)} defect reports",
        "",
        "## Summary",
        "",
        f"| Status | Count |",
        f"|--------|-------|",
        f"| CONFIRMED | {len(confirmed)} |",
        f"| FALSE_POSITIVE | {len(false_pos)} |",
        f"| NEEDS_IMPROVEMENT | {len(needs_imp)} |",
        "",
        "## Details",
        "",
    ]

    for r in results:
        lines.append(f"### {r['file']} — {r['status']}")
        lines.append(f"- **Reason**: {r['reason']}")
        if r.get("missing_rings"):
            lines.append(f"- **Missing Rings**: {', '.join(r['missing_rings'])}")
        if r.get("log_verdicts"):
            for v in r["log_verdicts"]:
                lines.append(f"- **Log Verdict**: `{v[:120]}`")
        lines.append("")

    lines.append("---")
    lines.append("*Generated by verify_defects.py*")

    review_path = os.path.join(session_dir, "defect-review.md")
    with open(review_path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    print(f"defect-review.md written to {review_path}")
    return review_path


def write_review_json(session_dir, verification_result, target):
    """落盘结构化 defect_review.json — 供 reporter retry 时按 defect 精确读取（设计 §3.2）。

    每条 {id, file, status, reason}；reporter retry 只重写 status=NEEDS_IMPROVEMENT 的项，
    不动 CONFIRMED；FALSE_POSITIVE 的项由调用方删除对应 defect-N.md。
    """
    results = verification_result.get("results", [])
    entries = [
        {
            "id": r["file"].replace("defect-", "").replace(".md", ""),
            "file": r["file"],
            "status": r["status"],
            "reason": r["reason"],
        }
        for r in results
    ]
    out = {"target": target, "total": len(results), "defects": entries}
    path = os.path.join(session_dir, "defect_review.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(out, f, indent=2, ensure_ascii=False)
    print(f"defect_review.json written to {path}")
    return path


# ── self-check ──────────────────────────────────────────

def _self_check():
    """守护非平凡逻辑（ponytail: parser + classifier + exit router 必须留 check）。

    覆盖：VERDICT 行解析、SCRIPT_ERROR marker 检测、Ring regex、find_script、
    分类决策矩阵 6 分支（verify_session）、exit code 映射（FP 优先于 NI）、
    write_review_json id 提取（reporter retry 按此 id 精确读取）。
    合成 fixture，不依赖真实 session。
    """
    import tempfile, shutil
    failures = []

    def expect(cond, msg):
        if not cond:
            failures.append(msg)

    tmp = tempfile.mkdtemp(prefix="vd_selfcheck_")
    try:
        # 1. extract_verdict_from_log — VERDICT 行解析
        log = os.path.join(tmp, "v.log")
        with open(log, "w", encoding="utf-8") as f:
            f.write("VERDICT: DEFECT_FOUND\nx\nVERDICT: NO_DEFECT\n")
        v = extract_verdict_from_log(log)
        expect(len(v) == 2, f"extract_verdict_from_log 应解析 2 VERDICT 行，实际 {len(v)}")
        expect(extract_verdict_from_log(os.path.join(tmp, "missing.log")) == [],
               "extract_verdict_from_log 缺失文件应返回 []")

        # 2. extract_script_errors_from_log — markers 检测
        with open(log, "w", encoding="utf-8") as f:
            f.write("x\nTypeError: boom\n")
        expect(extract_script_errors_from_log(log) is True,
               "extract_script_errors_from_log 应检测 TypeError:")
        with open(log, "w", encoding="utf-8") as f:
            f.write("clean\nVERDICT: DEFECT_FOUND\n")
        expect(extract_script_errors_from_log(log) is False,
               "extract_script_errors_from_log 干净 log 应 False")

        # 3. check_ring_completeness — Ring 1/2/3 regex
        full = ("Ring 1 Contract Clause Ring 2 Document Reference source_url "
                "Ring 3 Actual Behavior HTTP Response")
        expect(all(check_ring_completeness(full).values()),
               "check_ring_completeness 完整 content 应全 True")
        expect(not any(check_ring_completeness("nothing relevant").values()),
               "check_ring_completeness 空 content 应全 False")

        # 4. find_script — 目录查找
        os.makedirs(os.path.join(tmp, "debate_logs"))
        with open(os.path.join(tmp, "debate_logs", "foo.py"), "w", encoding="utf-8") as f:
            f.write("# t")
        expect(find_script(tmp, "foo") is not None, "find_script 应找到 debate_logs/foo.py")
        expect(find_script(tmp, "missing") is None, "find_script 缺失应 None")
        expect(find_script(tmp, "") is None, "find_script 空名应 None")

        # 5. 分类决策矩阵 6 分支 — 构造 fixture session 跑 verify_session
        ddir = os.path.join(tmp, "defects")
        os.makedirs(ddir)
        full_rings = ("Ring 1 Contract Clause\n"
                      "Ring 2 Document Reference source_url\n"
                      "Ring 3 Actual Behavior HTTP Response\n")

        def wdef(name, body):
            with open(os.path.join(ddir, name), "w", encoding="utf-8") as f:
                f.write(body)

        def wlog(name, body):
            with open(os.path.join(tmp, name), "w", encoding="utf-8") as f:
                f.write(body)

        # A: script_error → NEEDS_IMPROVEMENT
        wdef("defect-1.md", full_rings + "Log: output_a.log\n")
        wlog("output_a.log", "TypeError: boom\n")
        # B: methodology → FALSE_POSITIVE（脚本测索引但无 CREATE INDEX）
        wdef("defect-2.md", full_rings + "Log: output_b.log\n")
        wlog("output_b.log", "VERDICT: DEFECT_FOUND\n")
        os.makedirs(os.path.join(tmp, "scripts"))
        with open(os.path.join(tmp, "scripts", "b.py"), "w", encoding="utf-8") as f:
            f.write("SELECT * FROM t ORDER BY x LIMIT 1; -- index scan\n")
        # C: missing_rings → NEEDS_IMPROVEMENT
        wdef("defect-3.md", "Log: output_c.log\n")
        wlog("output_c.log", "VERDICT: DEFECT_FOUND\n")
        # D: NO_DEFECT verdict → FALSE_POSITIVE
        wdef("defect-4.md", full_rings + "Log: output_d.log\n")
        wlog("output_d.log", "VERDICT: NO_DEFECT\n")
        # E: no verdict → NEEDS_IMPROVEMENT
        wdef("defect-5.md", full_rings + "Log: output_e.log\n")
        wlog("output_e.log", "some log without verdict line\n")
        # F: 干净 + DEFECT_FOUND + 完整 rings → CONFIRMED
        wdef("defect-6.md", full_rings + "Log: output_f.log\n")
        wlog("output_f.log", "VERDICT: DEFECT_FOUND\n")

        res = verify_session(tmp, "testdb")
        by_file = {r["file"]: r["status"] for r in res.get("results", [])}
        expect(by_file.get("defect-1.md") == "NEEDS_IMPROVEMENT",
               f"A script_error → NEEDS_IMPROVEMENT，实际 {by_file.get('defect-1.md')}")
        expect(by_file.get("defect-2.md") == "FALSE_POSITIVE",
               f"B methodology → FALSE_POSITIVE，实际 {by_file.get('defect-2.md')}")
        expect(by_file.get("defect-3.md") == "NEEDS_IMPROVEMENT",
               f"C missing_rings → NEEDS_IMPROVEMENT，实际 {by_file.get('defect-3.md')}")
        expect(by_file.get("defect-4.md") == "FALSE_POSITIVE",
               f"D NO_DEFECT verdict → FALSE_POSITIVE，实际 {by_file.get('defect-4.md')}")
        expect(by_file.get("defect-5.md") == "NEEDS_IMPROVEMENT",
               f"E no verdict → NEEDS_IMPROVEMENT，实际 {by_file.get('defect-5.md')}")
        expect(by_file.get("defect-6.md") == "CONFIRMED",
               f"F 干净+DEFECT_FOUND → CONFIRMED，实际 {by_file.get('defect-6.md')}")

        # 6. exit code 映射（CLI 末尾语义：FP>0→2 优先于 NI>0→1，再 else→0）
        def exit_for(fp, ni):
            if fp > 0:
                return 2
            if ni > 0:
                return 1
            return 0
        expect(exit_for(0, 0) == 0, "exit: 全 CONFIRMED → 0")
        expect(exit_for(0, 1) == 1, "exit: NI → 1")
        expect(exit_for(1, 0) == 2, "exit: FP → 2")
        expect(exit_for(1, 1) == 2, "exit: FP+NI → 2（FP 优先于 NI）")

        # 7. write_review_json id 提取（reporter retry 按 id 精确读取）
        import contextlib, io
        review = {"results": [{"file": "defect-7.md", "status": "CONFIRMED", "reason": "x"}]}
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            out_path = write_review_json(tmp, review, "testdb")
        with open(out_path, encoding="utf-8") as f:
            rj = json.load(f)
        expect(rj["defects"][0]["id"] == "7",
               f"write_review_json id 提取 defect-7.md → '7'，实际 {rj['defects'][0].get('id')}")

        # 8. subprocess 跑真实 CLI — 验证 exit code 与 CLI 末尾逻辑一致
        # ponytail: exit_for 内部测语义；subprocess 测真实 CLI（防 FP/NI exit code 被交换）
        # fixture: FP=2 (B,D), NI=3 (A,C,E), CONFIRMED=1 (F) → FP>0 优先 → exit 2
        import subprocess as _sp
        proc = _sp.run([sys.executable, os.path.abspath(__file__), tmp, "--target", "testdb"],
                       capture_output=True, text=True)
        expect(proc.returncode == 2,
               f"subprocess CLI exit 应=2（FP 优先于 NI），实际 {proc.returncode}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if failures:
        for m in failures:
            print(f"  FAIL: {m}", file=sys.stderr)
        print(f"self-check FAILED: {len(failures)} assertion(s)", file=sys.stderr)
        sys.exit(1)
    print("self-check OK")


# ── CLI ──────────────────────────────────────────────────

if __name__ == "__main__":
    if "--self-check" in sys.argv:
        _self_check()
        sys.exit(0)
    if len(sys.argv) < 2:
        print("Usage: python scripts/verify_defects.py <session_dir> [--target qdrant]")
        sys.exit(1)

    session_dir = sys.argv[1]
    target = "unknown"
    for i, arg in enumerate(sys.argv[2:], 2):
        if arg == "--target":
            if i + 1 < len(sys.argv):
                target = sys.argv[i + 1]
            else:
                print("ERROR: --target requires a value (e.g. --target qdrant)", file=sys.stderr)
                sys.exit(1)

    if not os.path.isdir(session_dir):
        print(f"ERROR: Session directory not found: {session_dir}")
        sys.exit(4)  # 4=启动错误（避开 exit 2=FALSE_POSITIVE 语义）

    result = verify_session(session_dir, target)

    if "error" in result:
        print(f"Verification failed: {result['error']}")
        sys.exit(3)  # No data to verify — caller should distinguish from PASS

    write_review_md(session_dir, result, target)
    write_review_json(session_dir, result, target)

    statuses = [r["status"] for r in result.get("results", [])]
    confirmed = sum(1 for s in statuses if s == "CONFIRMED")
    false_pos = sum(1 for s in statuses if s == "FALSE_POSITIVE")
    needs_imp = sum(1 for s in statuses if s == "NEEDS_IMPROVEMENT")
    print(f"\nDone: {confirmed} CONFIRMED, {false_pos} FALSE_POSITIVE, {needs_imp} NEEDS_IMPROVEMENT")

    # Exit code 分级（设计 §3.2 / §4.2）：
    #   0 = 全 CONFIRMED | 1 = 有 NEEDS_IMPROVEMENT | 2 = 有 FALSE_POSITIVE（优先于 1）
    #   3 = 无数据 | 4 = session_dir 不存在
    if false_pos > 0:
        sys.exit(2)
    if needs_imp > 0:
        sys.exit(1)
    sys.exit(0)
