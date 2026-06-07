[English](./README.md) | 中文

# TestVDB

基于 LLM 的向量数据库自动化缺陷挖掘工具，以 Claude Code 插件形式运行，通过自然语言契约逆向工程与多 Agent 辩论机制，在 Docker 沙箱中自动发现 Milvus、Qdrant、Weaviate、pgvector 的合规性缺陷。

---

## 目录

- [项目概述](#项目概述)
- [核心理论](#核心理论)
- [架构设计](#架构设计)
- [快速开始](#快速开始)
- [目录结构](#目录结构)
- [环境要求](#环境要求)
- [使用方法](#使用方法)
- [配置说明](#配置说明)
- [缺陷分类体系](#缺陷分类体系)
- [流水线流程](#流水线流程)
- [辩论机制](#辩论机制)
- [输出产物](#输出产物)
- [Rust 实现](#rust-实现)
- [许可证](#许可证)

---

## 项目概述

TestVDB 是面向向量数据库的自动化缺陷挖掘工具，旨在解决传统数据库测试在面对复杂向量数据库时缺乏业务语义理解、以及大模型自动化测试极易产生"幻觉"的痛点。

**核心能力：**

- 从官方文档自动提取结构化契约（自然语言契约逆向工程）
- 基于契约自动生成针对性攻击测试脚本
- Docker 沙箱隔离执行，确保安全可控
- 多 Agent 辩论机制过滤假阳性，保证缺陷可信度
- 三环证据链支撑，缺陷报告可追溯、可复现

**支持目标：**

| 目标数据库 | 协议 | Docker 复杂度 |
|-----------|------|--------------|
| Milvus | gRPC / REST | 高（etcd + MinIO + standalone） |
| Qdrant | REST / gRPC | 低（单容器） |
| Weaviate | REST / gRPC | 低（单容器） |
| pgvector | SQL | 低（单容器） |

---

## 核心理论

TestVDB 的理论基础详见 [THEORETICAL_FRAMEWORK.md](./THEORETICAL_FRAMEWORK.md)，核心贡献包含两大支柱：

### 自然语言契约逆向工程

抛弃传统的随机 Fuzzing 或硬编码断言，利用 LLM 的阅读理解能力，将官方文档中非结构化的自然语言规则逆向提取为高度结构化、机器可执行的 JSON 契约。这一范式转移使测试用例的生成真正基于官方定义的语义，并实现了"提取"与"执行"的两步走解耦。

### 人机协同防幻觉沙箱模型

大模型在遇到测试脚本报错时，往往通过"篡改正确的断言"来迎合错误的系统返回，产生假阴性。TestVDB 提出严格的隔离与门控架构：

- **物理隔离**：所有生成的代码仅在 Docker 沙箱中运行
- **逻辑隔离**：通过四型缺陷分类法作为 Gatekeeper，命中真实缺陷时强制阻断 LLM 的自我修复权限
- **人机协同**：重试超过阈值时系统主动挂起，请求人类工程师介入

---

## 架构设计

### Agent 体系（12 个 Agent）

| Agent | 职责 |
|-------|------|
| orchestrator | 主编排器，协调全部子 Agent 完成流水线 |
| knowledge-extractor | 从官方文档提取 API 知识 |
| contract-formalizer | 将原始知识形式化为结构化契约 |
| attack-boundary | 边界值攻击，测试参数边界约束 |
| attack-state | 状态攻击，测试状态一致性和逻辑违规 |
| attack-semantic | 语义攻击，测试语义层面的合规性 |
| docker-executor | 在 Docker 沙箱中执行攻击脚本 |
| judge-doc | 文档审查，验证候选缺陷的文档引用有效性与内容一致性 |
| judge-evidence | 证据审查，判定缺陷证据可信度 |
| judge-novelty | 新颖性审查，判定缺陷是否为已知问题 |
| judge-severity | 严重性评估，判定缺陷影响等级 |
| reporter | 生成缺陷报告和汇总文档 |

### Skill 体系（4 个 Skill）

| Skill | 用途 |
|-------|------|
| pipeline | 缺陷挖掘流水线 SOP，定义六阶段执行流程 |
| contract-schema | 结构化契约 JSON Schema 参考 |
| defect-taxonomy | 四型缺陷分类法参考 |
| docker-templates | Docker 容器模板参考 |

### 数据流

```
Orchestrator
  |
  +--> Knowledge Extractor --> raw_knowledge.md
  |                                    |
  +--> Contract Formalizer <-----------+
  |         |
  |         v
  |   structured_contract.json
  |         |
  +--> Attack Trio (并发) <-- contract + reflection_context
  |   boundary | state | semantic
  |         |
  |         v
  |   test_scripts[] + debate_log_stage1.json
  |         |
  +--> Executor (并发) <-- test_scripts[]
  |         |
  |         v
  |   execution_results[]
  |         |
  +--> Judge Quartet (并发) <-- execution_results[]
  |   doc (先行，权重调节) | evidence | novelty | severity
  |         |
  |         v
  |   confirmed_defects[] + debate_log_stage2.json
  |         |
  +--> Reporter --> defect-N.md + summary.md
```

---

## 安装

### 方式 1: Marketplace（推荐）
```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

### 方式 2: 本地开发
```bash
git clone https://github.com/yihui504/TestVDB.git
claude --plugin-dir TestVDB
```

## 快速开始

### 1. 安装 Claude Code CLI

```bash
npm install -g @anthropic-ai/claude-code
```

### 2. 克隆项目

```bash
git clone https://github.com/yihui504/TestVDB.git
cd TestVDB
```

### 3. 启动挖掘

```bash
claude --plugin-dir .
```

> **注意**：`--plugin-dir .` 仅在当前会话加载插件（适用于开发/测试）。如需永久安装，参见[在 Claude Code 上测试](#在-claude-code-上测试)。

然后在 Claude Code 会话中使用 `/mine` 命令：

```
/testvdb:mine milvus v2.6.17
/testvdb:mine qdrant v1.13.0 --max-rounds 3
/testvdb:mine weaviate 1.25.0 --min-defects 2
/testvdb:mine pgvector pg17 --max-rounds 0
```

---

## 目录结构

```
TestVDB/
  .claude-plugin/plugin.json       插件清单
  .mcp.json                        MCP 服务器配置（GitHub API）
  agents/                          12 个 Agent 定义
    orchestrator.md
    knowledge-extractor.md
    contract-formalizer.md
    attack-boundary.md
    attack-state.md
    attack-semantic.md
    docker-executor.md
    judge-evidence.md
    judge-novelty.md
    judge-severity.md
    judge-doc.md
    reporter.md
  commands/mine.md                 入口命令
  docker/                          Docker Compose 模板
    crawl4ai.yml
    milvus.yml
    qdrant.yml
    weaviate.yml
    pgvector.yml
  hooks/hooks.json                  生命周期钩子
  skills/                          4 个 Skill 定义
    pipeline/SKILL.md
    contract-schema/SKILL.md
    defect-taxonomy/SKILL.md
    docker-templates/SKILL.md
  contracts/                        配置与种子数据
    milvus_contract.json           预爬取 Milvus 文档
    weaviate_contract.json         预爬取 Weaviate 文档
    pgvector_contract.json         预爬取 pgvector 文档
    db_strategies.json              Per-DB 集中化策略配置
    settings_schema.json            配置验证 Schema
  issues/                          已发现缺陷报告
    00-summary.md
    001-*.md ... 007-*.md
    milvus_*.md
    qdrant_*.md
    weaviate_*.md
  scripts/                         辅助脚本
    crawl_fetch.py                 Crawl4AI 网页抓取器（主方案）
    hook_runner.py                跨平台 Python 解释器解析器
    verify_defects.py
    github_search.py
    prioritizer.py
    developer_attitude.py
    verify/                        缺陷验证脚本
      verify_defect3.py
      verify_defect4.py
      verify_extra.py
      verify_extra2.py
      verify_p0b_extended.py
      verify_remaining.py
    cleanup_stop.py                会话清理
    emergency_cleanup.py           紧急容器清理
    log_execution.py               执行日志记录
    notify_check.py                通知配置检查
    postcompact_verify.py          压缩后状态恢复
    precompact_save.py             压缩前状态保存
    preflight.py                   会话预检
    retry_policy.py                重试策略报告
  settings.json                    26 个可配置参数
  THEORETICAL_FRAMEWORK.md         理论框架论文
```

---

## 环境要求

| 依赖 | 最低版本 | 说明 |
|------|---------|------|
| **LLM 模型** | Claude Sonnet/Opus | 通过 Claude Code 运行。 |
| Claude Code CLI | 最新 | `npm install -g @anthropic-ai/claude-code` |
| Docker Engine | 20.10+ | 运行中，用于沙箱隔离 |
| Python | 3.9+ | 低于 3.9 为致命错误，流水线将终止 |
| 磁盘空间 | 10GB+ | Docker 镜像与结果存储 |
| Docker Hub Token | -- | **必须**。设置 `DOCKER_HUB_TOKEN` 环境变量。未认证请求被严格限流。通过 `echo $TOKEN \| docker login --username $USER --password-stdin` 获取 |
| 网络访问 | -- | WebFetch 必须能访问目标文档站点（milvus.io、qdrant.tech 等）。企业代理需白名单这些域名。 |
| GitHub Token | -- | 可选，用于新颖性判定（无则降级为 WebSearch） |

---

## 在 Claude Code 上测试

### 方式 1：会话内加载（推荐开发时使用）

```bash
cd TestVDB
claude --plugin-dir .
```

仅在当前会话加载插件，修改文件后重启会话即可生效。

### 方式 2：永久本地安装

```bash
# 将插件目录添加为本地 marketplace
/plugin marketplace add /path/to/TestVDB

# 安装插件
/plugin install testvdb@TestVDB

# 验证安装
/help
# 应看到 /testvdb:mine 命令
```

### 方式 3：从 GitHub 安装

```bash
/plugin marketplace add yihui504/TestVDB
/plugin install testvdb@yihui504-TestVDB
```

### 调试

```bash
# 启用 debug 模式查看插件加载详情
claude --plugin-dir . --debug

# 查看已加载的 agents
/agents

# 查看可用命令
/help
```

### 测试流水线

1. 确保 Docker Engine 正在运行：`docker info`
2. 启动会话：`claude --plugin-dir .`
3. 用 Qdrant 快速测试（最简单，单容器）：
   ```
   /testvdb:mine qdrant v1.13.0 --max-rounds 1
   ```
4. 检查结果：`results/qdrant/v1.13.0/<timestamp>/`

---

## 使用方法

### 命令格式

```
/testvdb:mine <db> <version> [--max-rounds N] [--min-defects N]
```

### 参数说明

| 参数 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `<db>` | 是 | -- | 目标数据库：`milvus`、`qdrant`、`weaviate`、`pgvector` |
| `<version>` | 是 | -- | 目标版本号（如 `v2.6.17`、`v1.13.0`、`pg17`） |
| `--max-rounds N` | 否 | 5 | 最大挖掘轮数，设为 0 表示无上限 |
| `--min-defects N` | 否 | 1 | 最低缺陷产出要求，达到后可提前终止 |

### 终止条件

以下任一条件满足即终止挖掘循环：

1. **僵局**：连续 5 轮无新缺陷
2. **覆盖率**：契约覆盖率达到 95% 以上
3. **最大轮数**：达到 `--max-rounds` 设定值
4. **最低缺陷**：达到 `--min-defects` 设定值

### 多数据库并行

开多个终端窗口即可并行挖掘不同数据库：

```bash
# 终端 1
/testvdb:mine milvus v2.6.17
# 终端 2
/testvdb:mine qdrant v1.13.0
```

### 错误恢复

如果会话中断，重新执行相同命令即可恢复。系统会自动检测未完成的会话并从断点继续。

---

## 配置说明

### settings.json

主配置文件，包含以下可配置参数：

| 分组 | 参数 | 默认值 | 说明 |
|------|------|--------|------|
| docker | cleanup_on_exit | true | 会话结束时自动清理容器 |
| docker | startup_timeout_seconds | 120 | 容器启动超时时间 |
| github | token | "" | GitHub PAT，用于新颖性判定 |
| retry | max_attempts | 5 | 最大重试次数 |
| retry | docker_startup_delay_seconds | 10 | Docker 启动重试间隔 |
| retry | script_execution_delay_seconds | 3 | 脚本执行重试间隔 |
| retry | doc_fetch_delay_seconds | 5 | 文档抓取重试间隔 |
| pipeline | default_max_rounds | 5 | 默认最大挖掘轮数 |
| pipeline | default_min_defects | 1 | 默认最低缺陷数 |
| results | base_dir | "results" | 结果输出目录 |
| results | max_sessions | 10 | 最大会话保留数 |
| knowledge | cache_enabled | true | 是否启用知识缓存 |
| knowledge | cache_ttl_hours | 168 | 缓存有效期（小时） |
| notification | on_severity | "critical" | 触发通知的严重性等级 |
| notification | webhook_url | "" | Webhook 通知地址 |
| network | proxy | "" | 网络代理地址 |
| -- | log_level | "info" | 日志级别 |

### .mcp.json

MCP 服务器配置，当前配置了 GitHub MCP 服务器，用于 Judge Novelty Agent 搜索已知缺陷以判定新颖性。需设置 `GITHUB_TOKEN` 环境变量。

---

## 缺陷分类体系

TestVDB 采用四型缺陷分类法（MECE 原则），确保缺陷判定的客观性和一致性：

### Type 1: Illegal Success（非法操作成功）

违反文档约束的输入被数据库接受，返回成功状态（2xx）而非错误（4xx）。

**检测模式**：expect 4xx -> got 2xx

**示例**：`limit=-1` 返回 200 OK；缺失必需参数 `vector` 返回 200 空结果

### Type 2: Poor Diagnostics（诊断信息不足）

数据库正确拒绝了错误输入，但错误消息不够清晰。

**诊断质量评分**（3 分制）：参数名被提及（1 分）+ 正确格式/范围被说明（1 分）+ 可操作的修复建议（1 分）。低于 2 分即为 Type 2 缺陷。

### Type 3: Runtime Failure（运行时失败）

合法输入导致数据库崩溃、500 错误或异常行为。

**示例**：合法搜索请求返回 500；特定维度导致容器 crash；并发操作死锁

### Type 4: State/Logic Violation（状态/逻辑违规）

API 正确返回，但数据状态或语义结果不一致。

**示例**：INSERT 3 行但 COUNT 返回 2；DELETE 后搜索仍返回数据；排序结果与向量距离不一致

### 分类决策树

```
1. 是合法输入被拒绝？ -> 是: 反向 Type 1
2. 是非法输入被接受？ -> 是: Type 1
3. 是合法输入导致崩溃/500？ -> 是: Type 3
4. 错误消息不清晰？ -> 是: Type 2
5. 状态/结果不一致？ -> 是: Type 4
6. 否则: 重新分类或非缺陷
```

---

## 流水线流程

### Phase 1: 知识获取

Knowledge Extractor Agent 使用 WebSearch 定位官方文档，WebFetch 抓取 API 参考页面，提取端点、参数、约束、SDK 版本和 Docker tags，产出 `raw_knowledge.md`。

### Phase 2: 契约形式化

Contract Formalizer Agent 读取原始知识，按 JSON Schema 转换为结构化契约，产出 `structured_contract.json`。通过合同门控检查（核心 CRUD 端点覆盖率不低于 90%）。

### Phase 3: 测试生成

Attack Trio（boundary + state + semantic）并发生成测试脚本。辩论 Stage 1 进行交叉同行评审投票，通过审查的脚本进入执行阶段。

### Phase 4: 沙箱执行

Docker Executor Agent 按 DB 选择 Docker 模板，启动容器、健康检查、安装依赖、执行脚本，收集结果和日志后清理容器。

### Phase 5: 缺陷判定

Judge Quartet（doc + evidence + novelty + severity）并发审查执行结果。judge-doc 先行执行作为权重调节器，产出 DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH 调节其他三个 Judge 的审查严格度。辩论 Stage 2 进行加权投票判定，确认缺陷存入候选列表。

### Phase 6: 报告生成

Reporter Agent 生成缺陷报告（defect-N.md）、自包含 MRE 脚本、汇总报告（summary.md）和会话元数据。

### 迭代与反思

每轮结束生成 `reflection_context`，注入下一轮 Attack Agents，指导策略调整。僵局检测触发时重新搜索文档并重新评估候选。

---

## 辩论机制

### Stage 1: 攻击脚本同行评审

三个 Attack Agent 交叉审查彼此生成的脚本（防止自评偏见）：

- Boundary 审查 State 和 Semantic 的脚本
- State 审查 Boundary 和 Semantic 的脚本
- Semantic 审查 Boundary 和 State 的脚本

**投票规则：**

| 投票结果 | 处理 |
|---------|------|
| 2/2 approve | 进入执行阶段 |
| 1 approve + 1 modify | Orchestrator 裁定（默认接受修改后进入） |
| 1 approve + 1 reject | Orchestrator 根据双方理由裁定 |
| 0/2 approve | 丢弃 |

### Stage 2: Judge Quartet 投票

四个 Judge Agent 独立审查全部执行结果：

- **judge-doc**：先行执行，验证文档引用有效性，产出 DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH 作为权重调节器
- **judge-evidence**：证据门控，证据等级 D 则自动判定为非缺陷
- **judge-severity**：严重性门控，severity = trivial 则判定为非缺陷
- **judge-novelty**：新颖性标记（new / new_similar / already_reported），永远投 is_defect，仅附加 novelty_rating 元数据，不参与缺陷确认投票

**缺陷确认规则：** evidence = is_defect AND severity = is_defect -> 确认缺陷。novelty_rating 附加到缺陷元数据，不影响确认状态，但影响 Reporter 中的提交优先级。

### 三环证据链

每个确认的缺陷必须包含完整的三环证据链：

1. **契约引用**：违反了哪条结构化契约约束
2. **来源 URL**：约束提取自哪个官方文档页面
3. **文档链接**：相关文档的永久链接（可选：源代码引用）

---

## 输出产物

```
results/{target}/{version}/{timestamp}/
├── defects/                    # 缺陷报告 (defect-1.md, defect-N.md)
├── mre/                        # 自包含 MRE 复现脚本
├── summary.md                  # 汇总报告
├── debate_logs/                # 辩论日志
│   ├── stage1.json             # Stage 1 攻击脚本评审
│   └── stage2.json             # Stage 2 Judge 投票
├── structured_contract.json    # 结构化契约
├── raw_knowledge.md            # 原始文档知识
├── mine_state.json             # 状态快照
├── coverage.json               # 覆盖率跟踪
├── session_metadata.json       # 会话元数据
└── experience_handoff.json     # 经验交接
```

---

## Rust 实现

Rust 实现已移至 `archive/rust-impl` 分支。该实现采用 Rust 2024 edition，基于 tokio 异步运行时，与 Claude Code 插件共享相同理论框架和缺陷分类体系，但独立运行。

访问归档代码：

```bash
git fetch origin archive/rust-impl
git checkout archive/rust-impl
```

---

## 许可证

[MIT](https://opensource.org/licenses/MIT)
