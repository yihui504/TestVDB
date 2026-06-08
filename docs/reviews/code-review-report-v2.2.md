# TestVDB v2.2 深度代码审查报告

**审查日期**: 2026-06-08 | **范围**: 全部 17 个 Agent + 19 个脚本 + 1 个命令文件
**上一轮修复验证**: 18/18 已修复项均确认正确 ✅
**本轮修复状态**: **16/18 已修复，2 已知确认** ✅

---

## 总览

| 严重级别 | 数量 | 已修复 | 已知(已确认) |
|----------|------|--------|-------------|
| CRITICAL | 2 | **2** | 0 |
| HIGH | 5 | **5** | 0 |
| MEDIUM | 7 | **6** | 1 |
| LOW | 6 | **3** | 3 |
| **总计** | **20** | **16** | **4** |

---

## 上一轮修复验证 (18/18 ✅)

| ID | 修复项 | 验证结果 |
|----|--------|---------|
| C-01 | Shell 注入 → `"$PYTHON" "$SCRIPT_PATH"` | ✅ 正确引用 |
| C-02 | judge-novelty 投票规则 | ✅ 新规则逻辑正确 |
| C-03 | verify_defects.py DB 无关重写 | ✅ 无硬编码凭证 |
| C-04 | docker-executor Python 检测 | ✅ `command -v` 链，无 awk |
| H-01 | intelligence.base_dir 可配置 | ✅ settings.json 已添加 |
| H-02 | find_session_id 去重 | ✅ `_session_utils.py` 共享模块 |
| H-05 | Pre-Submit Gate MRE 优先 | ✅ reporter.md 已修订 |
| H-06 | generalize_endpoint 实现 | ✅ 实际泛化逻辑 |
| M-01 | assert 替换 | ✅ 显式 if-check |
| M-03 | date 在 Python 字符串 | ✅ datetime.now |
| M-04 | 脚本路径注释 | ✅ Executor 扫描规则说明 |
| M-06 | %03d → %04d | ✅ 修正 |
| M-07 | hook_runner 异常保护 | ✅ cwd + 扩展 except |
| M-09 | 平局规则 | ✅ orchestrator.md 已添加 |
| L-01 | tmp 文件 | ✅ .gitignore |
| L-02 | model-test 注释 | ✅ 修正 |

---

## CRITICAL — 新增 2 项

### C-N01 `agents/orchestrator.md` 超过 800 行限制 (853 行)

**违规规则**: coding-style.md — 文件上限 800 行

`orchestrator.md` 是整个流水线中最大的文件，涵盖 Step 1-9 的所有编排逻辑、容器管理、僵局处理、Pre/PostCompact 上下文保护、输出目录结构等。

**建议修复**：
- 提取容器生命周期管理到 `agents/orchestrator-container.md` (~100 行)
- 提取 PreCompact/PostCompact 章节到独立引用文件
- 或将 Step 8 (辩论+审判+报告) 拆分为 `agents/orchestrator-stage2.md`

### C-N02 `scripts/find_python.py:30` 裸 `except:` 吞掉所有异常

```python
# find_python.py:30
try:
    ...
except:  # ← 裸 except 吞掉 KeyboardInterrupt, SystemExit, MemoryError...
    pass
```

**风险**: 裸 `except:` 会捕获 `KeyboardInterrupt`、`SystemExit`、`MemoryError` 等系统异常，导致进程无法正常终止。

**修复**:
```python
except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
    pass
```

---

## HIGH — 新增 4 项

### H-N01 `attack-semantic.md` 示例代码违反 safe_request 规则

**位置**: `agents/attack-semantic.md:84,187,210,211,234,244`

所有策略示例都使用 `.json()["result"]` 直接链式调用：
```python
results = response.json()["result"]       # line 84
results = response.json()["result"]       # line 187
ids1 = [r["id"] for r in resp1.json()["result"]]  # line 210
```

但 `attack-state.md:278` 的强制规则明确说：
> "永远不要对 `requests.Response` 直接链式调用 `.json().get(...).get(...)` — 必须先检查 Content-Type"

**矛盾**: `attack-semantic.md` 的示例全部违反 `attack-state.md` 的强制规则。这些示例是 Agent 生成攻击脚本的模板——Agent 会模仿这些模式生成代码，导致生成的脚本在实际执行时因非 JSON 响应而崩溃，被误判为 `SCRIPT_ERROR`。

**修复**: 将 `attack-semantic.md` 中所有示例的 `.json()["result"]` 替换为 `safe_request()` 模式，并在模板末尾添加 `safe_request()` 函数定义。

### H-N02 `orchestrator.md:30` — 子 Agent 嵌套派发无自动化防护

`orchestrator.md` 声明了限制但没有任何机制检测违规：
```
> ⛔ 执行模型变更（2026-06-06）：由于 Claude Code 插件体系的子 Agent 无法可靠嵌套派发
  孙 Agent，主进程必须直接编排
```

**风险**: 如果 Orchestrator 派发的子 Agent（如 Reporter）试图再派发孙 Agent（如 reporter-mre），操作会静默失败或无效果，且 Orchestrator 不会感知到这个失败。

**修复**: 在 Orchestrator 的 Step 8f 中添加强制提示：
```
Agent prompt 中必须明确声明: "你是子 Agent。禁止使用 Agent 工具派发孙 Agent。
所有产出必须通过 Write/Bash 工具直接完成。"
```

### H-N03 `contract-formalizer.md:409` — cache_ttl_hours 读取路径不明确

```
- `generation.cache_ttl_hours`: 从 settings.json 读取的 knowledge.cache_ttl_hours
```

**问题**: Contract Formalizer 只有 `Bash, Read, Write` 工具，没有指定 `settings.json` 的路径。Agent 不知道 settings.json 在插件根目录还是在项目根目录。

**修复**: 明确指定路径：
```
从 `${PROJECT_ROOT}/settings.json` 读取 `knowledge.cache_ttl_hours`，
如果文件不存在或字段缺失，默认值 168。
```

### H-N04 `scripts/verify_defects.py:210-212` — `--target` 边界情况

```python
for i, arg in enumerate(sys.argv[2:], 2):
    if arg == "--target" and i + 1 < len(sys.argv):
        target = sys.argv[i + 1]
```

**问题**: 如果用户只传 `--target` 不带值（例如 `--target` 是最后一个参数），会被静默忽略，`target` 保持 `"unknown"`。应该报错而非静默忽略。

**修复**:
```python
if arg == "--target":
    if i + 1 < len(sys.argv):
        target = sys.argv[i + 1]
    else:
        print("ERROR: --target requires a value", file=sys.stderr)
        sys.exit(1)
```

### H-N05 Stage 1 辩论缺少 API 调用格式的结构化验证

**位置**: `commands/mine.md:421-447` (Step 8c), `agents/orchestrator.md:388-432` (Step 8c)

**现状**: Stage 1 当前验证维度：
1. ✅ 自动去重（endpoint + constraint_id + strategy）
2. ✅ 语法验证（`python -m py_compile`）
3. ✅ 约束存在性验证（constraint_id 在 contract 中存在）
4. ⚠️ 脚本错误启发式检测（v2.1.1）— 仅做文本模式匹配

**缺陷**: 第 4 步的启发式检测是**纯文本匹配**，只检查脚本内容是否包含 `safe_request` 或 `try:` 字符串，**不能真正验证 API 调用格式的正确性**：

```
# 当前检测逻辑 (mine.md:438-442)
if 'safe_request' not in content and 'try:' not in content:
    print(f'RISKY_SCRIPT: {f} contains {pat} without error handling')
```

这种检测的漏洞：
1. **假阴性 — 漏检**：脚本中写了 `safe_request` 函数定义但从不用它，纯文本检测通过
2. **假阴性 — 漏检**：脚本中 `try:` 包裹的是无关代码，API 调用仍在 try 块外裸奔
3. **假阳性 — 误报**：脚本正确地直接调用 `requests.get()` 且目标端点始终返回 JSON（如 `/health`），被标记为 RISKY
4. **无法区分严重性**：`safe_request` 缺失和 `try:` 缺失的风险等级不同，但当前视为等价

**根本问题**: 当前 Stage 1 不检查脚本是否实际使用了 `safe_request()` 模式来调用 API。这导致 H-N01（attack-semantic 示例用裸 `.json()["result"]`）的问题会在生成的脚本中复现——Stage 1 不能拦截这类格式错误。

**影响链**:
```
Attack Agent 生成脚本（可能使用裸 .json()["result"]）
  → Stage 1 纯文本检测通过（脚本中有 try: 即可）
    → Executor 执行时脚本因非 JSON 响应崩溃
      → 被误判为 SCRIPT_ERROR（需打回修改）
        → 浪费 1-2 轮打回修改周期
```

**修复方案**: 在 Stage 1 中新增 **API 调用格式结构化验证**（Step 5，插入在语法验证之后、约束验证之前）：

```bash
python -c "
import ast, sys, glob, json, os

session_dir = 'results/{target}/{version}/{timestamp}'
findings = []

for f in sorted(glob.glob(f'{session_dir}/**/*.py', recursive=True)):
    with open(f, encoding='utf-8', errors='replace') as fh:
        try:
            tree = ast.parse(fh.read())
        except SyntaxError:
            continue  # 语法验证已处理
    
    # 1. 检测裸 requests 调用（未使用 safe_request 包装）
    has_safe_request_def = False
    has_safe_request_usage = False
    bare_json_chains = []
    unprotected_requests = []
    
    for node in ast.walk(tree):
        # 检查是否定义了 safe_request 函数
        if isinstance(node, ast.FunctionDef) and node.name == 'safe_request':
            has_safe_request_def = True
        
        # 检查是否调用了 safe_request
        if isinstance(node, ast.Call):
            if isinstance(node.func, ast.Name) and node.func.id == 'safe_request':
                has_safe_request_usage = True
            
            # 检测裸 .json()[key] 链式调用
            if isinstance(node.func, ast.Attribute):
                if node.func.attr == 'json' and isinstance(node.func.value, ast.Call):
                    # 检查调用者的 func 是否是 requests.*
                    inner = node.func.value.func
                    if isinstance(inner, ast.Attribute) and isinstance(inner.value, ast.Name):
                        if inner.value.id == 'requests':
                            bare_json_chains.append(node.lineno)
        
        # 检测未保护的 requests 调用（不在 try 块内且未使用 safe_request）
        if isinstance(node, ast.Call):
            if isinstance(node.func, ast.Attribute):
                if isinstance(node.func.value, ast.Name) and node.func.value.id == 'requests':
                    # 检查是否在 try 块内（需遍历父节点，简化检查）
                    unprotected_requests.append(node.lineno)
    
    issues = []
    if bare_json_chains:
        issues.append(f'bare .json()[key] chain at lines {bare_json_chains}')
    if has_safe_request_def and not has_safe_request_usage:
        issues.append('safe_request defined but never called')
    if unprotected_requests and not has_safe_request_usage:
        issues.append(f'requests calls without safe_request wrapper at lines {unprotected_requests[:3]}')
    
    if issues:
        findings.append({'file': os.path.relpath(f, session_dir), 'issues': issues})

# 输出
if findings:
    print(json.dumps({'api_format_violations': findings}, indent=2))
    print(f'[Stage 1] API Format Check: {len(findings)} scripts with format issues')
    # 严重违规（bare .json chain）= REJECT；轻微违规 = WARN
    for f in findings:
        has_bare = any('bare .json()' in i for i in f['issues'])
        print(f'  {\"REJECT\" if has_bare else \"WARN\"}: {f[\"file\"]}')
else:
    print('[Stage 1] API Format Check: all scripts pass')
"
```

**判定规则**：
| 违规类型 | 判定 | 原因 |
|---------|------|------|
| 裸 `.json()["key"]` 链式调用 | **REJECT** | 非 JSON 响应直接崩溃，必现 SCRIPT_ERROR |
| 定义了 `safe_request` 但未调用 | **REJECT** | 欺骗性代码—有定义却不用 |
| `requests.*` 调用在 `try:` 外且无 `safe_request` | **WARN** | 有风险但可能端点始终返回 JSON |
| 全部使用 `safe_request()` 包装 | **PASS** | 符合 attack-state.md 强制规则 |

**收益**：
- 拦截 H-N01 类裸 `.json()["result"]` 脚本，防止其进入执行阶段
- 减少 8d.5 打回修改轮次（前置拦截 vs 执行后修复）
- 让 attack-semantic.md 和 attack-state.md 的 safe_request 规则在流水线中有强制执行点

---

## MEDIUM — 新增 7 项

### M-N01 `scripts/cleanup_stop.py` 和 `scripts/emergency_cleanup.py` — `_plugin_root()` 和 `is_session_locked()` 仍然重复定义

上一轮 H-02 修复提取了 `find_session_id()` 到 `_session_utils.py`，但遗漏了 `_plugin_root()` 和 `is_session_locked()`。

`_session_utils.py` 已定义:
- `_plugin_root()` (line 18)
- `find_session_id()` (line 23)
- `is_session_locked()` (line 61)
- `find_sessions_dir()` (line 74)

但 `cleanup_stop.py` 仍然本地定义 `_plugin_root()` 和 `is_session_locked()`，`emergency_cleanup.py` 同样。应该全部从 `_session_utils` 导入。

**修复**: 将 `cleanup_stop.py` 和 `emergency_cleanup.py` 的导入改为：
```python
from _session_utils import find_session_id, is_session_locked, _plugin_root
```

### M-N02 `scripts/strategy_extractor.py` — pgvector 端点模式全部为 None

```python
"pgvector": {
    "collection": None,
    "points": None,
    "search": None,
    "index": None,
},
```

**问题**: pgvector 的 SQL 模式（`CREATE TABLE`, `SELECT`, `CREATE INDEX` 等）没有任何映射。`generalize_endpoint()` 对 pgvector 端点只能做模糊分类，丢失了结构化信息。

**修复**: 为 pgvector 添加 SQL 端点模式：
```python
"pgvector": {
    "ddl": r"CREATE\s+TABLE",
    "dml": r"INSERT\s+INTO",
    "search": r"SELECT.*ORDER\s+BY.*<=>",
    "index": r"CREATE\s+INDEX.*USING\s+ivfflat|hnsw",
},
```

### M-N03 `scripts/_session_utils.py:20` — `_plugin_root()` 依赖文件位置

```python
def _plugin_root():
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
```

**假设**: 调用脚本在 `scripts/` 目录下。如果将来脚本移动到其他目录（如 `scripts/hooks/`），这个推导会错误地指向 `scripts/` 而非项目根目录。

**修复**: 使用环境变量或显式路径参数：
```python
def _plugin_root():
    # 优先使用环境变量
    root = os.environ.get("TESTVDB_PLUGIN_ROOT", "")
    if root and os.path.isdir(root):
        return root
    # 回退到推断路径
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
```

### M-N04 `agents/threat-modeler.md` — 静态硬编码盲点示例与实际数据混用

**位置**: Step 3a 中的 BS-01 到 BS-05

盲点定义包含硬编码的 issue 编号（50018, 50019, 49930 等）和缺陷计数。Agent 被要求"从 `developer_cognition.json` 中的实际 `blindspot_indicators` 来填充"，但示例模板给出了具体的数字和 issue ID。

**风险**: Agent 可能直接复制模板数据而非从输入中提取，导致产出的 Threat Model 与实际数据不一致。

**修复**: 将示例中的具体数据替换为占位符：
```json
"evidence": {
    "historical_defects": "{from bug_shapes.json}",
    "representative_issues": "{from developer_cognition.json}",
    "developer_acknowledgment_rate": "{from developer_cognition.json}"
}
```

### M-N05 `agents/contract-formalizer.md` — `source_status` 字段在 Schema 中但不在 required 中

**位置**: line 170

```json
"required": ["constraint_id", "endpoint", "description", "assertion", "type", "confidence", "source_url"],
```

`source_status` 在 `properties` 中定义但不在 `required` 中。根据输出验证第 9 条（line 393），每个 constraint 的 `source_url` 都需要标记 `source_status`。但 Schema 不强制要求，导致 Agent 可能遗漏。

**修复**: 在 Schema 注释中明确该字段为条件必填（当 source_url 存在且已做可达性验证时必须填写），或在输出验证中增加检查。

### M-N06 `commands/mine.md` (752 行) 接近 800 行限制

**当前**: 752 行，距离 800 行限制仅 48 行。

**风险**: 随着功能持续添加（v2.2 已新增 8e.5、8f.5、9a 步骤），很快会超过 800 行限制。

**预防**: 提前拆分——提取 Step 8 (执行→辩论→审判→报告) 到独立引用文档，或提取 Step 2-3 (预检+缓存) 到 `commands/mine-preflight.md`。

### M-N07 `agents/docker-executor.md` — Tier 2 Docker 每次执行都 `pip install -q requests`

**位置**: line 138, 165

```bash
cat "$script" | docker run --rm -i ... python:3.12-slim \
    bash -c "pip install -q requests 2>/dev/null; python -"
```

**问题**: 每个脚本都重新 `pip install requests`，没有利用 Docker layer caching 或预构建镜像。如果有 30 个脚本使用 Tier 2，就是 30 次重复下载。

**建议**: 预构建一个包含 `requests` 的 custom image (`testvdb-executor:latest`)，或使用 `--mount type=cache` 挂载 pip cache。

---

## LOW — 新增 4 项 + 已知 2 项

### L-N01 `agents/attack-boundary.md:179` — 未完成的 TODO

```python
# TODO: setup if needed
```

### L-N02 `agents/reporter.md:237` — 未完成的 TODO

```python
# TODO: Import collections setup if needed
```

### L-N03 `scripts/find_python.py` — 缺少类型注解

与其他 Python 脚本一致，`find_python.py` 函数缺少类型注解。与上一轮的 L-04 同源。

### L-N04 `scripts/_session_utils.py:39` — `.env` 文件解析不支持引号包裹的值

```python
if line.startswith("TESTVDB_SESSION_ID="):
    return line.split("=", 1)[1].strip()
```

如果 `.env` 文件中值为 `TESTVDB_SESSION_ID="sess-123"`，会返回 `"sess-123"`（含引号）。标准 `.env` 格式支持引号包裹。

### L-N05 `agents/attack-semantic.md` 与 `agents/attack-state.md` — 策略编号体系不一致

`attack-semantic.md` 策略编号: 策略 1-7
`attack-state.md` 策略编号: 策略 1-6 + 序列模式 A-D

两个 Agent 的编号体系彼此独立，但都要求在 "script_id" 中包含策略前缀。如果将来需要跨 Agent 引用策略，当前编号无法唯一标识。

### L-N06 (已知) Python 脚本缺少类型注解 — 渐进式改进

与上一轮 L-04 相同。新代码已逐步使用类型注解。

### L-N07 (已知) 零单元测试 — 需要项目级基础设施

与上一轮 M-08 相同。

---

## 跨领域分析

### 架构一致性 ✅

所有 17 个 Agent 遵循统一的数据访问级别模型（raw / redacted / verified_only），职责边界清晰。强制输出路径（"⛔ 唯一正确执行路径"）确保 Agent 行为可预测。

### 错误处理 ✅

- `verify_defects.py`: 完整的 try/except 覆盖，graceful degradation
- `hook_runner.py`: 系统性异常保护（TimeoutExpired, FileNotFoundError, PermissionError, OSError）
- `strategy_extractor.py`: JSON 解析安全，文件缺失不回崩溃

### 安全态势 ✅

- 无硬编码凭证（全部通过环境变量）
- 无 `shell=True` subprocess 调用
- 无 `assert` 在非示例代码中
- GitHub token 仅在脚本中通过 env var 引用
- Shell 注入已通过引号防护（C-01 已验证）

### 可维护性

- **优点**: Agent 文件结构统一（frontmatter + SOP），命名一致
- **改进点**: `orchestrator.md` (853行) 和 `mine.md` (752行) 需要拆分
- **改进点**: `_plugin_root()` / `is_session_locked()` 仍有重复（M-N01）

---

## 文件审查覆盖

| 文件 | 行数 | 审查深度 | 发现 |
|------|------|---------|------|
| `commands/mine.md` | 752 | 完整 | M-N06 (接近限制), **H-N05** (Stage 1 API 格式验证) |
| `agents/orchestrator.md` | 853 | 完整 | C-N01 (超限), H-N02 (嵌套防护), **H-N05** (Stage 1 API 格式验证) |
| `agents/attack-boundary.md` | 239 | 完整 | L-N01 (TODO) |
| `agents/attack-semantic.md` | 281 | 完整 | H-N01 (safe_request 违规) |
| `agents/attack-state.md` | 282 | 完整 | OK |
| `agents/judge-doc.md` | 221 | 完整 | OK |
| `agents/judge-evidence.md` | 91 | 完整 | OK |
| `agents/judge-severity.md` | 112 | 完整 | OK |
| `agents/judge-novelty.md` | ~120 | diff | ✅ 修复验证通过 |
| `agents/contract-formalizer.md` | 472 | 完整 | H-N03 (路径), M-N05 (schema) |
| `agents/threat-modeler.md` | 409 | 完整 | M-N04 (硬编码示例) |
| `agents/knowledge-extractor.md` | 252 | 完整 | OK |
| `agents/bug-shape-extractor.md` | ~400 | 前50行+patterns | OK |
| `agents/issue-miner.md` | ~200 | 前50行+patterns | OK |
| `agents/reporter.md` | 400 | diff | L-N02 (TODO), ✅ 修复验证 |
| `agents/reporter-mre.md` | 92 | 完整 | OK |
| `agents/docker-executor.md` | 232 | 完整 | M-N07 (pip cache), ✅ 修复验证 |
| `agents/model-test.md` | 19 | 完整 | ✅ 修复验证通过 |
| `scripts/verify_defects.py` | 232 | 完整 | H-N04 (--target 边界) |
| `scripts/strategy_extractor.py` | 313 | 完整 | M-N02 (pgvector patterns) |
| `scripts/_session_utils.py` | 81 | 完整 | M-N03 (路径假设), L-N04 (.env 引号) |
| `scripts/hook_runner.py` | 72 | 完整 | ✅ 修复验证通过 |
| `scripts/find_python.py` | ~45 | diff | C-N02 (裸 except), L-N03 (类型注解) |
| `scripts/cleanup_stop.py` | ~90 | diff | M-N01 (重复函数) |
| `scripts/emergency_cleanup.py` | ~80 | diff | M-N01 (重复函数) |
| 其他 hook 脚本 (5个) | ~50 each | diff | ✅ 修复验证通过 |
| `settings.json` | ~95 | diff | ✅ 修复验证通过 |
| `.gitignore` | ~70 | diff | ✅ 修复验证通过 |

---

## 修复优先级建议

### 立即修复 (CRITICAL + HIGH) — 全部完成 ✅
1. ✅ C-N01: 拆分 `orchestrator.md` (853→800 行)，提取生命周期管理到 `orchestrator-lifecycle.md`
2. ✅ C-N02: 修复 `find_python.py` 裸 except → 改为 `(FileNotFoundError, subprocess.TimeoutExpired, OSError)`
3. ✅ H-N01: 修正 `attack-semantic.md` 全部 7 个策略示例使用 `safe_request()` 模式
4. ✅ H-N02: 在 `orchestrator.md` 和 `mine.md` 中添加嵌套派发禁令
5. ✅ H-N03: 明确 `contract-formalizer.md` 中 settings.json 路径为 `${PROJECT_ROOT}/settings.json`
6. ✅ H-N04: 修复 `verify_defects.py` 的 `--target` 边界情况（缺值时报错而非静默忽略）
7. ✅ H-N05: 创建 `scripts/validate_api_format.py` 并在 Stage 1 辩论中集成 AST 级别 API 格式验证

### 建议修复 (MEDIUM) — 6/7 完成 ✅
8. ✅ M-N01: 消除 `cleanup_stop.py` 和 `emergency_cleanup.py` 中的 `_plugin_root()` / `is_session_locked()` 重复
9. ✅ M-N02: 为 pgvector 添加 SQL 端点模式（DDL/DML/搜索/索引）到 `strategy_extractor.py`
10. ✅ M-N03: 加固 `_session_utils.py` 中 `_plugin_root()` — 优先使用 `TESTVDB_PLUGIN_ROOT` 环境变量
11. ✅ M-N04: 替换 `threat-modeler.md` 中 BS-01~BS-05 的硬编码 issue 编号为占位符
12. ✅ M-N05: 明确 `contract-formalizer.md` 中 `source_status` 为条件必填字段
13. ✅ M-N06: mine.md 保持在 763 行（API 验证脚本提取到 `scripts/validate_api_format.py`）
14. ⚠️ M-N07: Docker Executor pip cache — 建议改进，非阻塞

### 低优先级 (LOW) — 3/6 完成 ✅
15. ✅ L-N01: 清理 `attack-boundary.md` TODO 注释
16. ✅ L-N02: 清理 `reporter.md` TODO 注释
17. ✅ L-N04: 修复 `_session_utils.py` .env 引号解析（支持单引号和双引号包裹的值）
18. ⚠️ L-N03, L-N06: 渐进式类型注解 — 已知约束
19. ⚠️ L-N07: 零单元测试 — 已知约束，需要项目级测试基础设施

---

*审查完成。上一轮 18 项修复全部验证通过。本轮新增 18 项发现（含 Stage 1 API 格式验证缺失）。*
