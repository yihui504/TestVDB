# 批次 C · 小修收尾（find_python + schema + 端点完整度）设计

- **日期**: 2026-06-14
- **批次**: C（剩余技术债小修）
- **分支**: `feat/batch-c-cleanup`
- **原则**: 不动原有记录，全 commit 可回退

---

## 范围

3 个问题（mine 暴露 + review 遗留）：
- **[1] find_python.py 硬编码用户路径**（review L-N10）
- **[2] settings.json 无 schema 强制**（review L-N12）
- **[5] 端点提取完整度**（bug #3 端到端暴露：旧契约漏 admin 运维端点）

**排除**：
- [3] docker pip 缓存——grep 确认 docker-executor.md 已无 "pip install"，问题不存在
- [4] orchestrator.md 834 行——交给**批次 D（命令解耦）**顺带解决（架构对了行数自然下来）

---

## [1] find_python.py 动态化

**现状**（`scripts/find_python.py:17-19`）：
```python
r"C:\Users\11428\AppData\Local\Programs\Python\Python311\python.exe",
r"C:\Users\11428\AppData\Local\Programs\Python\Python310\python.exe",
r"C:\Users\11428\AppData\Local\Programs\Python\Python39\python.exe",
```
硬编码用户路径，其他机器不可用。

**修复**：动态构建候选路径，遍历常见版本：
```python
localappdata = os.environ.get("LOCALAPPDATA", "")
home = os.path.expanduser("~")
candidates = []
# Windows: %LOCALAPPDATA%\Programs\Python\PythonXY\python.exe（遍历 3.9-3.13）
if localappdata:
    for xy in ("313", "312", "311", "310", "39"):
        candidates.append(os.path.join(localappdata, "Programs", "Python", f"Python{xy}", "python.exe"))
# 通用兜底
candidates += [
    os.path.join(home, ".pyenv", "shims", "python"),  # pyenv
    "python", "python3", "py",  # PATH
]
```
保留现有 `command -v` / `py.exe` 检测逻辑作兜底。

**测试**：`tests/test_find_python.py`——动态路径构建（mock LOCALAPPDATA）+ 不含硬编码 `C:\Users\11428`。

---

## [2] settings.json schema 强制

**现状**：`contracts/settings_schema.json` 存在，但 `settings.json` 无 `$schema` 引用、`preflight.py` 不验证——配置错误静默失败。

**修复**：`scripts/preflight.py` 加 schema 验证步骤：
- 优先用 `jsonschema` 库（如装了）跑完整 schema 验证
- 否则轻量自检：读 `settings_schema.json` 的 required 顶层 keys，对照 `settings.json` 检查缺失
- 验证失败 → preflight 报错（非致命 warning 或致命 error，取决于严重度）

**测试**：`tests/test_preflight_schema.py`——合法 settings 通过、缺 required key 报错。

---

## [5] 端点提取完整度（引导 + 检测）

**根因**（bug #3 端到端诊断）：raw_knowledge **含** admin 运维端点（well-known/cluster/nodes/modules/backup/shards/tenants 各 6-25 次），但旧契约漏提取（53 端点）。说明 **contract-formalizer 提取不全**（LLM 读 raw_knowledge 时漏运维端点），非 knowledge-extractor 漏爬。新契约（bug #3 后）提取了 83——说明提取有波动，缺完整性约束。

**修复 ① 引导**（`agents/contract-formalizer.md` 规则 1）：明确"提取**所有**端点，**含运维/管理类**（health/ready/cluster/nodes/modules/backup/shards/tenants/well-known），按通用 category（admin）分类，勿漏。每个文档提及的 HTTP 端点都应进入 api_endpoints。"

**修复 ② 检测**（`scripts/validate_contract.py`）：加端点完整度检测——
- 从 raw_knowledge（version 目录）提取 HTTP 路径引用数（`/v1/...`、`POST /...` 等模式）
- 契约端点数 vs raw_knowledge 路径引用数，差异 > 阈值（如契约 < 引用数 × 0.5）→ 警告"端点提取可能不全"
- 这是**启发式**（raw_knowledge 非结构化，路径引用数是粗略上界），警告非 error

**测试**：`tests/test_validate_contract.py` 加完整度检测 case（契约端点少 vs raw_knowledge 多 → 警告）。

---

## 验收

1. `find_python.py` 无硬编码 `C:\Users\11428`（grep 验证 + 动态构建测试）
2. `preflight.py` 验证 settings schema（缺失 key 报错）
3. `contract-formalizer.md` 规则 1 含运维端点提取引导；`validate_contract.py` 含完整度检测
4. `pytest` 全过（含新测试）
5. 不动 results/contracts/main 历史

---

## 不做（YAGNI / 交批次 D）

- orchestrator.md 拆分 → 批次 D（命令解耦：intel/contract/mine 三命令）
- 端点完整度强行修 LLM 生成 → 只引导 + 检测（像 bug #3）
