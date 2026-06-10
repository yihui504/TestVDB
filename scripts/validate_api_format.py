#!/usr/bin/env python3
"""Stage 1 API call format validator — AST-level check for safe_request() compliance.

Detects:
  - ANY bare .json() method call (variable.json(), library.func().json(), etc.) → REJECT
  - .json() calls inside safe_request() function bodies are excluded (safe harbor)
  - safe_request() defined but never called → WARN
  - All calls use safe_request() → PASS

Usage:
  python scripts/validate_api_format.py <session_dir>
"""
from __future__ import annotations
import ast, glob, json, os, sys

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


def validate_scripts(session_dir: str) -> list[dict]:
    """Scan all .py files in session_dir for API call format violations."""
    findings = []
    for f in sorted(glob.glob(os.path.join(session_dir, "**/*.py"), recursive=True)):
        if "/mre/" in f:
            continue
        with open(f, encoding="utf-8", errors="replace") as fh:
            try:
                tree = ast.parse(fh.read())
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
        if issues:
            rel = os.path.relpath(f, session_dir)
            findings.append({"file": rel, "issues": issues})

    return findings


def main():
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
