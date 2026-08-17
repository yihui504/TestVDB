#!/usr/bin/env python3
"""Stage 1 API call format validator — AST-level check for safe_request() compliance.

Detects:
  - ANY bare .json() method call (variable.json(), library.func().json(), etc.) → REJECT
  - .json() calls inside safe_request() function bodies are excluded (safe harbor)
  - safe_request() defined but never called → WARN
  - All calls use safe_request() → PASS
  - chroma 脚本使用 raw /api/v1 REST 路径 → REJECT（冒烟实证 2026-08-17：
    chroma REST v1 已废弃返回 405，chroma 必须 SDK-first——见
    agents/_target_api_reference.md § "DB 特定 API 选择指南"。agent 规范有提示
    但 semantic agent 仍违规，此为机械防线）

Usage:
  python scripts/validate_api_format.py <session_dir>
"""
from __future__ import annotations
import ast, glob, json, os, re, sys

from _pipeline_utils import setup_encoding

setup_encoding()

# chroma 违规特征：raw REST 路径 /api/v1/collections...（SDK-first 例外：无）
CHROMA_RAW_REST_RE = re.compile(r"/api/v[12]/collections")


def _session_target(session_dir: str) -> str:
    """从 session 路径 results/<target>/<version>/<timestamp> 提取 target。"""
    parts = session_dir.replace("\\", "/").rstrip("/").split("/")
    for i, p in enumerate(parts):
        if p == "results" and i + 1 < len(parts):
            return parts[i + 1].lower()
    return ""


def validate_scripts(session_dir: str) -> list[dict]:
    """Scan all .py files in session_dir for API call format violations."""
    target = _session_target(session_dir)
    findings = []
    for f in sorted(glob.glob(os.path.join(session_dir, "**/*.py"), recursive=True)):
        if "/mre/" in f.replace("\\", "/"):
            continue
        with open(f, encoding="utf-8", errors="replace") as fh:
            src = fh.read()
            try:
                tree = ast.parse(src)
            except SyntaxError:
                continue

        has_safe_def = False
        has_safe_use = False
        bare_json = []
        safe_request_ranges = []

        for node in ast.walk(tree):
            if isinstance(node, ast.FunctionDef) and node.name == "safe_request":
                has_safe_def = True
                if hasattr(node, 'end_lineno'):
                    safe_request_ranges.append((node.lineno, node.end_lineno))
            if isinstance(node, ast.Call):
                if isinstance(node.func, ast.Name) and node.func.id == "safe_request":
                    has_safe_use = True
                # Detect ANY bare .json() method call on any object
                # (variable.json(), lib.func().json(), session.post().json(), etc.)
                # Excluding json.dumps()/json.loads() — those have attr="dumps"/"loads"
                if isinstance(node.func, ast.Attribute) and node.func.attr == "json":
                    bare_json.append(node.lineno)

        # Exclude .json() calls inside safe_request() function bodies (safe harbor)
        bare_json = [line for line in bare_json
                     if not any(start <= line <= end for start, end in safe_request_ranges)]

        issues = []
        if bare_json:
            issues.append(f"bare .json() at lines {bare_json}")
        if has_safe_def and not has_safe_use:
            issues.append("safe_request defined but never called")
        if target == "chroma" and CHROMA_RAW_REST_RE.search(src):
            issues.append("chroma script uses raw REST path /api/vN/collections — "
                          "chroma is SDK-first (REST v1 deprecated, returns 405); "
                          "use chromadb.HttpClient")
        if issues:
            rel = os.path.relpath(f, session_dir)
            findings.append({"file": rel, "issues": issues})

    return findings


def _self_check():
    """守护 AST parser 的 safe harbor 陷阱（ponytail: parser 必须留 check）。

    覆盖 5 case：(a) safe_request 体外 bare .json() 被检测；(b) 体内 .json() 排除；
    (c) json.dumps/loads 不被误判（attr=dumps/loads ≠ json）；(d) safe_request
    定义但未调用 → WARN；(e) 全 safe_request 调用 → clean（无 issue）。
    合成 .py fixture，不依赖真实 session。
    """
    import tempfile, shutil
    failures = []

    def expect(cond, msg):
        if not cond:
            failures.append(msg)

    tmp = tempfile.mkdtemp(prefix="vaf_selfcheck_")
    try:
        # Case 1: bare .json() outside safe_request → detected
        with open(os.path.join(tmp, "bad.py"), "w", encoding="utf-8") as f:
            f.write("import requests\n"
                    "def attack(s):\n"
                    "    r = requests.post(s)\n"
                    "    return r.json()\n")
        # Case 2: .json() inside safe_request body → excluded (safe harbor)
        with open(os.path.join(tmp, "safe.py"), "w", encoding="utf-8") as f:
            f.write("import requests\n"
                    "def safe_request(url, **kw):\n"
                    "    r = requests.post(url, **kw)\n"
                    "    return r.json()\n"
                    "def attack(s):\n"
                    "    return safe_request(s)\n")
        # Case 3: json.dumps/loads → not flagged (attr=dumps/loads)
        with open(os.path.join(tmp, "dumps.py"), "w", encoding="utf-8") as f:
            f.write("import json\n"
                    "def attack(s):\n"
                    "    data = json.dumps({'a': 1})\n"
                    "    return json.loads(data)\n")
        # Case 4: safe_request defined but never called → WARN
        with open(os.path.join(tmp, "unused.py"), "w", encoding="utf-8") as f:
            f.write("import requests\n"
                    "def safe_request(url, **kw):\n"
                    "    return requests.post(url, **kw)\n"
                    "def attack(s):\n"
                    "    return requests.get(s)\n")
        # Case 5: clean — safe_request defined and used, no bare .json()
        with open(os.path.join(tmp, "clean.py"), "w", encoding="utf-8") as f:
            f.write("import requests\n"
                    "def safe_request(url, **kw):\n"
                    "    return requests.post(url, **kw).json()\n"
                    "def attack(s):\n"
                    "    return safe_request(s)\n")

        findings = validate_scripts(tmp)
        by_file = {f["file"]: f["issues"] for f in findings}

        bad = by_file.get("bad.py", [])
        expect(any("bare .json()" in i for i in bad),
               f"bad.py bare .json() 应被检测，实际 issues={bad}")

        safe = by_file.get("safe.py", [])
        expect(not any("bare .json()" in i for i in safe),
               f"safe.py safe_request 内 .json() 应排除（safe harbor），实际 issues={safe}")

        dumps = by_file.get("dumps.py", [])
        expect(not any("bare .json()" in i for i in dumps),
               f"dumps.py json.dumps/loads 不应被误判，实际 issues={dumps}")

        unused = by_file.get("unused.py", [])
        expect(any("safe_request defined but never called" in i for i in unused),
               f"unused.py safe_request 未调用应 WARN，实际 issues={unused}")

        expect("clean.py" not in by_file,
               f"clean.py 全 safe_request 应无 issue（不在 findings），实际 {by_file.get('clean.py')}")

        # mre/ skip (Windows path root cause 完整化，与 detect_risky L39 同范式)
        mre_dir = os.path.join(tmp, "mre")
        os.makedirs(mre_dir, exist_ok=True)
        with open(os.path.join(mre_dir, "should_skip.py"), "w", encoding="utf-8") as f:
            f.write("import requests\nrequests.get('x').json()\n")  # bare .json() in mre/
        findings_with_mre = validate_scripts(tmp)
        mre_flagged = any("should_skip" in f["file"] for f in findings_with_mre)
        expect(not mre_flagged,
               f"mre/should_skip.py 应被跳过（Windows path 完整化），findings={[f['file'] for f in findings_with_mre]}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if failures:
        for m in failures:
            print(f"  FAIL: {m}", file=sys.stderr)
        print(f"self-check FAILED: {len(failures)} assertion(s)", file=sys.stderr)
        sys.exit(1)
    print("self-check OK")


def main():
    if "--self-check" in sys.argv:
        _self_check()
        sys.exit(0)
    if len(sys.argv) < 2:
        print("Usage: python scripts/validate_api_format.py <session_dir>", file=sys.stderr)
        sys.exit(1)

    session_dir = sys.argv[1]
    if not os.path.isdir(session_dir):
        print(f"ERROR: {session_dir} not found", file=sys.stderr)
        sys.exit(2)

    findings = validate_scripts(session_dir)

    if findings:
        print(json.dumps({"api_format_violations": findings}, indent=2))
        for f in findings:
            has_bare = any("bare .json()" in i for i in f["issues"])
            print(f'  {"REJECT" if has_bare else "WARN"}: {f["file"]}')
        rejects = [f for f in findings if any("bare .json()" in i for i in f["issues"])]
        if rejects:
            print(f"[Stage 1] API Format Check: {len(rejects)} scripts REJECTED (bare .json() chain)")
            sys.exit(1)  # Non-zero exit signals REJECT to caller (mine.md Step 8c)
        # Warn-only findings (safe_request defined but never called) — exit 0
        print("[Stage 1] API Format Check: warnings only, no bare .json() chains rejected")
        sys.exit(0)
    else:
        print("[Stage 1] API Format Check: all scripts pass")
        sys.exit(0)


if __name__ == "__main__":
    main()
