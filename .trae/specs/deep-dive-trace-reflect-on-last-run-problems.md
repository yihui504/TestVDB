# Deep-Dive Trace: 反思上次运行暴露的系统性问题

**日期**: 2026-05-24
**触发事件**: boundary策略运行产生60份报告，其中25份为flat key bug导致的假阳性
**方法**: 7条追踪车道并行证据收集 + 反驳轮

---

## 排位总览

| 排位 | 车道 | 假设 | 证据强度 | 因果角色 |
|------|------|------|----------|----------|
| **#1** | L3 | 成功指标度量了错误的东西 | **强** | 根因 |
| **#2** | L5 | 契约表示格式混淆语义 | **强** | 直接技术放大器 |
| **#3** | L1 | MRE完整性链在3处断裂 | **强** | L3的直接后果 |
| **#4** | L2 | 验证流水线缺少MRE有效性门 | **强** | L3的直接后果 |
| **#5** | L6 | AI辅助分析的自引用确认偏差 | **强** | 防线失效放大器 |
| **#6** | L4 | 黄金输出测试保护bug而非正确性 | **中强** | 修复阻力 |
| **#7** | L7 | 规格歧义容忍度被测试消除 | **中强** | L4的弱化版本 |

---

## 反驳轮：H1 vs H2

### H1（主导假设）：成功指标度量了错误的东西

**核心论点**：系统将 `HTTP 200` 等同于"参数生效"，这是25/60假阳性的根本原因。即使flat key bug不存在，这个判定逻辑仍然会产生假阳性——例如服务器静默忽略未知参数返回200时。

**支持证据**：
- `probe.rs:134`：`if r.status_code == 200: print(f'[DEFECT: ILLEGAL_SUCCESS]')` — 100%的自动probe使用此判定
- `executor.rs:270-274`：`found_defect = output.contains("[DEFECT:")` — 执行器只做字符串匹配
- `classifier.rs:134-151`：`detect_defect_type` 只做标记文本匹配
- `verification.rs:27-41`：双重复现只检查"同一脚本是否输出相同DEFECT标记"
- `generator.rs:103-128`：报告门控只检查占位符和文本格式

**H2对H1的反驳**：
> "HTTP 200判定本身不是bug——对于边界值测试，如果服务器接受了非法值（如shard_number=-1）并返回200，那确实是ILLEGAL_SUCCESS。问题出在flat key bug让参数根本没到达服务器，而不是判定逻辑有错。"

**H1的回应**：
> 反驳部分成立。对于**真正的**非法值（如shard_number=-1），HTTP 200判定是正确的。但问题在于：系统没有任何机制区分"服务器接受了非法值"和"服务器忽略了无效参数"。前者是真defect，后者是假阳性。flat key bug只是让这个区分问题暴露了——即使没有flat key bug，当Qdrant静默忽略未知JSON字段时，同样的判定逻辑也会产生假阳性。因此H1仍然成立，只是其影响范围需要限定为"静默忽略场景"。

### H2（最强替代假设）：契约表示格式混淆语义

**核心论点**：`create_collection.optimizers_config.indexing_threshold` 同时承载endpoint路由、JSON嵌套路径、参数实体三种语义，`strip_endpoint_prefix` 的硬编码前缀列表是唯一区分机制。这是flat key bug的直接技术根因。

**支持证据**：
- `qdrant_contract.json`：assertions中使用点分路径，三种语义无区分
- `probe.rs:70-81`：`strip_endpoint_prefix` 硬编码6个前缀，新增endpoint静默失败
- `probe.rs:57-68`：`dot_to_nested_json` 无法区分点号的语义角色
- `probe.rs:162-177`：`create_probe` 中短名称和带前缀名称混用同一入口
- `contract_loader.rs:67-69`：参数匹配使用模糊启发式 `llm.ends_with()`

**H1对H2的反驳**：
> "即使契约格式完美（如用结构化类型替代点分路径），如果判定逻辑仍然是HTTP 200 = ILLEGAL_SUCCESS，系统仍然无法检测'静默忽略'场景。契约格式问题只是让flat key bug成为可能，但不是所有假阳性的根因。"

**H2的回应**：
> 反驳成立。契约格式问题是flat key bug的**直接技术放大器**，但不是所有假阳性的根因。即使修复了契约格式，仍需要语义级判定逻辑来检测静默忽略场景。H2的因果角色应定位为"放大器"而非"根因"。

### 反驳轮结论

**H1胜出**，但需修正其表述：

> **根因假设（修正版）**：系统缺少"参数是否真正生效"的语义级判定能力，将HTTP响应码作为唯一的defect判定标准。在"服务器静默忽略无效参数"的场景下，此判定逻辑产生假阳性。flat key bug只是让这个问题大规模暴露，而非创造了这个问题。

**H2定位为直接技术放大器**：契约格式的语义混淆是flat key bug的直接原因，修复契约格式可以防止同类bug再次发生，但不能解决"静默忽略"场景下的假阳性问题。

---

## 各车道详细证据

### Lane 1: MRE完整性链在3处断裂

**证据强度：强**

**断裂点1 — 生成时**：`probe.rs:134` 硬编码 `status_code == 200` 为唯一判定条件，不验证参数是否真正生效。

**断裂点2 — 验证时**：`verification.rs:27-41` 双重复现只检查"同一脚本是否输出相同DEFECT标记"，`classifier.rs:177-280` 仅做字符串匹配。

**断裂点3 — 报告时**：`generator.rs:103-128` 报告门控只检查 `mre_code.contains("{{TESTVDB_DB_URL}}")` 等格式完整性，不验证MRE脚本的语义正确性。

**因果链**：L3（根因）→ L1（后果）。三处断裂的共同根因是"缺少语义级有效性校验"。

---

### Lane 2: 验证流水线缺少MRE有效性门

**证据强度：强**

- `boundary.rs:7-14`：`FuzzTestCase.expected_rejection` 始终为 `true`，`defect_marker` 始终为 `"ILLEGAL_SUCCESS"` — 只是声明，不是运行时断言
- `infra.rs:74`：`has_defect = stdout.contains("[DEFECT:")` — 纯字符串匹配
- 整个验证流水线从生成到判定到分类，全部基于HTTP响应码和字符串标记，缺少任何语义级有效性门控

**因果链**：L3（根因）→ L2（后果）。缺少有效性门是"度量错误东西"的直接表现。

---

### Lane 3: 成功指标度量了错误的东西

**证据强度：强**

- `state.rs:30-35`：`StrategyStats` 统计 `defects_found`，但defect的判定标准是HTTP响应码
- `probe.rs:134`：100%的自动probe使用 `status_code == 200` 作为唯一判定条件
- `executor.rs:270-274`：执行器通过字符串匹配检测defect标记
- 唯一例外是手写SafetyNet脚本（如 `qdrant.rs:115-116` 的score_threshold语义检查），但这些不是自动生成的probe

**这是根因假设**。修复建议：在probe脚本中增加"参数生效验证"步骤，例如：
```python
# 当前：只检查status_code
if r.status_code == 200: print('[DEFECT: ILLEGAL_SUCCESS]')

# 建议：检查参数是否真正生效
if r.status_code == 200:
    actual = r.json().get('result', {})
    if actual.get('shard_number') == -1:  # 参数确实生效了
        print('[DEFECT: ILLEGAL_SUCCESS]')
    else:  # 服务器静默忽略了参数
        print('[INFO: PARAM_IGNORED]')  # 新分类
```

---

### Lane 4: 黄金输出测试保护bug而非正确性

**证据强度：中等偏强**

- `probe.rs:498-511`：`test_qdrant_template_search_probe_golden` 断言 `from_template == from_function`（byte-for-byte）
- 这些测试的本意是保护重构安全性，但副作用是：修改probe判定逻辑（如从status_code检查改为语义检查）会导致byte-for-byte断言失败
- 没有任何测试验证"运行生成的MRE脚本后，如果服务器确实接受了非法参数，MRE能否正确检测到"

**因果角色**：修复阻力。即使识别了L3根因并想修改判定逻辑，golden测试会阻止变更——除非同时更新golden输出。

---

### Lane 5: 契约表示格式混淆语义

**证据强度：强**

- `qdrant_contract.json`：`"create_collection.optimizers_config.indexing_threshold must be > 0"` — 单一点分路径承载三种语义
- `probe.rs:70-81`：`strip_endpoint_prefix` 硬编码6个前缀，新增endpoint静默失败
- `probe.rs:57-68`：`dot_to_nested_json` 无法区分点号的语义角色
- `contract_loader.rs:67-69`：参数匹配使用模糊启发式 `llm.ends_with()`

**因果角色**：直接技术放大器。这是flat key bug的直接原因。修复建议：将契约格式从点分路径改为结构化类型：
```rust
struct ContractParam {
    endpoint: String,       // "create_collection"
    json_path: Vec<String>, // ["optimizers_config", "indexing_threshold"]
    param_name: String,     // "indexing_threshold" (唯一标识符)
}
```

---

### Lane 6: AI辅助分析中的自引用确认偏差

**证据强度：强**

- `llm.rs:6-7`：整个系统使用单一DeepSeek LLM
- `contract_loader.rs:661-688`：LLM提取契约（Phase 1/2/3）
- `llm_analysis.rs:51-108`：LLM判断缺陷真伪
- `llm_analysis.rs:110-155`：LLM生成验证变体
- `llm_analysis.rs:157-206`：LLM优化报告
- `verification.rs:44-63`：LLM既是裁判（判断缺陷真伪）又是选手（生成修复脚本）
- `review/mod.rs:12-28`：`IndependentReviewer` 只是硬编码脚本，不做交叉验证

**因果角色**：防线失效放大器。LLM自引用闭环使得：LLM生成的契约→LLM生成的测试→LLM判断的结果→LLM优化的报告，整个链条没有外部验证点。

---

### Lane 7: 规格歧义容忍度被测试消除

**证据强度：中等偏强**

- `contract/mod.rs:304-330`：`assert_eq!(contract, loaded)` — 逐字段精确比较
- `schema.rs:3-32`：自定义序列化器将 `Option<f64>` ↔ 字符串强制转换，消除数值/字符串歧义
- `contract/mod.rs:139-247`：`parse_constraints_from_assertions` 启发式规则将自然语言歧义锁定为单一解释
- `contract_loader.rs:418-497`：`validate_contract` 检测冲突但不解决歧义

**因果角色**：L4的弱化版本。当规格本身有歧义时（如"limit must be positive"到底是>=1还是>0），系统强制选择一种解释并锁定，使得另一种解释被视为"冲突"而非"歧义"。

---

## 因果关系图

```
L3 (根因: HTTP 200 = 参数有效性)
 ├── L1 (后果: MRE完整性链3处断裂)
 │    ├── 生成时: 硬编码status_code判定
 │    ├── 验证时: 字符串匹配而非语义验证
 │    └── 报告时: 格式门控而非语义门控
 ├── L2 (后果: 验证流水线缺少有效性门)
 │    └── expected_rejection=true 只是声明
 │
L5 (放大器: 契约格式混淆语义)
 └── flat key bug的直接技术根因
      └── strip_endpoint_prefix硬编码前缀
           └── dot_to_nested_json无法区分点号语义

L6 (防线失效: AI自引用确认偏差)
 └── LLM提取契约→LLM生成测试→LLM判断结果→LLM优化报告
      └── 无外部验证点

L4 (修复阻力: 黄金输出测试保护bug)
 └── byte-for-byte断言阻止判定逻辑改进
L7 (L4弱化版: 规格歧义被测试消除)
 └── assert_eq! 锁定单一解释
```

---

## 修复优先级建议

| 优先级 | 修复项 | 对应车道 | 预期效果 |
|--------|--------|----------|----------|
| **P0** | probe脚本增加"参数生效验证"步骤 | L3 | 消除"静默忽略"假阳性 |
| **P0** | 契约格式从点分路径改为结构化类型 | L5 | 防止同类flat key bug |
| **P1** | 验证流水线增加MRE有效性门 | L1+L2 | 在验证阶段拦截假阳性 |
| **P1** | 引入外部验证机制（非LLM） | L6 | 打破自引用确认闭环 |
| **P2** | golden测试改为语义断言 | L4 | 允许判定逻辑改进 |
| **P2** | 契约歧义标记机制 | L7 | 保留多种解释而非强制锁定 |

---

## 关键教训

1. **HTTP 200 ≠ 参数生效**：这是整个假阳性事件的根本认知错误。服务器静默忽略未知JSON字段返回200，在Qdrant的设计中是正常行为（宽松解析），但TestVDB将其误判为ILLEGAL_SUCCESS。

2. **点分路径是技术债**：`create_collection.optimizers_config.indexing_threshold` 用一个字符串承载三种语义，每次使用都需要手动拆分，极易出错。结构化类型是正确的长期方向。

3. **LLM自引用闭环是系统性风险**：当LLM同时负责"提出假设"和"验证假设"时，确认偏差几乎不可避免。需要引入非LLM的外部验证机制。

4. **Golden测试的双刃剑**：byte-for-byte断言保护了重构安全性，但也锁定了bug。正确做法是golden测试验证语义等价性而非字节等价性。

5. **验证≠复现**：双重复现验证的是"同一脚本是否产生相同输出"，而非"MRE是否真正触发了bug"。这是两个完全不同的问题。
