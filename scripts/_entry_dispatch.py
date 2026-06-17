#!/usr/bin/env python3
"""TestVDB mine 入口判断 — 决定 FRESH_START 还是 RESUME。

从 commands/mine.md 抽出以便测试。修复历史 bug:
  ① 只认 turn_type=loop → 现也认 setup（Turn1 setup turn 中断）
  ② 扫描不按 target/version 过滤 → 现按 /mine 参数过滤（防续错 target）
  ③ 无精确续指定入口 → .resume_target 标记优先（resume 命令设）
  ④ version 根目录残留 state → scan_resumable 只扫 timestamp 级（depth≥4）
"""
from __future__ import annotations
import glob, json, os

DONE_PHASES = {"CLEANUP", "DONE", None}
RESUMABLE_TURN_TYPES = {"loop", "setup"}


def _plugin_root() -> str:
    root = os.environ.get("TESTVDB_PLUGIN_ROOT", "")
    if root and os.path.isdir(root):  # env 显式指定即信；mine.md 校验仅用于 fallback 推断防漂移
        return root
    cur = os.getcwd()
    for _ in range(7):
        if os.path.isfile(os.path.join(cur, "commands", "mine.md")):
            return cur
        parent = os.path.dirname(cur)
        if parent == cur:
            break
        cur = parent
    return ""


def _read_json(path: str):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (json.JSONDecodeError, OSError):
        return None


def _resume_target_path(root: str) -> str:
    return os.path.join(root, "results", ".resume_target")


def read_resume_target(root: str):
    """读 .resume_target 标记（resume 命令设）。返回 session_dir 或 None。"""
    data = _read_json(_resume_target_path(root))
    if not data or not data.get("session_dir"):
        return None
    sd = data["session_dir"]
    return sd if os.path.isdir(sd) else None


def consume_resume_target(root: str) -> None:
    """RESUME 后删标记（一次性）。"""
    try:
        os.remove(_resume_target_path(root))
    except OSError:
        pass


def write_resume_target(root: str, session_dir: str, target: str, version: str) -> None:
    """resume 命令调用：写下次要 /mine 续的 session。"""
    os.makedirs(os.path.dirname(_resume_target_path(root)), exist_ok=True)
    with open(_resume_target_path(root), "w", encoding="utf-8") as f:
        json.dump({"session_dir": session_dir, "target": target, "version": version}, f)


def scan_resumable(root: str, target: str, version: str):
    """扫描 results/ 找匹配 target/version 的可恢复中断，按 mtime 降序。

    只扫 timestamp 级目录（results/target/version/timestamp/pipeline_state.json，
    depth=4），跳过 version 根目录残留（Bug ④）。
    """
    matches = []
    for p in glob.glob(os.path.join(root, "results", "**", "pipeline_state.json"), recursive=True):
        rel = os.path.relpath(p, root)
        if rel.count(os.sep) < 4:  # 跳过 version 根目录残留（3 层），只认 timestamp 级（4 层）
            continue
        ps = _read_json(p)
        if not ps:
            continue
        if target and ps.get("target") != target:
            continue
        if version and ps.get("version_target") != version:
            continue
        if ps.get("turn_type") not in RESUMABLE_TURN_TYPES:
            continue
        if ps.get("phase") in DONE_PHASES:
            continue
        try:
            mtime = os.path.getmtime(p)
        except OSError:
            continue
        matches.append((mtime, os.path.dirname(p), ps))
    matches.sort(key=lambda x: x[0], reverse=True)
    return matches


def find_incomplete(root: str, target: str | None = None, version: str | None = None):
    """列出所有未完成 session（phase∉DONE），供提示/resume 列选。"""
    out = []
    for p in glob.glob(os.path.join(root, "results", "**", "pipeline_state.json"), recursive=True):
        ps = _read_json(p)
        if not ps or ps.get("phase") in DONE_PHASES:
            continue
        if target and ps.get("target") != target:
            continue
        if version and ps.get("version_target") != version:
            continue
        out.append({
            "session_id": ps.get("session_id", "?"),
            "target": ps.get("target", "?"),
            "version": ps.get("version_target", "?"),
            "phase": ps.get("phase", "?"),
            "turn_type": ps.get("turn_type", "?"),
            "session_dir": os.path.dirname(p),
        })
    return out


def dispatch(target: str, version: str, force_new: bool = False) -> dict:
    """主入口判断。

    返回 {decision: FRESH_START|RESUME, session_dir?, phase?, reason, incomplete?}
    - force_new=True: 强制新建（--new），仍列出未完成供知情
    """
    root = _plugin_root()
    if not root:
        return {"decision": "FRESH_START", "reason": "no plugin root", "incomplete": []}

    incomplete = find_incomplete(root, target, version)
    same_target_incomplete = [i for i in incomplete if i["target"] == target and i["version"] == version]

    if force_new:
        return {
            "decision": "FRESH_START", "reason": "force_new (--new)",
            "incomplete": same_target_incomplete,
        }

    # 1. .resume_target 标记优先（resume 命令设，精确续指定）
    rt = read_resume_target(root)
    if rt:
        consume_resume_target(root)
        ps = _read_json(os.path.join(rt, "pipeline_state.json")) or {}
        return {
            "decision": "RESUME", "session_dir": rt,
            "phase": ps.get("phase", "ROUND_START"),
            "target": ps.get("target", ""),
            "version": ps.get("version_target", ""),
            "reason": f"resume_target 标记 → {rt}",
        }

    # 2. 扫描匹配 target/version 的中断（认 loop+setup，Bug ①②）
    matches = scan_resumable(root, target, version)
    if matches:
        sd, ps = matches[0][1], matches[0][2]
        return {
            "decision": "RESUME", "session_dir": sd,
            "phase": ps.get("phase", "ROUND_START"),
            "target": ps.get("target", ""),
            "version": ps.get("version_target", ""),
            "reason": f"扫描命中 {ps.get('turn_type')}/{ps.get('phase')}",
            "incomplete": same_target_incomplete,
        }

    return {"decision": "FRESH_START", "reason": "无可恢复中断", "incomplete": same_target_incomplete}


if __name__ == "__main__":
    # ponytail: demo self-check — 无参时打印当前 dispatch 结果（真实 results/）
    import sys
    t = sys.argv[1] if len(sys.argv) > 1 else "weaviate"
    v = sys.argv[2] if len(sys.argv) > 2 else "v1.38.0"
    print(json.dumps(dispatch(t, v), ensure_ascii=False, indent=2))
