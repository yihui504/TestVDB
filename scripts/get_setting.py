#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""dot-path 读取 settings.json 的机械查询工具（ADR-0009 §6）。

用法：python scripts/get_setting.py <dot.path>
输出：stdout 一行 JSON 编码的值（缺失路径输出 null，exit 0）；
     参数缺失 exit 2；文件损坏 exit 1。
设计目的：orchestrator/主进程经 Bash 确定性读取配置开关
（如 mining.confirm_per_round），替代 LLM 目测 settings.json。
"""
import json
import os
import sys

_PLUGIN_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_SETTINGS_PATH = os.path.join(_PLUGIN_ROOT, "settings.json")


def get_setting(dot_path: str):
    with open(_SETTINGS_PATH, encoding="utf-8") as f:
        node = json.load(f)
    for key in dot_path.split("."):
        if not isinstance(node, dict) or key not in node:
            return None
        node = node[key]
    return node


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: get_setting.py <dot.path>", file=sys.stderr)
        return 2
    try:
        value = get_setting(sys.argv[1])
    except (OSError, json.JSONDecodeError) as exc:
        print(f"error: cannot read settings.json: {exc}", file=sys.stderr)
        return 1
    print(json.dumps(value))
    return 0


if __name__ == "__main__":
    sys.exit(main())
