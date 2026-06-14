"""包装现有 _test_*.py 独立测试脚本，纳入 pytest 统一入口。

保留原脚本（仍可 `python scripts/_test_*.py` 独立跑），pytest 通过 subprocess
调用并断言退出码 0。这样批次 A 的 4 个独立测试不重写即纳入 pytest，零回归风险。
"""
import os
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent

EXISTING_TESTS = [
    "scripts/_test_reconstruct_context.py",
    "scripts/_test_validate_target_neutrality.py",
    "scripts/hooks/_test_pipeline_gate.py",
    "scripts/hooks/_test_stop_hook.py",
]


@pytest.mark.parametrize("script", EXISTING_TESTS)
def test_existing_script_passes(script):
    """现有 _test_*.py 应全部通过（退出码 0）。"""
    script_path = ROOT / script
    env = {**os.environ, "PYTHONUTF8": "1"}
    result = subprocess.run(
        [sys.executable, str(script_path)],
        capture_output=True, text=True,
        encoding="utf-8", errors="replace", timeout=60, env=env,
    )
    assert result.returncode == 0, (
        f"{script} 失败 (exit={result.returncode}):\n"
        f"--- stdout ---\n{result.stdout}\n--- stderr ---\n{result.stderr}"
    )
