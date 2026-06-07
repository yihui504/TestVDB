# Plan: TestVDB 审查流水线端到端文档感知重设计

## Overview
重新设计 TestVDB 插件的审查流水线，从 Knowledge Extractor 到 Reporter 建立端到端文档感知机制。新增 judge-doc Agent，修改 5 个现有 Agent prompt，确保 source_url/doc_version 全链路保留，4-Judge 加权投票，以证据链为核心的报告格式。

## Dependency Graph
```
Step 1 (KE+Contract 信息流修复)
    ↓
Step 2 (Attack Agent 传递 source_url) ──并行── Step 3 (新建 judge-doc.md)
    ↓                                           ↓
Step 4 (修改 judge-evidence/novelty/severity 支持权重调节) ←──依赖 Step 3 的 3 级定义
    ↓
Step 5 (修改 Reporter 证据链报告格式)
    ↓
Step 6 (修改 Orchestrator 调度 4-Judge)
    ↓
Step 7 (注册新 Agent + 代码审查验证)
```

## Parallelism Summary
- Serial steps: 5 (Step 1 → 4 → 5 → 6 → 7)
- Parallel groups: 1 (Step 2 ∥ Step 3)
- Total steps: 7

---

## Step 1: 修复 KE → Contract 信息流（source_url/doc_version 保留）

### Context Brief
根因分析确认：raw_knowledge.md 中的 source_url 以非结构化 Markdown 引用块存在，Contract Formalizer 转换时完全丢弃。需要：
1. 修改 knowledge-extractor.md，将 Sources 从 Markdown 引用块改为结构化 `sources` 数组
2. 修改 contract-formalizer.md，在 structured_contract.json 中新增 `endpoint_registry` 和每个 constraint/assertion 的 `source_url`/`doc_version` 字段

### Tasks
- [ ] 修改 `agents/knowledge-extractor.md` Step 5 的输出格式：将 `> Sources:` 引用块改为结构化 `sources` 数组（每个 source 含 url, doc_version, fetched_at, version_match）
- [ ] 修改 `agents/contract-formalizer.md`：在 structured_contract.json 的顶层新增 `endpoint_registry` 数组（每个条目含 path, method, source_url, doc_version, doc_quote, verified_at）
- [ ] 修改 `agents/contract-formalizer.md`：在每个 constraint 和 assertion 中新增 `source_url` 和 `doc_version` 必填字段
- [ ] 修改 `agents/contract-formalizer.md`：在输出验证 10 项自检中增加"每个 constraint/assertion 都有 source_url"的检查

### Verification
- 读取修改后的 knowledge-extractor.md，确认 Step 5 输出格式包含结构化 sources 数组
- 读取修改后的 contract-formalizer.md，确认 endpoint_registry 和 source_url/doc_version 字段存在

### Exit Criteria
- knowledge-extractor.md 的 raw_knowledge.md 模板包含结构化 sources 数组
- contract-formalizer.md 的 structured_contract.json schema 包含 endpoint_registry 和每个 constraint/assertion 的 source_url/doc_version

### Rollback
git checkout 修改的两个文件

---

## Step 2: 修改 Attack Agent 传递 source_url

### Context Brief
Attack Agent 产出的候选缺陷需要引用 source_url，以便下游 Judge 和 Reporter 使用。当前 Attack Agent 只引用 constraint_id，不引用 source_url。

### Tasks
- [ ] 修改 `agents/attack-boundary.md`：在输出模板中增加 `source_url` 和 `doc_version` 字段，从 structured_contract.json 的 constraint 中获取
- [ ] 修改 `agents/attack-state.md`：同上
- [ ] 修改 `agents/attack-semantic.md`：同上

### Verification
- 读取修改后的 3 个 Attack Agent prompt，确认输出模板包含 source_url 和 doc_version

### Exit Criteria
- 3 个 Attack Agent 的输出模板都包含 source_url 和 doc_version 字段

### Rollback
git checkout 修改的三个文件

---

## Step 3: 新建 judge-doc.md（文档契约验证 Agent）

### Context Brief
新增第 4 个 Judge Agent，专门负责文档契约验证。这是审查流水线重设计的核心新增组件。judge-doc 需要做到四层验证：链接可达 + 版本匹配 + 内容一致性 + 端点路径精确性。验证方式为双重验证：查端点注册表 + 联网实时验证。

### Tasks
- [ ] 创建 `agents/judge-doc.md`，包含以下内容：
  - 角色：文档契约验证 Judge
  - 输入：候选缺陷列表 + structured_contract.json（含 endpoint_registry）+ raw_knowledge.md
  - 四层验证流程：
    1. **链接可达性**：对每个缺陷引用的 source_url 执行 curl，验证 HTTP 200/301/302
    2. **版本匹配**：提取文档页面版本号，与目标版本 major.minor 宽松匹配
    3. **内容一致性**：联网抓取文档内容，验证缺陷描述的"预期行为"与文档一致（如不能把 SDK 功能误认为 REST API 功能）
    4. **端点路径精确性**：查 endpoint_registry + 联网验证缺陷引用的端点路径是否在文档中实际存在
  - 双重验证机制：先查 endpoint_registry（快速），查表失败则联网补充，联网失败则降级为查表结果
  - 输出 3 级结果：DOC_VERIFIED / DOC_PARTIAL / DOC_MISMATCH
  - 输出文件：`${SESSION_DIR}/judge_doc_{defect_id}.json` + `${SESSION_DIR}/debate_logs/stage2_doc.json`
  - 强制 Write 输出指令

### Verification
- 读取新建的 judge-doc.md，确认包含四层验证、双重验证机制、3 级输出、强制 Write 输出

### Exit Criteria
- judge-doc.md 文件存在且包含完整的四层验证流程
- 输出格式包含 DOC_VERIFIED/PARTIAL/MISMATCH 三级
- 包含强制 Write 输出指令

### Rollback
删除 judge-doc.md

---

## Step 4: 修改现有 3 个 Judge 支持权重调节

### Context Brief
judge-doc 的文档验证结果（DOC_VERIFIED/PARTIAL/MISMATCH）需要调节其他 3 个 Judge 的审查严格度。需要修改 judge-evidence/novelty/severity 的 prompt，让它们读取 judge-doc 的输出并据此调整审查标准。

### Tasks
- [ ] 修改 `agents/judge-evidence.md`：
  - 增加 Step 0：读取 `${SESSION_DIR}/debate_logs/stage2_doc.json`，获取每个缺陷的 doc_verification_result
  - 根据 doc_verification_result 调节审查严格度：
    - DOC_VERIFIED：正常审查流程
    - DOC_PARTIAL：evidence 需达到 A 级（11-13 分）而非 B 级（8-10 分）
    - DOC_MISMATCH：需 2 次独立复现 + 源码验证 + 排除行业惯例 + evidence_score 上限降为 7 分
  - 在投票字段中增加 `doc_verification_result` 字段
- [ ] 修改 `agents/judge-novelty.md`：
  - 增加 Step 0：读取 stage2_doc.json
  - DOC_MISMATCH 时需额外搜索 GitHub Discussions/StackOverflow 确认是否为已知行为
  - 在投票字段中增加 `doc_verification_result` 字段
- [ ] 修改 `agents/judge-severity.md`：
  - 增加 Step 0：读取 stage2_doc.json
  - DOC_MISMATCH 时严重性自动降一级（如 HIGH → MEDIUM）
  - 在投票字段中增加 `doc_verification_result` 字段

### Verification
- 读取修改后的 3 个 Judge prompt，确认包含 doc_verification_result 读取和权重调节逻辑

### Exit Criteria
- 3 个 Judge prompt 都包含读取 stage2_doc.json 的步骤
- 3 个 Judge prompt 都包含根据 DOC_VERIFIED/PARTIAL/MISMATCH 调节审查严格度的逻辑
- 投票字段都包含 doc_verification_result

### Rollback
git checkout 修改的三个文件

---

## Step 5: 修改 Reporter 证据链报告格式

### Context Brief
当前 defect-N.md 格式以 Metadata + Description + Steps + Expected + Impact + Evidence 组织，没有 Ring 1/Ring 2 的位置。需要以证据链为核心重新设计报告格式。

### Tasks
- [ ] 修改 `agents/reporter.md`：
  - 重新设计 defect-N.md 模板，以三环证据链为核心结构：
    ```
    # Defect {N}: {Title}

    ## Metadata
    - Defect ID, Type, Severity, Endpoint, Discovered

    ## Evidence Chain

    ### Ring 1: Contract Clause (契约条款引用)
    - constraint_id: ...
    - contract_assertion: ...
    - expected_behavior: ...
    - source_url: ...

    ### Ring 2: Document Reference (原始文档引用)
    - source_url: ... (必须可达)
    - doc_version: ... (必须匹配 major.minor)
    - doc_quote: ... (文档原文引用)
    - url_status: verified/degraded/unreachable
    - version_match: matched/mismatched

    ### Ring 3: Actual Behavior (实际行为证据)
    - HTTP Request: ...
    - HTTP Response: ...
    - Container Logs: ...
    - reproduced_at: ...

    ### Ring 4: Source Code Reference (可选)
    - github_url: ...
    - code_snippet: ...

    ## Completeness Check
    - Ring 1: PRESENT/MISSING
    - Ring 2: PRESENT/MISSING
    - Ring 3: PRESENT/MISSING
    - Overall: COMPLETE/INCOMPLETE_EVIDENCE

    ## Reproduction Steps
    ...

    ## Impact Analysis
    ...
    ```
  - 修改完整性检查逻辑：Ring 1 + Ring 2 + Ring 3 必须全部 PRESENT，否则标记 INCOMPLETE_EVIDENCE 不生成报告
  - 修改 Pre-Submit Gate：增加 source_url 可达性验证（curl 检查）
  - 修改 summary.md 模板：增加证据链完整性统计

### Verification
- 读取修改后的 reporter.md，确认 defect-N.md 模板以证据链为核心
- 确认完整性检查逻辑包含 Ring 1/2/3 三环验证

### Exit Criteria
- defect-N.md 模板包含 Ring 1/2/3/4 结构
- 完整性检查要求 Ring 1+2+3 全部 PRESENT
- Pre-Submit Gate 包含 source_url 可达性验证

### Rollback
git checkout reporter.md

---

## Step 6: 修改 Orchestrator 调度 4-Judge

### Context Brief
Orchestrator 当前调度 3 个 Judge（evidence + novelty + severity），需要改为 4 个 Judge（+ doc），并更新辩论 Stage 2 的投票逻辑。

### Tasks
- [ ] 修改 `agents/orchestrator.md`：
  - Step 8e 辩论 Stage 2：从 3 个 Judge 改为 4 个 Judge（增加 judge-doc）
  - 更新加权投票逻辑：
    - judge-doc 先执行（产出 DOC_VERIFIED/PARTIAL/MISMATCH）
    - 其他 3 个 Judge 读取 judge-doc 结果后执行（受权重调节）
    - 最终投票：evidence AND severity 都投 is_defect 才确认缺陷，novelty 仅标记元数据，doc 结果作为权重调节器
  - 更新 pipeline_state.json 的 Judge 列表
  - 更新 PreCompact/PostCompact 状态保存逻辑

### Verification
- 读取修改后的 orchestrator.md，确认 Step 8e 包含 4 个 Judge 调度
- 确认投票逻辑包含 judge-doc 权重调节

### Exit Criteria
- orchestrator.md 的 Step 8e 包含 4 个 Judge 调度
- 投票逻辑包含 judge-doc → 其他 Judge 的权重调节流程
- pipeline_state.json 包含 judge-doc 状态

### Rollback
git checkout orchestrator.md

---

## Step 7: 注册新 Agent + 代码审查验证

### Context Brief
需要在 plugin.json 中注册新的 judge-doc Agent，并通过代码审查验证所有修改是否满足 9 项验收标准。

### Tasks
- [ ] 修改 `.claude-plugin/plugin.json`：在 agents 数组中增加 `"./agents/judge-doc.md"`
- [ ] 代码审查验证（逐项检查 AC1-AC9）：
  - AC1: reporter.md 的 defect-N.md 模板包含 Ring 1+2+3，完整性检查要求三环全部 PRESENT
  - AC2: judge-doc.md 包含链接可达性验证（curl HTTP 200/301/302）
  - AC3: judge-doc.md 包含版本匹配验证（major.minor 宽松匹配）
  - AC4: judge-doc.md 包含内容一致性验证（联网抓取文档对比预期行为）
  - AC5: judge-doc.md 包含端点路径精确性验证（查表+联网）
  - AC6: KE→Contract→Attack→Judge→Reporter 全链路 source_url/doc_version 保留
  - AC7: judge-doc.md 包含双重验证机制（查表+联网）
  - AC8: 3 个 Judge prompt 包含权重调节逻辑（DOC_VERIFIED/PARTIAL/MISMATCH）
  - AC9: 代码审查确认所有 prompt 包含必要验证步骤

### Verification
- 读取 plugin.json 确认 judge-doc.md 已注册
- 逐项检查 AC1-AC9

### Exit Criteria
- plugin.json 包含 judge-doc.md
- AC1-AC9 全部通过代码审查

### Rollback
git checkout plugin.json
