#!/usr/bin/env python3
"""TestVDB 组件 C 测试 — reconstruct_context 端点速查表注入。

自造临时 session tree（不依赖 results/，因其被 gitignore），调用真实
reconstruct() + format_text()，断言 target_reference 与速查表 section。
"""
from __future__ import annotations

import json
import os
import shutil
import sys
import tempfile
from pathlib import Path

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from reconstruct_context import reconstruct, format_text  # noqa: E402

PASSED: list[str] = []
FAILED: list[str] = []


def _check(name: str, cond: bool, detail: str = "") -> None:
    (PASSED if cond else FAILED).append(name)
    print(f"[{'PASS' if cond else 'FAIL'}] {name}" + (f" — {detail}" if detail and not cond else ""))


def _scaffold() -> Path:
    """造临时 session tree：root/results/testdb/1.0/，含 pipeline_state + structured_contract。"""
    root = Path(tempfile.mkdtemp(prefix="recon_"))
    ver_dir = root / "results" / "testdb" / "1.0"
    ver_dir.mkdir(parents=True)

    pipeline_state = {
        "version": 3,
        "session_id": "test-001",
        "target": "testdb",
        "version_target": "1.0",
        "current_round": 1,
        "max_rounds": 1,
        "phase": "ATTACK_GEN",
        "phases_completed": ["ROUND_START"],
        "project_root": str(root),
        "session_dir": "results/testdb/1.0",
        "global_state": {"total_defects_confirmed": 0, "consecutive_no_defect_rounds": 0,
                         "docker_container_running": True},
    }
    (ver_dir / "pipeline_state.json").write_text(json.dumps(pipeline_state), encoding="utf-8")

    contract = {
        "target": "testdb",
        "version": "1.0",
        "api_endpoints": [
            {"path": "collections+{collection_name}", "method": "PUT", "category": "collections",
             "source_url": "https://example.test/docs/create", "parameters": []},
            {"path": "collections+{collection_name}+points+search", "method": "POST",
             "category": "search", "source_url": "https://example.test/docs/search", "parameters": []},
            {"path": "collections+{collection_name}+points", "method": "PUT", "category": "points",
             "source_url": "https://example.test/docs/upsert", "parameters": []},
        ],
        "data_types": [{"name": "vector", "type": "array"}],
    }
    (ver_dir / "structured_contract.json").write_text(json.dumps(contract), encoding="utf-8")
    return root


def main() -> int:
    root = _scaffold()
    try:
        session_dir = str(root / "results" / "testdb" / "1.0")
        data = reconstruct(session_dir)

        # 1. target_reference 存在
        tr = data.get("target_reference")
        _check("1 target_reference 存在", tr is not None)

        # 2. target 正确
        _check("2 target=testdb", tr is not None and tr.get("target") == "testdb")

        # 3. endpoint_cheatsheet 非空且含全部 3 个端点
        cs = tr.get("endpoint_cheatsheet", []) if tr else []
        _check("3 cheatsheet 含 3 端点", len(cs) == 3, f"got {len(cs)}")
        if cs:
            _check("3b cheatsheet 条目含 method/path/category",
                   all({"method", "path", "category"} <= set(e.keys()) for e in cs))

        # 4. key_data_types 透传
        _check("4 key_data_types 透传",
               tr is not None and len(tr.get("key_data_types", [])) == 1)

        # 5. format_text 输出含速查表 section
        text = format_text(data)
        _check("5 format_text 含『端点速查表』section", "端点速查表" in text)
        _check("6 format_text 含 target 标注", "Target: testdb" in text or "testdb" in text)
        _check("7 format_text 含端点 markdown 表", "| Method |" in text or "Method" in text)
        # 速查表 section 在「本轮关键信息」之前（先确认两者都存在，避免 .index 崩溃）
        _check("8 速查表 section 在「本轮关键信息」之前",
               "端点速查表" in text and "本轮关键信息" in text
               and text.index("端点速查表") < text.index("本轮关键信息"))
    finally:
        shutil.rmtree(root, ignore_errors=True)

    print(f"\n{len(PASSED)}/{len(PASSED) + len(FAILED)} passed")
    return 1 if FAILED else 0


if __name__ == "__main__":
    sys.exit(main())
