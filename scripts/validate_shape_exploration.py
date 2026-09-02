#!/usr/bin/env python3
"""Gate: 检查 attack agent 是否产出 shape_exploration 清单 + novel_candidate 脚本阈值。

v2.3 反"LLM 不自发泛化"——14 轮证明 attack agent 即使注入 threat_model 也只测具体向量。
本 gate 强制 attack 产出参数族枚举清单 + 至少 MIN novel_candidate 脚本，未达 → DEBATE_S1 打回。

用法：
    py -3 scripts/validate_shape_exploration.py <session_dir> [--min-novel 3]

退出码：
    0 = PASS（清单产出 + novel_candidate 脚本 ≥ 阈值）
    1 = FAIL（清单缺失 或 novel_candidate 不足）
"""
import os
import re
import sys
from pathlib import Path

MIN_NOVEL_DEFAULT = 3


def find_shape_exploration_files(session_dir: str) -> list[Path]:
    """找 debate_logs/shape_exploration_*.md 文件。"""
    dl = Path(session_dir) / "debate_logs"
    if not dl.exists():
        return []
    return sorted(dl.glob("shape_exploration_*.md"))


def count_novel_candidate_scripts(session_dir: str) -> tuple[int, int, list[str]]:
    """统计 attack 脚本中 exploration_target 标注：novel_candidate vs regression。

    返回 (novel_count, regression_count, novel_script_names)。

    glob 修复（2026-09-02 声称审计）：原先只扫 debate_logs/attack_*.py——
    攻击脚本实际按 boundary_*/state_*/semantic_* 前缀写在各通道子目录
    （boundary_scripts/、state_scripts/、scripts/），即使被调用也恒计 0。
    现改为递归全扫 session_dir 下的 *.py（gate 只认 exploration_target 标注，
    全扫防目录漂移）。
    """
    base = Path(session_dir)
    if not base.exists():
        return (0, 0, [])
    novel, reg = 0, 0
    novel_names = []
    for sp in base.rglob("*.py"):
        try:
            txt = sp.read_text(encoding="utf-8", errors="replace")
        except Exception:
            continue
        # 匹配 # exploration_target: novel_candidate | regression
        m = re.search(r"exploration_target\s*:\s*(\w+)", txt, re.I)
        if not m:
            continue
        target = m.group(1).lower()
        if "novel" in target:
            novel += 1
            novel_names.append(sp.name)
        elif "regress" in target:
            reg += 1
    return (novel, reg, novel_names)


def main():
    if len(sys.argv) < 2:
        print("usage: validate_shape_exploration.py <session_dir> [--min-novel N]", file=sys.stderr)
        sys.exit(2)
    session_dir = sys.argv[1]
    min_novel = MIN_NOVEL_DEFAULT
    for i, a in enumerate(sys.argv):
        if a == "--min-novel" and i + 1 < len(sys.argv):
            min_novel = int(sys.argv[i + 1])

    print(f"=== Shape Exploration Gate (v2.3) ===")
    print(f"session_dir: {session_dir}")
    print(f"min_novel_required: {min_novel}")
    print()

    # 1. shape_exploration 清单
    expl_files = find_shape_exploration_files(session_dir)
    print(f"shape_exploration 清单文件: {len(expl_files)}")
    for f in expl_files:
        print(f"  ✓ {f.name}")

    # 2. novel_candidate 脚本
    novel, reg, novel_names = count_novel_candidate_scripts(session_dir)
    print(f"\nexploration_target 标注脚本:")
    print(f"  novel_candidate: {novel}")
    print(f"  regression: {reg}")
    if novel_names:
        print(f"  novel 脚本:")
        for n in novel_names[:10]:
            print(f"    - {n}")

    # 3. 判定
    print()
    fails = []
    if len(expl_files) == 0:
        fails.append("shape_exploration 清单未产出（attack agent 未执行参数族枚举）")
    if novel < min_novel:
        fails.append(f"novel_candidate 脚本 {novel} < 阈值 {min_novel}（泛化探索不足）")

    if fails:
        print("⛔ GATE FAIL — 打回 attack agent 重跑:")
        for f in fails:
            print(f"  - {f}")
        print("\n修复: attack agent 必须先产出 shape_exploration_{shape_id}.md 清单，")
        print("再对清单里 ✗（非 known_instance）参数生成 novel_candidate 脚本（标 exploration_target: novel_candidate）。")
        sys.exit(1)
    else:
        print(f"✓ GATE PASS — {len(expl_files)} 清单 + {novel} novel_candidate 脚本（≥{min_novel}）")
        sys.exit(0)


if __name__ == "__main__":
    main()
