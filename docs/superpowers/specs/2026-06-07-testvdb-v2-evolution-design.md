# TestVDB v2.0 — 自进化与质量增强设计规格

**版本**: 1.0
**日期**: 2026-06-07
**状态**: 待评审

---

## 1. 概述

### 1.1 目标

在 TestVDB v1.x（12 Agent 流水线 + 4-Judge 辩论 + Docker 沙箱）基础上，引入四个新能力：

| 优先级 | 改造项 | 核心增量 |
|--------|--------|---------|
| P0 | 跨会话自进化引擎 | experience_handoff → Strategy Registry（跨 DB、跨会话） |
| P1 | Fan-Out Attack Trio | 3 Agent → 9 seed 并行生成 |
| P1 | 7-mode AI Failure Checklist | Reporter 自检 + 关键异常挂起 |
| P2 | Material Passport | 契约版本化 + hash 防篡改 |
| P2 | data_access_level | Agent 数据权限文档约定 |
| P3 | Marketplace 分发 | 一键安装体验 |

### 1.2 设计原则

- **最小侵入**：不改变现有流水线结构（Step 1-10 SOP），只在关键节点插入新行为
- **渐进增强**：每个改造可独立启用/禁用，fallback 到 v1.x 行为
- **文件通信**：沿用现有 Agent 间文件通信机制（.done 标记、pipeline_state.json）
- **混合模式**：默认自主运行，关键异常挂起等待人工介入

### 1.3 术语

| 术语 | 定义 |
|------|------|
| Strategy Registry | 跨 DB、跨会话的策略知识库（替代 experience_handoff.json） |
| Fan-Out | 同一 Attack Agent 用不同 seed 多次生成，增加策略多样性 |
| Pre-Submit Gate | Reporter 生成最终报告前的阻断检查点 |
| Material Passport | 结构化契约的元数据+hash，确保契约完整性 |
| data_access_level | Agent 声明式数据访问权限标记 |

---

## 2. P0: 跨会话自进化引擎

### 2.1 现状问题

- `experience_handoff.json` 仅在单次会话的轮次间传递
- 会话结束后所有经验丢失
- 从 Milvus 挖掘中学到的有效攻击策略无法迁移到 Qdrant
- 没有机制让过去的 false positive 模式指导未来的攻击生成

### 2.2 设计

#### 2.2.1 目录结构

```
TestVDB/
  strategy_registry/
    global_strategies.json       ← 跨 DB 通用策略
    milvus_strategies.json       ← Milvus 特有策略
    qdrant_strategies.json       ← Qdrant 特有策略
    weaviate_strategies.json     ← Weaviate 特有策略
    pgvector_strategies.json     ← PGVector 特有策略
    evolution_log.jsonl          ← 策略演化审计日志（每行一条 JSON）
```

**evolution_log.jsonl 格式**：
```json
{"ts":"2026-06-07T14:00:00Z","event":"strategy_created","strategy_id":"enum_boundary_injection_v1","origin_db":"milvus","origin_defect":"defect-001"}
{"ts":"2026-06-07T15:00:00Z","event":"strategy_migrated","strategy_id":"enum_boundary_injection_v1","from_db":"milvus","to_db":"qdrant","success":true}
{"ts":"2026-06-07T16:00:00Z","event":"confidence_updated","strategy_id":"enum_boundary_injection_v1","old":0.75,"new":0.82,"reason":"confirmed in qdrant"}
{"ts":"2026-06-07T17:00:00Z","event":"strategy_deprecated","strategy_id":"enum_boundary_injection_v1","reason":"3 consecutive false_positives in weaviate"}
```

#### 2.2.2 策略条目 Schema

```json
{
  "strategy_id": "enum_boundary_injection_v1",
  "category": "boundary|state|semantic",
  "origin": {
    "db": "milvus",
    "version": "2.6.17",
    "session_id": "milvus-2617-r1",
    "defect_id": "defect-001",
    "created_at": "2026-06-07T14:00:00Z"
  },
  "pattern": {
    "name": "枚举参数边界外值注入",
    "description": "对枚举类型参数，测试所有不在合法值集合内的值（包括 null、空字符串、大小写变体、未定义值）",
    "template": "For parameter {param_name} with enum values {valid_values}, test: null, empty_string, case_variants, {boundary_values}",
    "constraint_types": ["enum_constraint", "type_constraint"],
    "applicable_endpoints": ["*+create", "*+insert", "*+search"]
  },
  "migration": {
    "applicable_dbs": ["milvus", "qdrant", "weaviate", "pgvector"],
    "confirmed_dbs": ["milvus", "qdrant"],
    "rejected_dbs": [],
    "migration_rules": {
      "qdrant": "replace milvus Collection with qdrant Collection API",
      "weaviate": "use GraphQL mutation equivalent",
      "pgvector": "use SQL INSERT equivalent"
    }
  },
  "performance": {
    "total_attempts": 5,
    "defects_found": 3,
    "false_positives": 1,
    "avg_confidence": 0.82,
    "last_used": "2026-06-07T16:00:00Z"
  },
  "status": "active|deprecated|experimental"
}
```

#### 2.2.3 工作流

```
Mining Session 结束 (Step 9)
  │
  ├── 1. 读取本轮 experience_handoff.json
  │      └── 提取 confirmed_defects 的策略模式
  │
  ├── 2. 策略提取（主进程自行完成——编排工作）
  │      ├── 对每个确认缺陷：提取 attack_type + constraint_type + endpoint 模式
  │      ├── 泛化：将 DB 特定的 API 调用替换为抽象模式
  │      └── 交叉分析：检查相同模式是否已在其他 DB 的 registry 中存在
  │
  ├── 3. 策略合并
  │      ├── 新策略 → 以 strategy_id 写入对应 DB 的 registry
  │      ├── 已有策略 → 更新 performance 计数 + 调整 confidence
  │      └── 追加 evolution_log.jsonl 审计条目
  │
  └── 4. 下一会话注入
         Attack Agent 启动时 → 读取 global + 目标 DB 的 registry
         → 高 confidence (>0.7) 策略作为优先攻击模板
```

#### 2.2.4 注入方式

Attack Agent prompt 修改（在现有 reflection_context 之后追加）：

```
## 跨会话策略注入

以下策略来自之前成功挖掘的经验（跨 DB 迁移）：

{从 strategy_registry 序列化的策略列表，每条包含 pattern.template 和 migration_rules}

使用这些策略作为初始 seed。对于标记了 applicable_dbs 包含当前 DB 的策略，
应用 migration_rules 中的 DB 特定适配规则。
```

#### 2.2.5 误迁移防护

- `rejected_dbs` 字段记录在哪些 DB 上被证明无效
- 连续 3 次 false positive 的 DB → 自动加入 `rejected_dbs`
- `status = deprecated` 的策略不再注入，但保留在 registry 中供审计
- 策略注入时附带 `confidence`, Agent 应降低对低 confidence 策略的依赖

#### 2.2.6 文件改动

| 文件 | 改动 |
|------|------|
| `commands/mine.md` | Step 8a 增加 strategy registry 读取逻辑；Step 9 增加策略提取+合并逻辑 |
| `agents/orchestrator.md` | Step 8a 增加策略注入模板；Step 8h 增加策略提取步骤 |
| `agents/attack-boundary.md` | 增加跨会话策略消费指令 |
| `agents/attack-state.md` | 增加跨会话策略消费指令 |
| `agents/attack-semantic.md` | 增加跨会话策略消费指令 |
| `strategy_registry/` | 新建目录 + 各 DB 的初始空策略文件 |
| `skills/pipeline/SKILL.md` | Phase 3 增加策略注入步骤 |

---

## 3. P1: Fan-Out Attack Trio

### 3.1 现状问题

- 每个 Attack Agent 单次生成，策略多样性受限于单次 LLM 采样
- 同类型 Agent 内没有多样性机制

### 3.2 设计

#### 3.2.1 Fan-Out 模型

每个 Attack Agent 派发 3 次，每次用不同的 `focus_profile`：

```
Attack Round N:
  attack-boundary  ─┬─ seed: priority_first   → 脚本集 B1
                    ├─ seed: coverage_gap      → 脚本集 B2
                    └─ seed: rejection_pattern → 脚本集 B3

  attack-state     ─┬─ seed: priority_first   → 脚本集 S1
                    ├─ seed: coverage_gap      → 脚本集 S2
                    └─ seed: rejection_pattern → 脚本集 S3

  attack-semantic  ─┬─ seed: priority_first   → 脚本集 M1
                    ├─ seed: coverage_gap      → 脚本集 M2
                    └─ seed: rejection_pattern → 脚本集 M3
```

9 组脚本 → 统一汇聚 → Stage 1 去重 + 交叉审查

#### 3.2.2 Focus Profile 定义

| Profile | 策略 | Agent prompt 差异 |
|---------|------|-------------------|
| `priority_first` | 从 contract 中 severity 最高的约束开始 | 无额外指令（当前默认行为） |
| `coverage_gap` | 从 coverage.json 中覆盖率最低的端点开始 | 注入 uncovered_endpoints 列表 |
| `rejection_pattern` | 从上轮 false positive 反向推导新攻击 | 注入 rejection_patterns，要求"绕过已知驳回模式" |

#### 3.2.3 去重与汇聚流程

```
9 组脚本收集完毕
  │
  ├── Step 1: 按 (endpoint, constraint_id, strategy) 三级去重
  │     └── 相同三元组 → 保留 confidence 最高的版本
  │
  ├── Step 2: 跨 profile 重复检测
  │     └── 不同 seed 独立生成相同脚本 → confidence +0.1（独立验证奖励）
  │
  └── Step 3: 进入原有 Stage 1 语法验证+约束验证
```

#### 3.2.4 并发控制

- 9 个 Agent 全部并行派发
- 超时机制不变（3 分钟无产出 → 超时）
- 部分超时不影响其他 seed
- `fan_out.seeds_per_agent` 可在 settings.json 中调低（如设为 1 则回退到 v1.x 的单 seed 行为）
- 首轮建议：先用 2 seed 测试，确认去重逻辑正确后再增加到 3

#### 3.2.5 文件改动

| 文件 | 改动 |
|------|------|
| `commands/mine.md` | Step 8b 从 3 并发改为 9 并发 + 汇聚步骤 |
| `agents/orchestrator.md` | Step 8b 增加 focus_profile 参数规范 |
| `skills/pipeline/SKILL.md` | Phase 3 更新 Fan-Out 描述 |

---

## 4. P1: 7-Mode AI Failure Checklist

### 4.1 现状问题

- Judge Quartet 负责判定缺陷，但没有机制检查 LLM 本身的幻觉
- 已发生的已知问题：Orchestrator prompt 中写入过多指令导致跳过子 Agent 派发

### 4.2 设计

#### 4.2.1 Checklist

在 Reporter 的 Pre-Submit Gate 之前插入自检步骤。Reporter 必须用 Bash 工具执行检查脚本 `python scripts/ai_failure_check.py`：

| Mode | 检查内容 | 检测方法 |
|------|---------|---------|
| M1 | 脚本错误被误判为数据库缺陷 | 检查 execution_summary.txt 中 exit_code≠0 且并非 FAILED: 标记 |
| M2 | 编造文档引用（幻觉 URL） | curl 每个 source_url → 验证 HTTP 200 + 页面内容包含引用段落 |
| M3 | 编造执行结果数据 | 比对 defect-N.md 中的输出与 output_*.log 中的原始输出 |
| M4 | 走捷径跳过关键验证 | 检查 .done 标记是否全部存在；有缺陷但缺少 stage2_*.json.done → FAIL |
| M5 | 脚本 bug 被说成新发现 | 检查 FAILED: 输出是否匹配预期缺陷类型（Type1=expect 4xx got 2xx 等） |
| M6 | 编造方法论 | 检查 defect-N.md 中是否有不在 attack-*.md 中的测试策略描述 |
| M7 | 锁定早期错误假设 | 检查同一 endpoint 的缺陷是否在多个 round 中反复出现但从未确认 |

#### 4.2.2 阻断规则

```
任一 Mode 触发 → 缺陷标记 [AI_SELF_CHECK_FAILED]
  ├── M2/M3/M6 触发（数据造假）→ 直接丢弃该缺陷，不生成报告
  ├── M1/M5 触发（分类错误）→ 降级 confidence，回退到 Stage 2 重新判定
  ├── M4 触发（流程违规）→ 挂起，等待人工介入
  └── M7 触发（死循环）→ 挂起，等待人工介入
```

#### 4.2.3 实现

新增独立 Python 脚本 `scripts/ai_failure_check.py`：

```
输入: SESSION_DIR + defect_id
输出: JSON {checklist: [{mode, passed: bool, detail}], overall: PASS|FAIL|HALT}

M2 检查需要网络（curl source_url），其余都是纯本地文件验证。
M2 失败时降级为 WARN（网络问题可能是临时性的），不直接丢弃缺陷。
M2 每个 source_url 最多重试 2 次，间隔 3s。如果所有 URL 都不可达，
可能是网络问题 → 整体降级为 WARN 而非 FAIL。
```

#### 4.2.4 文件改动

| 文件 | 改动 |
|------|------|
| `agents/reporter.md` | Pre-Submit Gate 之前增加 7-mode 自检步骤 |
| `scripts/ai_failure_check.py` | 新建脚本 |
| `skills/defect-taxonomy/SKILL.md` | 增加 7-mode checklist 参考 |

---

## 5. P2: Material Passport（契约版本化）

### 5.1 现状问题

- `structured_contract.json` 无完整性保护，LLM 可能在后续步骤中不经意修改
- 缓存 TTL 依赖文件系统时间戳，缺少内容级别的缓存验证
- 无法追溯契约是由哪个 Agent 在什么时间生成的

### 5.2 设计

#### 5.2.1 Passport Schema

在 `structured_contract.json` 顶层增加 `_passport` 字段：

```json
{
  "_passport": {
    "schema_version": "2.0",
    "contract_hash": "sha256:abc123def456...",
    "contract_hash_algorithm": "sha256",
    "source": {
      "doc_urls": ["https://milvus.io/api-reference/..."],
      "doc_version": "v2.6.x",
      "crawl_method": "crawl4ai",
      "crawled_at": "2026-06-07T14:00:00Z"
    },
    "generation": {
      "knowledge_extractor_agent": "testvdb:knowledge-extractor",
      "contract_formalizer_agent": "testvdb:contract-formalizer",
      "generated_at": "2026-06-07T14:05:00Z",
      "cache_ttl_hours": 168
    },
    "integrity": {
      "verified": true,
      "verified_at": "2026-06-07T14:05:00Z",
      "core_crud_coverage_pct": 95.0,
      "endpoint_count": 12,
      "constraint_count": 85
    }
  },
  "api_endpoints": { ... },
  "constraints": { ... }
}
```

#### 5.2.2 Hash 计算规则

```
hash_input = 排除 _passport 字段后的 JSON（按 key 排序，无空格）
hash = sha256(hash_input)
```

验证：每次读取 `structured_contract.json` 时，重新计算 hash 与 `_passport.contract_hash` 比对。不一致 → 标记 `PASSPORT_TAMPERED` → 拒绝使用 → 强制重新生成。

#### 5.2.3 改动点

- `contract-formalizer`：生成契约后计算 hash 并写入 `_passport`
- `mine.md` Step 6（合同门控检查）：增加 hash 验证
- `mine.md` Step 3（缓存检查）：增加 hash 验证作为 TTL 的补充

#### 5.2.4 文件改动

| 文件 | 改动 |
|------|------|
| `agents/contract-formalizer.md` | 增加 _passport 生成步骤 |
| `commands/mine.md` | Step 3, Step 6 增加 hash 验证 |
| `skills/contract-schema/SKILL.md` | 增加 _passport schema |

---

## 6. P2: data_access_level（Agent 数据权限标记）

### 6.1 设计

在每个 Agent 定义的 frontmatter 中增加 `dataAccess` 字段：

| Agent | dataAccess | 可访问 | 不可访问 |
|-------|-----------|--------|---------|
| knowledge-extractor | `raw` | 网络（WebFetch/Crawl4AI） | 其他 Agent 产出 |
| contract-formalizer | `raw` | raw_knowledge.md | 网络 |
| attack-boundary | `redacted` | structured_contract.json, strategy_registry | 网络、执行结果 |
| attack-state | `redacted` | 同上 | 同上 |
| attack-semantic | `redacted` | 同上 | 同上 |
| docker-executor | `redacted` | 脚本文件 | 网络、契约文件 |
| judge-doc | `verified_only` | 执行结果, contract | 网络 |
| judge-evidence | `verified_only` | 执行结果 | 网络、契约 |
| judge-novelty | `verified_only` | 执行结果, GitHub MCP | 契约 |
| judge-severity | `verified_only` | 执行结果 | 网络、契约 |
| reporter | `verified_only` | Judge 结果, 执行日志 | 网络 |
| orchestrator | `redacted` | 所有 Agent 产出 | 网络（爬取由子 Agent 完成） |

### 6.2 实现

- **文档约定，非运行时强制**：在 Agent prompt 中加入 `data_access_level` 约束指令
- 每个 Agent 的 prompt 开头增加：
  ```
  ## 数据访问级别: {raw|redacted|verified_only}
  你只能访问 {allowed_sources}。如果你需要访问 {forbidden_sources}，
  请告知 Orchestrator 并由对应权限的 Agent 处理。
  ```
- 无新代码，纯 prompt 工程

### 6.3 文件改动

| 文件 | 改动 |
|------|------|
| 所有 `agents/*.md` | frontmatter 增加 `dataAccess`；prompt 增加数据访问约束指令 |

---

## 7. P3: Marketplace 分发优化

### 7.1 改动

- `README_zh.md`：在一键安装部分增加 marketplace 命令
- `AGENTS.md`：增加安装说明
- 更新 `plugin.json` version 为 `2.0.0`

### 7.2 安装命令

```bash
# 方式 1: Marketplace（推荐）
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB

# 方式 2: 本地开发
git clone https://github.com/yihui504/TestVDB.git
claude --plugin-dir TestVDB
```

---

## 8. 配置变更

### 8.1 settings.json 新增字段

```json
{
  "evolution": {
    "enabled": true,
    "strategy_registry_dir": "strategy_registry",
    "max_strategies_per_injection": 10,
    "min_confidence_for_injection": 0.6,
    "auto_deprecate_after_failures": 3
  },
  "fan_out": {
    "enabled": true,
    "seeds_per_agent": 3,
    "profiles": ["priority_first", "coverage_gap", "rejection_pattern"]
  },
  "ai_failure_check": {
    "enabled": true,
    "halt_on": ["M4", "M7"],
    "reject_on": ["M2", "M3", "M6"],
    "rewind_on": ["M1", "M5"]
  },
  "material_passport": {
    "enabled": true,
    "hash_algorithm": "sha256",
    "reject_on_tamper": true
  }
}
```

---

## 9. 向后兼容

- 所有新功能通过 `settings.json` 中的 feature flag 控制
- 默认全部启用，但可单独关闭回退到 v1.x 行为
- `strategy_registry/` 目录不存在时，策略注入静默跳过
- `_passport` 字段缺失时（旧契约文件），跳过 hash 验证但输出警告

---

## 10. 测试计划

| 测试项 | 方法 | 验收标准 |
|--------|------|---------|
| 策略提取 | 运行一次完整 mining → 检查 strategy_registry/ 是否有新条目 | registry 中有本次 confirmed_defects 的策略 |
| 策略注入 | 第二次 mining → 检查 Attack Agent 产出的脚本是否包含上次策略模式 | 出现 ≥1 个跨会话策略的脚本 |
| 策略迁移 | Milvus mining → 检查 qdrant_strategies.json 是否有 applicable_dbs 包含 qdrant | 泛化逻辑正确标记 |
| Fan-Out | mining → 检查脚本数量是否为 3 的倍数 | 脚本数 ≥ 9（最少 9 seed 各 1 脚本） |
| Fan-Out 去重 | 同一 endpoint+constraint 被多个 seed 覆盖 → 检查最终脚本数 | 去重后 < seed 总数 |
| 7-mode M2 | 故意给一个假 source_url → Reporter 应标记 FAIL | curl 404 → M2 触发 → 标记 |
| 7-mode M4 | 删除一个 .done 标记 → Reporter 应挂起 | 缺失 .done → HALT |
| Passport hash | 手工修改 contract → 读取时应报 PASSPORT_TAMPERED | hash 不匹配 → 拒绝 |
| 配置开关 | 关闭 evolution.enabled → 无策略注入 | 行为与 v1.x 一致 |
