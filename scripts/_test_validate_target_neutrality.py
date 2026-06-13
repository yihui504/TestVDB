#!/usr/bin/env python3
"""TestVDB 组件 B 测试 — validate_target_neutrality.py target-aware 检测。

自造临时 session tree（structured_contract.json + 若干 fixture 脚本），
subprocess 跑真实验证器，断言 exit code。
"""
from __future__ import annotations

import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

VALIDATOR = Path(__file__).resolve().parent / "validate_target_neutrality.py"
PASSED: list[str] = []
FAILED: list[str] = []


def _check(name: str, got: int, want: int, out: str) -> None:
    ok = got == want
    (PASSED if ok else FAILED).append(name)
    print(f"[{'PASS' if ok else 'FAIL'}] {name}: exit={got} (want {want})")
    if not ok:
        print(f"    --- output ---\n{out}    --------------")


def _scaffold(target: str) -> tuple[Path, Path]:
    root = Path(tempfile.mkdtemp(prefix="neut_"))
    sd = root / "session"
    sd.mkdir()
    (sd / "structured_contract.json").write_text(
        json.dumps({"target": target, "api_endpoints": []}), encoding="utf-8"
    )
    return root, sd


def _run(sd: Path) -> tuple[int, str]:
    proc = subprocess.run(
        [sys.executable, str(VALIDATOR), str(sd)],
        capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=30,
    )
    return proc.returncode, proc.stdout + proc.stderr


# fixture 脚本
QDRANT_SIG = '''import requests
resp = requests.post("http://host:6333/collections/mycoll/points/search", json={})
x = resp.json()["result"]
'''
WEAVIATE_SIG = '''import requests
resp = requests.get("http://host:8080/v1/objects")
x = resp.json()["data"]
'''
CLEAN = '''import os, requests
url = os.environ["TESTVDB_DB_URL"]
resp = requests.post(url + "/some/path", json={})
'''


def main() -> int:
    # 1. target=weaviate 但脚本含 qdrant 签名 → REJECT(1)
    root, sd = _scaffold("weaviate")
    try:
        (sd / "bad.py").write_text(QDRANT_SIG, encoding="utf-8")
        rc, out = _run(sd)
        _check("1 weaviate+qdrant-sig → REJECT(1)", rc, 1, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 2. target=qdrant 脚本含 qdrant 签名 → PASS(0)，不误伤
    root, sd = _scaffold("qdrant")
    try:
        (sd / "ok.py").write_text(QDRANT_SIG, encoding="utf-8")
        rc, out = _run(sd)
        _check("2 qdrant+qdrant-sig → PASS(0)", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 3. target=weaviate 脚本含 weaviate 签名 → PASS
    root, sd = _scaffold("weaviate")
    try:
        (sd / "ok.py").write_text(WEAVIATE_SIG, encoding="utf-8")
        rc, out = _run(sd)
        _check("3 weaviate+weaviate-sig → PASS(0)", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 4. target=weaviate 干净脚本(用 env url) → PASS
    root, sd = _scaffold("weaviate")
    try:
        (sd / "clean.py").write_text(CLEAN, encoding="utf-8")
        rc, out = _run(sd)
        _check("4 weaviate+clean → PASS(0)", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 5. 端口误报控制：6333 出现在无关数字上下文不应触发(需 :6333 或 6333/)
    root, sd = _scaffold("weaviate")
    try:
        (sd / "num.py").write_text('x = 6333 + 1\nprint(x)\n', encoding="utf-8")
        rc, out = _run(sd)
        _check("5 裸数字 6333 非 URL 上下文 → PASS(0)", rc, 0, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    # 6. 契约在父级 version 目录（pipeline v3 布局），_load_target 路径回退仍读得到
    root = Path(tempfile.mkdtemp(prefix="neut_path_"))
    version_dir = root / "results" / "weaviate" / "1.38.0"
    sd = version_dir / "20260613-162742"   # timestamp 目录 = session_dir（脚本所在）
    sd.mkdir(parents=True)
    (version_dir / "structured_contract.json").write_text(
        json.dumps({"target": "weaviate"}), encoding="utf-8")   # 契约在 version 目录，非 session_dir
    (sd / "bad.py").write_text(QDRANT_SIG, encoding="utf-8")
    try:
        rc, out = _run(sd)
        _check("6 契约在父级 version 目录 → 仍 REJECT(1)", rc, 1, out)
    finally:
        shutil.rmtree(root, ignore_errors=True)

    print(f"\n{len(PASSED)}/{len(PASSED) + len(FAILED)} passed")
    return 1 if FAILED else 0


if __name__ == "__main__":
    sys.exit(main())
