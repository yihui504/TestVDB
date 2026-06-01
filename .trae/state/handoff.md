# TestVDB 交接信息

## 最后更新: 2026-06-01

## 项目状态: 🎯 新一轮 deep-interview 已完成，等待 Ralph 执行

---

## Plan/Spec 唯一存放目录

**`TestVDB/.trae/`** — 所有plan/spec只存放在此目录下

### 活跃文件

| 文件 | 路径 | 状态 |
|------|------|------|
| Spec (三重提升) | `.trae/specs/deep-interview-triple-pillar-2026-06-01.md` | **ACTIVE — 7轮deep-interview，模糊度11.3%，等待执行** |
| Plan (Oracle优化) | `.trae/plans/conditional-branch-llm-next.md` | COMPLETED |
| Plan (技术债修复) | `.trae/plans/tech-debt-remediation.md` | COMPLETED |
| Spec (条件分支) | `.trae/specs/deep-interview-llm-defect-discovery-next.md` | ARCHIVED |
| PRD (技术债) | `.trae/prd.json` | COMPLETED |
| Goal State | `.trae/state/goal-state.json` | COMPLETED |
| Ralph State | `.trae/state/ralph-state.json` | INACTIVE（等待新PRD） |
| Handoff | `.trae/state/handoff.md` | 本文件 |
| Progress | `.trae/progress.txt` | ACTIVE |

---

## 本轮 Spec 摘要: 三重提升 (自动化 + Bug产出 + 验证能力)

### 三大支柱

1. **自动化程度（自愈+全流程闭环）**
   - 五阶段无人值守管线：合同提取 → 缺陷挖掘 → 假阳性过滤 → Issue生成 → 经验交接与多轮循环
   - 自愈：Docker重启、LLM重试(≤3次)、断点恢复、沙箱自动清洗
   - 底线：全流程无人值守运行

2. **Bug真实产出能力**
   - 每轮≥1个真正有效的新bug（非假阳性、非by-design）即成功
   - 核心CRUD端点100%覆盖 + 合同质量门控
   - 不得硬编码引导发现已知bug

3. **验证能力**
   - 独立审查者二次验证 + 一键复现 + `cargo run -- verify <issue-id>` 独立命令

### 12项验收标准 (AC1-AC12)

| AC | 内容 | 类型 |
|----|------|------|
| AC1 | 自愈(Docker+LLM+沙箱三方) | 自动化 |
| AC2 | 断点恢复 | 自动化 |
| AC3 | 无人值守四DB完整Mine | 自动化 |
| AC4 | 合同质量门控+CRUD 100% | Bug产出 |
| AC5 | 四DB中≥2个发现真实新bug | Bug产出 |
| AC6 | 假阳性漏网≤5%, 真实100% | Bug产出 |
| AC7 | IndependentReviewer二次验证 | 验证 |
| AC8 | MRE一键复现 | 验证 |
| AC9 | verify独立命令 | 验证 |
| AC10 | 经验交接+不重复探索 | 自动化 |
| AC11 | 四DB通用代码路径 | 通用性 |
| AC12 | contracts/issues数据资产不变 | 安全性 |

### 关键约束
- contracts/ 和 issues/ 不可动（数据资产）
- 四DB全部支持（Milvus+Qdrant+Weaviate+PGVector）
- 激进改造允许，但做好备份
- 结果导向，通用性优先
- 回归测试不强制但推荐

---

## 环境信息（不变）

### 编译: Rust 2024 / cargo build 成功 / 444 passed, 0 failed
### Docker: Milvus(19530), Qdrant(6333), Weaviate, PGVector
### DeepSeek API Key: C:\Users\11428\Desktop\mftui\deepseekapikey.txt