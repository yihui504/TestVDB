"""find_python.py 不应硬编码用户路径（review L-N10 回归保护，批次 C [1]）。"""
from pathlib import Path

FIND_PY = Path(__file__).resolve().parent.parent / "scripts" / "find_python.py"


def test_no_hardcoded_user_path():
    """find_python.py 不含硬编码用户路径（C:\\Users\\11428）。"""
    content = FIND_PY.read_text(encoding="utf-8")
    assert "11428" not in content, "find_python.py 含硬编码用户路径 '11428'"


def test_uses_dynamic_localappdata():
    """应通过 LOCALAPPDATA 动态构建路径，而非写死。"""
    content = FIND_PY.read_text(encoding="utf-8")
    assert "LOCALAPPDATA" in content, "find_python.py 应用 LOCALAPPDATA 动态构建"


def test_iterates_versions():
    """应遍历多个 Python 版本（动态），覆盖 3.9-3.13。"""
    content = FIND_PY.read_text(encoding="utf-8")
    # 至少覆盖 3.9（最低）和一个高版本
    assert "39" in content
    assert any(v in content for v in ("313", "312", "311"))
