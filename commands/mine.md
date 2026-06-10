---
description: 启动向量数据库自动化缺陷挖掘流水线
allowed-tools: Read, Write, Bash, Grep, Glob, Agent
---

# /testvdb:mine

启动向量数据库自动化缺陷挖掘流水线。

---

## ⚠️ 架构约束（CRITICAL — 技术原因）

**Claude Code 插件体系的技术限制：子 Agent 无法可靠地嵌套派发孙 Agent。**

这意味着：
- ✅ 主进程 → `testvdb:knowledge-extractor`（可以——主进程直接派发）
- ✅ 主进程 → `testvdb:orchestrator`（可以——但 orchestrator 内部派发孙 Agent 不可靠）
- ❌ orchestrator(子) → `testvdb:knowledge-extractor`(孙)（不可靠——agent_type 会被丢失为 "unknown"）

**因此本命令的设计：主进程直接担任编排者角色，按照 `agents/orchestrator.md` 的 SOP 逐步派发子 Agent。**
`testvdb:orchestrator` 的 agent 定义保留为 SOP 参考文档。

---

## ⛔ 核心铁律

**主进程永远只做编排，不做执行。违反任何一条流水线立即故障。**

| 禁止事项 | 正确做法 |
|---------|---------|
| ❌ 使用 WebSearch/WebFetch 爬取文档 | ✅ `Agent(subagent_type="testvdb:knowledge-extractor")` |
| ❌ 自己生成 structured_contract.json | ✅ `Agent(subagent_type="testvdb:contract-formalizer")` |
| ❌ 自己写 Python 攻击脚本 | ✅ `Agent(subagent_type="testvdb:attack-boundary/state/semantic")` |
| ❌ 自己运行 Python 脚本或 curl | ✅ `Agent(subagent_type="testvdb:docker-executor")` |
| ❌ 自己判断缺陷有效性 | ✅ `Agent(subagent_type="testvdb:judge-*")` |
| ❌ 自己生成缺陷报告 | ✅ `Agent(subagent_type="testvdb:reporter")` |

**主进程只使用这些工具做编排工作：** `Read`(读文件), `Write`(写状态文件), `Bash`(验证产出), `Grep`(搜索), `Glob`(匹配), `Agent`(派发子Agent)。

---

## Usage

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, 或 `pgvector` |
| `<version>` | Yes | — | 目标版本号 |
| `--max-rounds N` | No | `5` | 最大挖掘轮数。`0` = 无上限 |
| `--min-defects N` | No | `1` | 最低缺陷产出要求 |

---

## 执行流程（主进程 = 编排者）

> **📖 完整 SOP 参考**: `agents/orchestrator.md`（阶段详解、投票规则、错误处理）、`skills/pipeline/SKILL.md`（六阶段流水线规范）。本文件只保留编排调度命令，不重复 SOP 描述。

### Step 1: 解析参数
- 验证 `target` ∈ {milvus, qdrant, weaviate, pgvector}
- 解析 `version`, `max_rounds`, `min_defects`
- 确定 `PROJECT_ROOT`: `git rev-parse --show-toplevel 2>/dev/null || pwd`

### Step 2: 前置条件检查
自行检查 Docker/Python/磁盘/网络：
```bash
python scripts/preflight.py
docker compose -f docker/crawl4ai.yml up -d --wait 2>/dev/null || true
```

**v2.2 新增 — 自动压缩检查**：多轮流水线会产生大量上下文，需确保 Claude Code 的自动压缩已开启：

```bash
python -c "
import json, sys, os
settings_path = os.path.expanduser('~/.claude/settings.json')
try:
    with open(settings_path, encoding='utf-8') as f:
        s = json.load(f)
    if s.get('autoCompactEnabled'):
        print('[Preflight] autoCompactEnabled: OK')
    else:
        print('[Preflight] autoCompactEnabled: MISSING — 多轮流水线可能因上下文溢出而中断')
        print('[Preflight] 建议: 在 ~/.claude/settings.json 中设置 \"autoCompactEnabled\": true')
        sys.exit(0)  # 不阻断，仅警告
except FileNotFoundError:
    print('[Preflight] ~/.claude/settings.json 不存在，跳过 autoCompact 检查')
except json.JSONDecodeError:
    print('[Preflight] settings.json 格式错误，跳过 autoCompact 检查')
"
```

### Step 3: 缓存检查
检查 `results/{target}/{version}/structured_contract.json` 是否存在且未过期（TTL 见 settings.json 的 `knowledge.cache_ttl_hours`，默认 168h）。如果缓存有效 → 跳到 Step 6。

**v2.0 新增 — Passport Hash 验证（material_passport.enabled=true 时）：**
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```
- 退出码 0（PASS）→ 缓存有效，跳到 Step 6
- 退出码 1（NO_PASSPORT）→ 旧格式契约，输出警告但继续使用缓存
- 退出码 2（TAMPERED）→ 契约被篡改，强制重新生成（继续 Step 4）
- 退出码 3（INVALID_JSON/FILE_NOT_FOUND）→ 视为缓存无效

如果 `material_passport.enabled=false`，跳过 hash 验证，仅按 TTL 判断。

### Step 3.5: 跨会话策略注入准备（v2.0 新增，evolution.enabled=true 时）

读取 Strategy Registry 中适用于当前 target 的策略：
```bash
python scripts/strategy_injector.py {target} --text-only
```

将输出文本保存为临时变量 `cross_session_strategies`，供 Step 8a 注入 Attack Agent 使用。

如果 `evolution.enabled=false`，跳过此步骤。

### Step 3.6: 历史情报采集（v2.1 新增，intelligence.enabled=true 时）

**⛔ 铁律：主进程只做编排，不做执行。** 本步骤的所有实质性工作通过 `Agent(subagent_type="testvdb:xxx")` 派发。

如果 `intelligence.enabled=false`，跳过整个 Step 3.6。

**主进程在派发以下 Agent 前，先从 settings.json 读取 intelligence 配置并提取为模板变量：**
```bash
python -c "
import json
with open('settings.json', encoding='utf-8') as f:
    c = json.load(f).get('intelligence', {})
print(f'INTEL_TW={c.get(\"time_window_months\", 24)}')
print(f'INTEL_MI={c.get(\"max_issues\", 500)}')
print(f'INTEL_MC={c.get(\"max_commits\", 200)}')
print(f'INTEL_TTL={c.get(\"cache_ttl_hours\", 720)}')
"
默认值（未自定义时）：time_window_months=24, max_issues=500, max_commits=200, cache_ttl_hours=720。
将输出值作为 shell 变量 source，在后续 prompt 模板中替换 `{INTEL_TW}`、`{INTEL_MI}`、`{INTEL_MC}`、`{INTEL_TTL}`。

#### 3.6a: 检查情报缓存

检查 `intelligence/{target}/threat_model.json` 是否存在且未过期（TTL 由 `settings.json` 的 `intelligence.cache_ttl_hours` 决定，默认 720h = 30 天）。

```bash
# 检查缓存
ls -la intelligence/{target}/threat_model.json 2>/dev/null && echo "CACHE_HIT" || echo "CACHE_MISS"
# 如果命中，检查 TTL
python -c "
import json, os, time
from datetime import datetime, timezone
with open('intelligence/{target}/threat_model.json', encoding='utf-8') as f:
    data = json.load(f)
generated = data['_meta']['generated_at']
# 鲁棒的 ISO 8601 解析（处理带/不带毫秒和时区的格式）
try:
    # Python >=3.11 supports Z; replace with +00:00 for 3.8/3.9/3.10
    clean = generated.replace("Z", "+00:00") if "Z" in generated.upper() else generated
    ts = datetime.fromisoformat(clean)
except ValueError:
    # Fallback: strip to YYYY-MM-DDTHH:MM:SS and assume UTC
    ts = datetime.strptime(generated[:19], '%Y-%m-%dT%H:%M:%S')
    if generated.endswith('Z'):
        ts = ts.replace(tzinfo=timezone.utc)
age_hours = (time.time() - ts.timestamp()) / 3600
print(f'Age: {age_hours:.1f}h')
" 2>/dev/null
```

如果缓存有效 → 跳到 Step 3.6e（仅加载 threat_model 到上下文）。

#### 3.6b: 派发 issue-miner（⛔ 禁止自己爬取 GitHub）

```
Agent(
  subagent_type="testvdb:issue-miner",
  description="采集 {target} 历史 Issues 和 Commits",
  prompt="按照 agents/issue-miner.md 规范，为 {target} 采集历史 Issues 和已合并修复 PR。输入参数: target={target}, version={version}, intelligence_dir=intelligence/{target}/, time_window_months={INTEL_TW}, max_issues={INTEL_MI}, max_commits={INTEL_MC}。将结果写入 intelligence/{target}/issue_corpus.json 和 intelligence/{target}/commit_corpus.json。"
)
```

**等待完成后验证：**
```bash
ls -la intelligence/{target}/issue_corpus.json && ls -la intelligence/{target}/commit_corpus.json && echo "ISSUE_MINER_OK" || echo "ISSUE_MINER_FAILED"
```

**如果 `ISSUE_MINER_FAILED`** → 记录警告到 error_log，跳过后续 3.6c/3.6d，继续 Step 4（Phase 0 非关键路径，不阻塞流水线）。

#### 3.6c: 派发 bug-shape-extractor（⛔ 禁止自己分析 issue）

```
Agent(
  subagent_type="testvdb:bug-shape-extractor",
  description="提取 {target} 历史 Bug Shapes",
  prompt="按照 agents/bug-shape-extractor.md 规范，对 intelligence/{target}/issue_corpus.json 和 intelligence/{target}/commit_corpus.json 进行分类和根因模式提取。输入参数: target={target}, intelligence_dir=intelligence/{target}/, strategy_registry_dir=strategy_registry/。将结果写入 intelligence/{target}/classified_issues.json、intelligence/{target}/bug_shapes.json、intelligence/{target}/developer_cognition.json。"
)
```

**等待完成后验证：**
```bash
ls -la intelligence/{target}/bug_shapes.json && ls -la intelligence/{target}/developer_cognition.json && echo "SHAPE_EXTRACTOR_OK" || echo "SHAPE_EXTRACTOR_FAILED"
```

如果失败 → 记录警告，跳过 3.6d，继续 Step 4。

#### 3.6d: 派发 threat-modeler（⛔ 禁止自己构建威胁模型）

```
Agent(
  subagent_type="testvdb:threat-modeler",
  description="构建 {target} 威胁模型",
  prompt="按照 agents/threat-modeler.md 规范，基于 intelligence/{target}/bug_shapes.json、intelligence/{target}/classified_issues.json、intelligence/{target}/developer_cognition.json 和 THEORETICAL_FRAMEWORK.md 构建威胁模型和认知盲点模型。输入参数: target={target}, version={version}, intelligence_dir=intelligence/{target}/, contract_path=results/{target}/{version}/structured_contract.json（如果存在）。将结果写入 intelligence/{target}/threat_model.json。"
)
```

**等待完成后验证：**
```bash
ls -la intelligence/{target}/threat_model.json && echo "THREAT_MODEL_OK" || echo "THREAT_MODEL_FAILED"
```

如果失败 → 记录警告，继续 Step 4（不阻塞流水线）。

#### 3.6e: 加载情报到上下文

读取威胁模型的简化摘要到内存变量 `threat_model_summary`，供后续步骤使用：

```bash
python -c "
import json
with open('intelligence/{target}/threat_model.json', encoding='utf-8') as f:
    tm = json.load(f)
# 输出关键字段的摘要
print(json.dumps({
    'blindspot_count': len(tm.get('cognitive_blindspots', {}).get('blindspots', [])),
    'high_priority_areas': [a['area'] for a in tm.get('attack_surface', {}).get('high_priority_areas', [])],
    'top_blindspots': [b['blindspot_id'] for b in tm.get('cognitive_blindspots', {}).get('blindspots', [])[:3]],
    'by_design_patterns_count': len(tm.get('defect_criteria', {}).get('by_design_behaviors', [])),
}, indent=2, ensure_ascii=False)
" 2>/dev/null || echo "THREAT_MODEL_NOT_AVAILABLE"
```

**📊 stdout 日志输出（模板，实际运行时替换为 3.6e Python 脚本的实际输出值）：**
```
[Step 3.6] Intelligence: N blindspots identified, M priority areas
[Step 3.6] Top blindspots: BS-XX, BS-YY, BS-ZZ
[Step 3.6] Cache TTL: {INTEL_TTL}h
```

### Step 4: 派 Knowledge Extractor（⛔ 禁止自己爬取文档）
```
Agent(
  subagent_type="testvdb:knowledge-extractor",
  description="提取 {target} {version} 文档知识",
  prompt="按照 agents/knowledge-extractor.md 规范，为 {target} {version} 提取 API 文档知识，产出 raw_knowledge.md。输入参数: target={target}, version={version}。将结果写入 results/{target}/{version}/raw_knowledge.md"
)
```
**等待完成后验证：** `ls -la results/{target}/{version}/raw_knowledge.md`

### Step 5: 派 Contract Formalizer（⛔ 禁止自己生成契约）
```
Agent(
  subagent_type="testvdb:contract-formalizer",
  description="形式化 {target} v{version} API 契约",
  prompt="按照 agents/contract-formalizer.md 规范，将 results/{target}/{version}/raw_knowledge.md 转换为 structured_contract.json。输入参数: target={target}, version={version}。将结果写入 results/{target}/{version}/structured_contract.json"
)
```
**等待完成后验证：** `ls -la results/{target}/{version}/structured_contract.json`

### Step 6: 合同门控检查
检查 `structured_contract.json` 的核心 CRUD 端点覆盖率 ≥ 90%。不通过 → 输出缺失端点 + 终止。

**v2.0 新增 — Passport Hash 验证（material_passport.enabled=true 时）：**
对新生成的 structured_contract.json 执行 hash 验证：
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```
- 退出码 0（PASS）→ 契约完整性确认
- 退出码 2（TAMPERED）→ 异常：契约刚生成 hash 就不匹配，可能是 Agent 写入不完整。
  重试 `contract-formalizer` 一次。如果重试后仍不匹配，标记为 `PASSPORT_TAMPERED` 并终止。

### Step 7: 初始化状态
- 生成 `session_id`: `{target}-{version_short}-{counter}`（sanitize: `[a-z0-9-]`，≤63字符）
- 创建 `results/{target}/{version}/` 目录
- 写入 `mine_state.json` 和 `.session.lock`

### Step 8: 挖掘循环（每轮）

每轮执行以下子步骤。timestamp 子目录在第一轮开始时创建。

#### 8a. 注入 reflection_context + threat_model

第一轮：无 reflection_context，Attack Agents 自由探索。
后续轮次：注入上轮 reflection_context 到 Attack Agents 的 context。

**v2.0 新增 — reflection_context 结构：**
```json
{
  "key_learnings": ["...", "..."],
  "rejection_patterns": [{ "endpoint": "...", "reason": "..." }],
  "high_value_endpoints": ["..."],
  "exhausted_endpoints": ["..."],
  "last_round_summary": "..."
}
```

**reflection_context 注入模板**：在 Agent 调用的 prompt 参数中，将 reflection_context 以纯文本形式注入：
```
上轮经验：{key_learnings 的要点}。已排除的端点：{exhausted_endpoints}。高价值端点：{high_value_endpoints}。驳回模式：{rejection_patterns 的摘要}
```

### v2.0 跨会话策略注入（evolution.enabled=true）

在 reflection_context 之后，追加从 Strategy Registry 读取的策略：
```
## 跨会话策略注入

以下策略来自之前成功挖掘的经验（跨 DB 迁移）：

{cross_session_strategies 的输出}

使用这些策略作为初始 seed。对于标记了 applicable_dbs 包含当前 DB 的策略，
应用 migration_rules 中的 DB 特定适配规则。
```

策略由 `scripts/strategy_injector.py {target} --text-only` 生成。

### v2.1 威胁模型与认知盲点注入（intelligence.enabled=true 且 inject_to_attack_agents=true）

**使用程序化注入脚本生成 Attack Agent 的威胁模型注入文本：**

```bash
THREAT_MODEL_ATTACK=$(python scripts/threat_model_injector.py {target} --mode attack --text-only 2>/dev/null || echo "")
```

在派发 Attack Agent 时，将 `${THREAT_MODEL_ATTACK}` 追加到 prompt 末尾（跨会话策略之后）。如果 threat_model.json 不存在，脚本输出 `（威胁模型数据不可用）`——流水线不中断。

**注入条件汇总**：
- `reflection_context != null` → 注入本轮经验
- `evolution.enabled=true` 且 `cross_session_strategies` 有实质内容 → 注入跨会话策略
- `intelligence.enabled=true` 且 `inject_to_attack_agents=true` → 执行 `threat_model_injector.py --mode attack` 并注入结果

### v2.1 Judge Agent 增强注入（intelligence.enabled=true 且 inject_to_judge_agents=true）

**使用程序化注入脚本生成各 Judge Agent 的增强注入文本：**

```bash
THREAT_MODEL_JUDGE_SEVERITY=$(python scripts/threat_model_injector.py {target} --mode judge --judge-type severity --text-only 2>/dev/null || echo "")
THREAT_MODEL_JUDGE_NOVELTY=$(python scripts/threat_model_injector.py {target} --mode judge --judge-type novelty --text-only 2>/dev/null || echo "")
THREAT_MODEL_JUDGE_EVIDENCE=$(python scripts/threat_model_injector.py {target} --mode judge --judge-type evidence --text-only 2>/dev/null || echo "")
```

在派发对应 Judge Agent 时（Step 8e），将 `${THREAT_MODEL_JUDGE_*}` 追加到 prompt 末尾。

**注入映射**：
- `judge-severity` → `${THREAT_MODEL_JUDGE_SEVERITY}`（severity_calibration：by-design 降级、历史高严重性确认、wontfix 降级）
- `judge-novelty` → `${THREAT_MODEL_JUDGE_NOVELTY}`（novelty_context：最近修复模式、已知进行中 issue、回归风险区域）
- `judge-evidence` → `${THREAT_MODEL_JUDGE_EVIDENCE}`（submission_success_probability：基于开发者历史态度预测提交成功率）

**v2.0 新增 — Fan-Out 模式（fan_out.enabled=true 时）：**

每个 Attack Agent 派发 `fan_out.seeds_per_agent` 次（默认 3），每次使用不同的 `focus_profile`：

```
Agent(subagent_type="testvdb:attack-boundary", description="边界攻击 {target} focus=priority_first",
  prompt="按照 agents/attack-boundary.md 规范，为 {target} v{version} 生成边界攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}, focus_profile=priority_first")

Agent(subagent_type="testvdb:attack-boundary", description="边界攻击 {target} focus=coverage_gap",
  prompt="按照 agents/attack-boundary.md 规范，focus_profile=coverage_gap。优先测试 coverage.json 中覆盖率最低的端点。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-boundary", description="边界攻击 {target} focus=rejection_pattern",
  prompt="按照 agents/attack-boundary.md 规范，focus_profile=rejection_pattern。从上轮驳回模式反向推导新攻击，绕过已知驳回路径。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-state", description="状态攻击 {target} focus=priority_first",
  prompt="按照 agents/attack-state.md 规范，为 {target} v{version} 生成状态攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}, focus_profile=priority_first")

Agent(subagent_type="testvdb:attack-state", description="状态攻击 {target} focus=coverage_gap",
  prompt="按照 agents/attack-state.md 规范，focus_profile=coverage_gap。优先测试 coverage.json 中覆盖率最低的端点。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-state", description="状态攻击 {target} focus=rejection_pattern",
  prompt="按照 agents/attack-state.md 规范，focus_profile=rejection_pattern。从上轮驳回模式反向推导新攻击，绕过已知驳回路径。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-semantic", description="语义攻击 {target} focus=priority_first",
  prompt="按照 agents/attack-semantic.md 规范，为 {target} v{version} 生成语义攻击脚本。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}, focus_profile=priority_first")

Agent(subagent_type="testvdb:attack-semantic", description="语义攻击 {target} focus=coverage_gap",
  prompt="按照 agents/attack-semantic.md 规范，focus_profile=coverage_gap。优先测试 coverage.json 中覆盖率最低的端点。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")

Agent(subagent_type="testvdb:attack-semantic", description="语义攻击 {target} focus=rejection_pattern",
  prompt="按照 agents/attack-semantic.md 规范，focus_profile=rejection_pattern。从上轮驳回模式反向推导新攻击，绕过已知驳回路径。contract=results/{target}/{version}/structured_contract.json, session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}, reflection_context={reflection_context}")
```

**v2.1 威胁模型注入（每个 Attack Agent prompt 末尾）：**

以上所有 Attack Agent 的 prompt 末尾必须追加 `${THREAT_MODEL_ATTACK}`（由 Step 8a 中的 `threat_model_injector.py --mode attack` 生成）。即：

```
<原 prompt>\n\n${THREAT_MODEL_ATTACK}
```

如果 `${THREAT_MODEL_ATTACK}` 为空（threat_model.json 不存在或 intelligence 禁用），不影响流水线。

**9 个 Agent 全部并行派发**。超时机制不变（3 分钟无产出 → 超时）。部分超时不影响其他 seed。

**汇聚与去重（fan_out.enabled=true 时）：**
```bash
find results/{target}/{version}/{timestamp} -name "*.py" -type f ! -path "*/mre/*" ! -name "_stage1*" ! -name "script_*" 2>/dev/null | wc -l
```
为 0 则报错终止。

**3 级去重（主进程自行执行）：**
1. 按 (endpoint, constraint_id, strategy) 三元组去重
2. 相同三元组 → 保留 confidence 最高的版本
3. 不同 seed 独立生成相同脚本 → confidence +0.1（独立验证奖励）

**如果 fan_out.enabled=false 或 seeds_per_agent=1 → 回退到 v1.x 行为（3 并发，无 focus_profile）。**

#### 8c. 辩论 Stage 1（主进程自行审查——这是编排工作，可自己做）

1. 收集三个 Agent 产出的脚本，按来源标记 boundary/state/semantic
2. 自动去重（endpoint + constraint_id + strategy）
3. 语法验证（`python -m py_compile`）
4. 约束存在性验证（constraint_id 在 contract 中存在）
5. **v2.1.1 脚本错误启发式检测（⛔ 关键——防止脚本错误被误判为数据库缺陷）**
   对每个脚本执行静态检测：
   ```bash
   python scripts/detect_risky_scripts.py "results/{target}/{version}/{timestamp}"
   ```
   标记输出中的 `RISKY_SCRIPT`，在后续执行日志中优先检查这些脚本的 SCRIPT_ERROR。

6. **v2.2 新增 — API 调用格式结构化验证（AST 级别）**
   用 AST 检测脚本是否使用 `safe_request()` 包装 API 调用，拒绝裸 `.json()` 链式调用：
   ```bash
   python scripts/validate_api_format.py "results/{target}/{version}/{timestamp}"
   ```
   **判定**：裸 `.json()` 链式调用 → **REJECT**（直接丢弃脚本）。`safe_request` 定义但未调用 → **REJECT**。**裸 `.json()` 被拒绝的脚本需从后续步骤中排除。**

7. 审查结果写入 `debate_logs/stage1.json`
8. 将通过审查的脚本复制到标准路径

#### 8d. 派 Docker Executor（⛔ 禁止自己运行脚本）

**⛔ 所有子 Agent prompt 末尾必须包含嵌套派发禁令（详见 `agents/orchestrator.md` 顶部执行模型变更）。**

```
Agent(
  subagent_type="testvdb:docker-executor",
  description="执行 {target} v{version} 攻击脚本",
  prompt="按照 agents/docker-executor.md 规范，在 Docker 沙箱中执行攻击脚本。target={target}, version={version}, SESSION_DIR=${PROJECT_ROOT}/results/{target}/{version}/{timestamp}, session_id={session_id}。⛔ 立即执行 Step 1 命令，不要分析、不要检查、不要读取脚本内容。脚本位于 SESSION_DIR 下的 boundary_scripts/、state_scripts/、scripts/ 子目录和 script_*.py 文件中。所有脚本已通过语法验证，无需再检查。\n\n你是 TestVDB 流水线中被主进程派发的子 Agent。禁止使用 Agent 工具派发孙 Agent — 插件体系不支持嵌套派发，调用会静默失败。所有产出必须通过 Write/Bash/Read 工具直接完成。"
)
```
**等待完成后验证：** `ls results/{target}/{version}/{timestamp}/output_*.log.done 2>/dev/null | wc -l`，为 0 则报错终止。

#### 8d.5 打回修改机制（v2.1.1 新增 — 替代静态分析）

**目标：不再丢弃脚本错误，而是打回给 Attack Agent 修复后重新执行。**

1. **扫描脚本错误**（主进程执行）：
```bash
python scripts/scan_script_errors.py "results/{target}/{version}/{timestamp}"
```
输出 JSON 包含 `errored_count` 和 `scripts` 列表。

2. **如果 errored_count = 0** → 跳过打回修改，直接进入 Step 8e。

3. **如果 errored_count > 0** → 对每个出错脚本执行打回修改（最多 2 轮）：

   3a. **识别脚本来源**：从脚本文件名判断 attack 类型
   - `boundary_*` → `testvdb:attack-boundary`
   - `state_*` → `testvdb:attack-state`
   - `semantic_*` → `testvdb:attack-semantic`

   3b. **派发打回修改 Agent**（⛔ 禁止自己修改脚本）：
   ```
   Agent(
     subagent_type="testvdb:attack-{type}",
     description="打回修改 {script_base}",
     prompt="## 打回修改任务

你的脚本 {script_base}.py 在执行时发生了 Python 错误（非数据库缺陷）。

错误日志:
```
{error_context}
```

请根据错误日志修复脚本。常见修复：
- 用 safe_request() 包装所有 HTTP 调用
- 检查 .json() 返回值类型再调用 .get()
- 添加 try/except 捕获 json.JSONDecodeError
- 修复后脚本末尾打印 VERDICT: DEFECT_FOUND / NO_DEFECT / SCRIPT_ERROR

直接 Write 修复后的脚本到原路径: {session_dir}/{type}_scripts/{script_base}.py")
   ```

   3c. **重新执行修复后的脚本**：
   ```bash
   # 使用 Docker Executor 统一执行（不绕过沙箱）
   SCRIPT_PATH="{session_dir}/{type}_scripts/{script_base}.py"
   # 优先使用已检测的 PYTHON，带引号防注入
   if [ -n "$PYTHON" ]; then
     "$PYTHON" "$SCRIPT_PATH" > "{session_dir}/output_{script_base}.log" 2>&1
   else
     python3 "$SCRIPT_PATH" > "{session_dir}/output_{script_base}.log" 2>&1 || python "$SCRIPT_PATH" > "{session_dir}/output_{script_base}.log" 2>&1
   fi
   echo $? > "{session_dir}/output_{script_base}.log.exit"
   touch "{session_dir}/output_{script_base}.log.done"
   ```

   3d. **检查修复结果**：再次扫描修复后的日志。如果仍有 SCRIPT_ERROR → 第 2 轮打回。最多 2 轮，2 轮后仍失败 → 标记 `UNFIXABLE`，跳过该脚本。

4. **记录打回修改统计**：
```
[Step 8d.5] Reject-and-Revise: N scripts errored, M fixed, K unfixable after 2 rounds
```

#### 8e. 辩论 Stage 2 — 派 Judge Quartet + Fallback

**阶段 1：先派 judge-doc**
```
Agent(subagent_type="testvdb:judge-doc", description="文档契约验证 {target}",
  prompt="按照 agents/judge-doc.md 规范，验证以下候选缺陷的文档引用有效性：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}")
```
**等待完成后验证：** `test -f "results/{target}/{version}/{timestamp}/debate_logs/stage2_doc.json.done" && echo "READY"`

**阶段 2：并发派其他 3 个 Judge + 超时 fallback**

先并发派发 3 个 Judge Agent：
```
Agent(subagent_type="testvdb:judge-evidence", description="证据审查 {target}",
  prompt="按照 agents/judge-evidence.md 规范，审查以下执行结果的证据可信度：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}\n\n${THREAT_MODEL_JUDGE_EVIDENCE}")

Agent(subagent_type="testvdb:judge-novelty", description="新颖性审查 {target}",
  prompt="按照 agents/judge-novelty.md 规范，审查以下候选缺陷的新颖性：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}\n\n${THREAT_MODEL_JUDGE_NOVELTY}")

Agent(subagent_type="testvdb:judge-severity", description="严重性评估 {target}",
  prompt="按照 agents/judge-severity.md 规范，评估以下候选缺陷的严重程度：{execution_results}。session_id={session_id}, session_dir=results/{target}/{version}/{timestamp}\n\n${THREAT_MODEL_JUDGE_SEVERITY}")
```

**v2.1 Judge 增强注入（每个 Judge Agent prompt 末尾）：**

以上 Judge Agent 的 prompt 末尾已追加对应的 `${THREAT_MODEL_JUDGE_*}` 变量（由 Step 8a 中的 `threat_model_injector.py --mode judge` 生成）：
- `judge-evidence` → `${THREAT_MODEL_JUDGE_EVIDENCE}`
- `judge-novelty` → `${THREAT_MODEL_JUDGE_NOVELTY}`
- `judge-severity` → `${THREAT_MODEL_JUDGE_SEVERITY}`

如果对应变量为空（threat_model.json 不存在或 intelligence 禁用），不影响流水线。

**等待全部完成后验证产出：**
```bash
echo "doc: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_doc.json.done && echo 1 || echo 0)"
echo "evidence: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_evidence.json.done && echo 1 || echo 0)"
echo "novelty: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_novelty.json.done && echo 1 || echo 0)"
echo "severity: $(test -f results/{target}/{version}/{timestamp}/debate_logs/stage2_severity.json.done && echo 1 || echo 0)"
```

**Fallback 机制：如果任一 Judge 的 .done 不存在，主进程基于执行日志自动生成默认评估文件（编排容错，非替代 Judge）：**

- **evidence fallback**：读取缺陷列表+执行日志，基于 FAILED/PASSED 行判定 is_defect/not_defect，写 stage2_evidence.json+.done
- **severity fallback**：基于 defect_type 映射（Type1→High, Type2→Medium, Type3/4→High），写 stage2_severity.json+.done
- **novelty fallback**：全部标记为 unknown, vote=is_defect，写 stage2_novelty.json+.done

**投票逻辑和缺陷确认规则见 `agents/orchestrator.md` Step 8e。**

#### 8e.5 缺陷去重（v2.2 新增 — 防止同一根因的多个缺陷重复报告）

**主进程在派发 Reporter 之前，必须对 confirmed_defects 执行去重。** 去重维度：

1. **同 endpoint + 同 defect_type 去重**：相同端点、相同缺陷类型的多个候选 → 合并为单个缺陷，列出所有 reproduction scenario
2. **跨轮次去重**：与本 session 前几轮已确认的缺陷比较，相同 root cause → 丢弃（记录到 `dedup_log.json`）
3. **合并规则**：合并后的缺陷保留最高 severity，evidence 取 AND（所有场景的证据合并）

去重脚本（主进程执行）：
```bash
python scripts/dedup_defects.py "results/{target}/{version}/{timestamp}"
```
**如果去重后数量为 0** → 本轮无新缺陷，跳过 Reporter，直接进入 8g。

#### 8f. 派 Reporter（⛔ 禁止自己生成报告）
```
Agent(
  subagent_type="testvdb:reporter",
  description="生成缺陷报告 {target}",
  prompt="按照 agents/reporter.md 规范，为以下确认的缺陷生成报告：{confirmed_defects}。session_id={session_id}, target={target}, version={version}, session_dir=results/{target}/{version}/{timestamp}"
)
```
**等待完成后验证：** `ls results/{target}/{version}/{timestamp}/defects/defect-*.md 2>/dev/null | wc -l`

#### 8f.5 逐缺陷全面审查（v2.2 新增 — 每轮末尾逐条审核）

**⛔ 铁律：主进程只做编排，不做执行。** 本步骤通过 `Agent(subagent_type="testvdb:judge-evidence")` 对 Reporter 产出逐条审查。

对每个 `defect-N.md`，审查以下维度：
1. **证据链完整性**：Ring 1（契约引用）、Ring 2（文档引用）、Ring 3（执行日志）是否齐全
2. **严重性校准**：基于实际执行日志重新确认 severity 是否合理
3. **脚本错误排除**：检查原始日志是否包含 SCRIPT_ERROR 标记
4. **假阳性识别**：对比日志中的 VERDICT 行和 defect 报告的声称

审查脚本（主进程执行）：
```bash
python scripts/verify_defects.py "results/{target}/{version}/{timestamp}"
```
产出 `defect-review.md`，标记每个缺陷为 CONFIRMED / FALSE_POSITIVE / NEEDS_IMPROVEMENT。

**审查不通过的处理**：
- FALSE_POSITIVE → 删除对应 defect-N.md
- NEEDS_IMPROVEMENT → 打回 Reporter 重写（最多 1 次）
- 全部通过 → 继续 8g

#### 8g-8i: 保存状态、分析产出、检查终止条件
主进程自行完成：保存 `mine_state.json` + `coverage.json` + `experience_handoff.json`，分析本抡产出，检查终止条件（连续5轮无新缺陷 / 覆盖率≥95% / max_rounds 达到 / min_defects 达到）。

> **上下文压缩**：Claude Code 内置自动压缩 + `autoCompactEnabled: true`。PreCompact hook（`testvdb-pre-compact.js`）自动保存状态，PostCompact hook（`testvdb-post-compact.js`）自动注入恢复提示。流水线无需暂停，自动压缩后主进程从磁盘恢复继续执行。

#### 8j: 轮次间容器管理
继续下一轮 → `docker restart`。终止 → `docker compose down -v`。

### Step 9: 生成 Issue 草稿 + 汇总 + 清理

#### 9a. 生成 Issue 草稿（v2.2 新增 — 本地 MD 文件，不上传 GitHub）

**⛔ 绝对禁止：主进程或任何 Agent 直接提交 Issue 到 GitHub。所有产出仅限本地文件系统。**

对每个通过 8f.5 审查的缺陷，生成独立的 GitHub Issue 格式草稿：

```bash
mkdir -p results/{target}/{version}/{timestamp}/issues
```

每个 issue 草稿文件 `issues/issue-{N}-{slug}.md` 包含：
- Title: `Bug: {简短描述}`
- Description, Version, Steps to Reproduce, Expected Behavior, Actual Behavior, Impact, Environment
- 关联的 MRE 脚本路径
- 底部标注 `🤖 Generated with [Claude Code](https://claude.com/claude-code)`（本地草稿，需人工审核后手动提交）

#### 9a.5 Issue 审核提醒（v2.1.2 新增）

> ⚠️ **人工审核必需**：上述 Issue 草稿由 AI 生成，仅作为本地参考。
> 在提交到 GitHub 之前，必须由人类工程师完成以下审核：
> 1. [ ] 确认缺陷在当前最新版本中仍然存在
> 2. [ ] 验证复现步骤的准确性和完整性
> 3. [ ] 检查是否已有其他用户报告的重复 Issue
> 4. [ ] 调整语气和格式以符合目标项目的 Issue 模板
> 5. [ ] 移除 AI 生成标记，以个人身份提交
>
> **Issue 草稿路径**: `results/{target}/{version}/{timestamp}/issues/issue-{N}-{slug}.md`

#### 9b. 生成 `summary.md` + `defect-review.md`

#### 9c. 清理

**v2.0 新增 — 策略提取（evolution.enabled=true 时）：**

```bash
python scripts/strategy_extractor.py "results/{target}/{version}/{timestamp}" {target}
```
- 清理 Docker 容器和网络
- 更新 `.session.lock` status 为 `completed`

### Step 10: 标记完成

---

## Termination Conditions

1. **Stalemate**: 连续 5 轮无新缺陷
2. **Coverage**: 合同覆盖率 ≥ 95%
3. **Max Rounds**: `--max-rounds` 达到（且 > 0）
4. **Min Defects**: `--min-defects` 达到

## Output

```
results/{target}/{version}/{timestamp}/
├── defects/defect-1.md              # Reporter 产出的详细报告
├── mre/defect-1-script.py           # 自包含可复现脚本
├── issues/issue-1-batch-atomicity.md # GitHub Issue 格式草稿（本地，不提交）
├── defect-review.md                  # 8f.5 逐缺陷审查结果
├── summary.md                        # 会话汇总
├── debate_logs/
│   ├── stage1.json
│   ├── stage2_aggregation.json
│   ├── stage2_deduped.json           # 8e.5 去重后结果
│   ├── stage2_doc.json
│   ├── stage2_evidence.json
│   ├── stage2_novelty.json
│   └── stage2_severity.json
├── structured_contract.json
├── mine_state.json
├── coverage.json
├── experience_handoff.json
└── session_metadata.json

intelligence/{target}/
├── issue_corpus.json         # 原始 Issue 语料（v2.1 新增）
├── commit_corpus.json        # 原始 Commit/PR 语料（v2.1 新增）
├── classified_issues.json    # 三分类结果（v2.1 新增）
├── bug_shapes.json           # 根因模式提取（v2.1 新增）
├── developer_cognition.json  # 开发者认知分析（v2.1 新增）
└── threat_model.json         # 威胁模型 + 认知盲点模型（v2.1 新增）
```

## Error Recovery

重新运行相同命令可恢复中断的会话。系统自动检测未完成会话。

## Multi-DB Mining

```bash
# Terminal 1
/testvdb:mine milvus v2.4.0
# Terminal 2
/testvdb:mine qdrant v1.13.0
```
