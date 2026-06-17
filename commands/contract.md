---
description: 单独生成/刷新指定 DB 版本的文档知识与结构化契约
allowed-tools: Read, Write, Bash, Grep, Glob, Agent
---

# /testvdb:contract — 文档提取 + 契约生成

单独为指定向量数据库版本提取官方文档知识并形式化为结构化契约（`structured_contract.json`）。**只跑文档+契约**，不启动攻击/执行/judge/reporting。用于调试 contract-formalizer、验证契约（如 bug #3 category）、刷新过期契约。

---

## ⚠️ 架构约束（CRITICAL — 技术原因）

**与 `/testvdb:mine` 相同：主进程永远只做编排，不做执行。**

| 禁止事项 | 正确做法 |
|---------|---------|
| ❌ 使用 WebSearch/WebFetch 爬取文档 | ✅ `Agent(subagent_type="testvdb:knowledge-extractor")` |
| ❌ 自己生成 structured_contract.json | ✅ `Agent(subagent_type="testvdb:contract-formalizer")` |

主进程只使用 `Read`/`Write`/`Bash`(验证)/`Grep`/`Glob`/`Agent` 做编排。

> **派发纪律**：派 `testvdb:*` 子 Agent **只用 `Agent(subagent_type=...)`**；❌ 禁用 `TaskCreate`（不识别 plugin agent_type → `Spawning agent: unknown`，任务永久 `pending` 幽灵条目，`TaskStop` 删不掉，背后无真实 agent 执行）。`Agent` 是核心内置工具，直接调用（`ToolSearch` 搜不到 ≠ 不可用）。详见 `commands/mine.md`「派发工具纪律」。

---

## Usage

```
/testvdb:contract <db> <version> [--force]
```

## Parameters

| Parameter | Required | Default | Description |
|-----------|----------|---------|-------------|
| `<db>` | Yes | — | `milvus`, `qdrant`, `weaviate`, 或 `pgvector` |
| `<version>` | Yes | — | 目标版本号（如 `1.38.0`） |
| `--force` | No | — | 强制重新生成，忽略缓存（即使缓存有效也重跑） |

---

## 执行步骤

### Step 1: 解析参数 + 前置检查

- 验证 `target` ∈ {milvus, qdrant, weaviate, pgvector}
- 解析 `version`、`force`
- 确定 `PROJECT_ROOT`: `git rev-parse --show-toplevel 2>/dev/null || pwd`
- 前置检查：`python scripts/preflight.py`

### Step 2: 缓存检查（D 判断，批次 D）

检查 `results/{target}/{version}/structured_contract.json` 是否可复用：

```bash
python scripts/check_cache.py contract "results/{target}/{version}" {target} {version} --ttl {knowledge.cache_ttl_hours}
```

- **USABLE**（exit 0）且**未传 `--force`** → 跳到 [Step 6: 输出](#step-6-输出)（报告"缓存有效，跳过生成"）
- **STALE / INVALID / MISSING** 或 **传了 `--force`** → 继续 Step 3 重新生成

> TTL 默认 168h（读 `settings.json` 的 `knowledge.cache_ttl_hours`）。

### Step 3: 派 Knowledge Extractor

```
Agent(subagent_type="testvdb:knowledge-extractor",
  description="提取 {target} {version} 文档知识",
  prompt="按照 agents/knowledge-extractor.md 规范，为 {target} {version} 提取 API 文档知识。将结果写入 results/{target}/{version}/raw_knowledge.md")
```

**验证：** `ls -la results/{target}/{version}/raw_knowledge.md`

### Step 4: 派 Contract Formalizer

```
Agent(subagent_type="testvdb:contract-formalizer",
  description="形式化 {target} v{version} API 契约",
  prompt="按照 agents/contract-formalizer.md 规范，将 results/{target}/{version}/raw_knowledge.md 转换为 structured_contract.json。将结果写入 results/{target}/{version}/structured_contract.json")
```

**验证：** `ls -la results/{target}/{version}/structured_contract.json`

### Step 5: 合同门控检查

契约合法性验证（批次 B 的通用 `validate_contract`）：

```bash
python scripts/validate_contract.py "results/{target}/{version}/structured_contract.json"
```

- exit 0（PASS，可能有 warnings）→ 通过
- exit 1（FAIL，有 errors）→ 输出错误 + 终止（契约不合格，不可用于挖掘）
- exit 2（加载/用法错误）→ 终止

**Passport Hash 验证**（`material_passport.enabled=true` 时）：
```bash
python scripts/passport_verify.py "results/{target}/{version}/structured_contract.json"
```

### Step 6: 输出

报告：
- 契约路径：`results/{target}/{version}/structured_contract.json`
- 端点数（`len(api_endpoints)`）、category 分布、data_types 数
- 来源：缓存复用 / 新生成
- 门控结果：PASS / FAIL（warnings 数）

---

## 独立性

本命令**只跑文档提取 + 契约生成 + 门控**，不启动：
- ❌ 攻击生成（attack-boundary/state/semantic）
- ❌ Docker 执行（docker-executor）
- ❌ Judge 辩论（judge-*）
- ❌ 报告生成（reporter）

典型用途：
1. **契约调试**：单独验证 contract-formalizer 输出（如 bug #3 的 category 中立化）
2. **契约刷新**：`--force` 强制重新生成过期契约
3. **跨 DB 迁移前验证**：为目标 DB 生成契约，确认端点覆盖

---

## 与 /testvdb:mine 的关系

`/testvdb:mine` 的契约阶段（智能消费）在缓存缺失/过期时调用与本命令**完全相同**的 agent 派发逻辑（knowledge-extractor → contract-formalizer → 门控）。本命令是 mine 契约阶段的**独立可触发版本**。详见 `commands/mine.md` Step 3。
