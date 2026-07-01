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

from _pipeline_utils import read_json

DONE_PHASES = {"CLEANUP", "DONE", None}
RESUMABLE_TURN_TYPES = {"loop", "setup"}


def _plugin_root() -> str:
    root = os.environ.get("TESTVDB_PLUGIN_ROOT", "")
    if root and os.path.isdir(root):  # env 显式指定即信；mine.md 校验仅用于 fallback 推断防漂移
        return root
    # Delegate to canonical _pipeline_utils.plugin_root() (ADR-0007)
    from _pipeline_utils import plugin_root
    result = plugin_root()
    if result is not None:
        return str(result)
    return ""



def _resume_target_path(root: str) -> str:
    return os.path.join(root, "results", ".resume_target")


def read_resume_target(root: str):
    """读 .resume_target 标记（resume 命令设）。返回 session_dir 或 None。"""
    data = read_json(_resume_target_path(root))
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
        ps = read_json(p)
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
    """列出所有未完成 session（phase∉DONE），供提示/resume 列选。只扫 timestamp 级。"""
    out = []
    for p in glob.glob(os.path.join(root, "results", "**", "pipeline_state.json"), recursive=True):
        rel = os.path.relpath(p, root)
        if rel.count(os.sep) < 4:  # 跳过 version 根目录残留，只认 timestamp 级（与 scan_resumable 一致）
            continue
        ps = read_json(p)
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


def find_by_session_id(root: str, session_id: str) -> str | None:
    """按 session_id 定位 session_dir（只扫 timestamp 级，resume 命令用，避免重复 glob）。"""
    for p in glob.glob(os.path.join(root, "results", "**", "pipeline_state.json"), recursive=True):
        rel = os.path.relpath(p, root)
        if rel.count(os.sep) < 4:
            continue
        ps = read_json(p)
        if ps and ps.get("session_id") == session_id:
            return os.path.dirname(p)
    return None


def dispatch(target: str, version: str, force_new: bool = False) -> dict:
    """主入口判断。

    返回 {decision: FRESH_START|RESUME, session_dir?, phase?, target?, version?, reason, incomplete}
    - force_new=True: 强制新建（--new），仍列出未完成供知情
    - incomplete 字段：指定 target/version → 同 target 未完成（精确提示）；未指定（Loop Turn 扫描）→ 所有未完成
    """
    root = _plugin_root()
    if not root:
        return {"decision": "FRESH_START", "reason": "no plugin root", "incomplete": []}

    incomplete = find_incomplete(root, target, version)
    same_target_incomplete = [i for i in incomplete if i["target"] == target and i["version"] == version]
    incomplete_field = same_target_incomplete if (target or version) else incomplete

    if force_new:
        consume_resume_target(root)  # --new 明确新建，清残留 resume 标记防下次误 RESUME
        return {
            "decision": "FRESH_START", "reason": "force_new (--new)",
            "incomplete": incomplete_field,
        }

    # 1. .resume_target 标记优先（resume 命令设，精确续指定）
    rt = read_resume_target(root)
    if rt:
        consume_resume_target(root)
        ps = read_json(os.path.join(rt, "pipeline_state.json")) or {}
        return {
            "decision": "RESUME", "session_dir": rt,
            "phase": ps.get("phase", "ROUND_START"),
            "target": ps.get("target", ""),
            "version": ps.get("version_target", ""),
            "reason": f"resume_target 标记 → {rt}",
            "incomplete": incomplete_field,
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
            "incomplete": incomplete_field,
        }

    return {"decision": "FRESH_START", "reason": "无可恢复中断", "incomplete": incomplete_field}


if __name__ == "__main__":
    # ponytail: demo self-check — 无参时打印当前 dispatch 结果（真实 results/）
    import sys
    t = sys.argv[1] if len(sys.argv) > 1 else "weaviate"
    v = sys.argv[2] if len(sys.argv) > 2 else "v1.38.0"
    print(json.dumps(dispatch(t, v), ensure_ascii=False, indent=2))
