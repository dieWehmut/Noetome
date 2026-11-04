# B05-T02 条件与历史观点实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在同一普通协调入口保留明确条件与实际来源时间的并列事实，让未知条件和无法证明的时间或单位差异保持可追溯的复核状态。

**Architecture:** 旧严格 v1 原样保留，v2 使用独立模型、来源背景重建和事实/关系模块，共用已验收的身份准入、来源九件重验及输出守卫。Python 独立重建作者事实和真实來源，普通审计构造新测试项目与依据，拒绝一致伪造。

**Tech Stack:** 现有 Rust/serde/SQLite/SHA256、普通 CLI、Python 标准库与现有来源核对 verifier；无新依赖。

**Spec:** [条件与来源时间协调契约 v2](../technical/KNOWLEDGE_VARIANTS_CONTRACT.md)。

## Global Constraints

- 只消费同一实际项目内 2 至 3 份成功、单原子的 source_review 九件依据，沿用原 SourceSession、完整重建、身份映射和最终保存守卫。
- 新字段只进入独立协调 Schema2，不写回旧来源核对或 gold。v1 拒绝混入 v2 字段，v2 不默认为缺失字段填值，未知版本仍拒绝。owner 协议独立保持 Schema1。
- 成功范围固定为 `explicit_identity_condition_time_v2`，case purpose 固定为 `explicit_identity_condition_time`。
- 知识 `pending`、科学 `not_run`、研究 `not_requested`、`published=false`、网络及付费调用为实际 0。
- 禁止通过 mtime、文件名、导入/响应/运行时间或默认 0 推造 time。
- 规范化仍只去 Unicode 空白，不数值化、大小写折叠或单位换算。
- 报告 status=needs_review 时 canonical_facts=null、退出 3、不保存协调 IR，完整双报告和来源原件仍保存。
- 原概念、原子、链接顺序及全部支持依据不改写。
- 同时最多一个产品任务 in_progress；本计划两项内部工作都属于 B05-T02，不启动 B05-T03。保留用户两项旧 docs PRD 删除，不暂存或恢复。
- 普通提交和当前任务分支推送已获用户授权。代理只能提交自己的明确文件，不修改全局身份、不 force push、不写生产 Vault、不读凭据、不调用真实模型。

## Task 1: Rust v2 背景、并列事实与普通 CLI

**Files:**

- Create: `crates/noetome-cli/src/knowledge_coordination_v2_model.rs`（严格 v2 输入/输出 DTO）。
- Create: `crates/noetome-cli/src/knowledge_coordinate_context.rs`（从实际 unit 重建有界背景、公历范围及原支持引用）。
- Create: `crates/noetome-cli/src/knowledge_coordinate_variants.rs`（v2 完整键、状态、关系及实际分母）。
- Create: `crates/noetome-cli/tests/support/coordination_variants_case.rs`（使用冻结作者输入构造普通来源核对），`crates/noetome-cli/tests/knowledge_variants.rs`（普通 CLI 外部行为）。
- Modify: `crates/noetome-cli/src/knowledge_coordinate_input.rs`（v1/v2 严格分派与共同持有，不放宽旧 DTO）、`knowledge_coordinate_facts.rs`（提取共用的已验身份与原出现构造，v1 归并规则保持）。
- Modify: `crates/noetome-cli/src/knowledge_coordinate.rs`（共享输入/出处复制与最终守卫后按版本保存严格报告/IR）、`lib.rs`（私有模块与帮助范围）。
- Read only: `fixtures/knowledge-coordination-v2/`、旧 B04/v1 fixture、原来源核对及输出守卫。

**Interfaces:**

- Consumes: `VerifiedReview {input,source,review,gold,candidate,checks,files}`，已验 `IdentityMap` 本地角色；实际 unit 为 `source.units[0]`，gold.quote 是原作者事实。
- Produces: 仍使用 `knowledge_coordinate::run(project:&Path,case:&Path,output:&Path)->Result<CoordinationOutput,ProjectError>`；wire 增加契约全部 Schema2 字段，v1 调用行为不变。
- 共同身份构造建议函数 `build_occurrences(map:&IdentityMap,reviews:&[VerifiedReview])->Result<(Vec<CanonicalConcept>,Vec<CoordinationInput>,Vec<Occurrence>),ProjectError>`；只共享身份与原出处，v1/v2 分组各自明确。
- v2 时间为 `{status,precision,value}`；背景为 `{applicability_status,time,evidence_ids,unit_id,start_byte,end_byte,quote,sha256}`。精确事实键、关系顺序及 counters 以 Spec 为准。

- [ ] **Step 1: 读取冻结作者 case，不从输出生成预期，写外部测试与准备器。**

```rust
let f = VariantsCase::prepare("conditional");
let result = f.coordinate();
assert_eq!(result.status.code(), Some(0));
let report: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
assert_eq!(report["schema_version"], 2);
assert_eq!(report["totals"]["retained_fact_groups"], 2);
assert_eq!(report["totals"]["condition_variants"], 2);
f.assert_author_expectations(&report);
f.assert_protected();
```

准备器从 `authors.json` 的完整旧 Gold2、源文件和已写好的 identity/expected 构造原候选；支持 quote 必须真实包含背景+gold.quote。先写 gold、expected 和原文，再 init/import/extract verify，再 coordinate；完整 stdout/stderr/exit/argv 及作者/输入/SQL/资源/哨兵指纹写独立新测试根。

- [ ] **Step 2: 在生产代码修改前运行真实 RED 并保存证据。**

Run: `./scripts/cargo_local.ps1 test -p noetome-cli --test knowledge_variants conditional_variants_preserve_all_author_fields '--' --exact --nocapture`。

Expected: 三份来源核对实际成功，普通 v1 CLI 对已声明 v2 返回 `COORD_SCHEMA/2`，测试期望 0 因新能力缺失失败。编译/依赖/准备问题先修准备并另保留失败，不称行为 RED。选定 Python 由运行时绝对路径环境传入，仓库不得硬编码本机路径。

- [ ] **Step 3: 实现三个独立 v2 模块并接线；逐项实施 Spec。**

```rust
// 新严格模型使用 deny_unknown_fields；Option 字段使用已有 nullable 反序列化器。
// case 原字节先 recover_unambiguous_json，再匹配 schema_version 1/2，
// 分别 parse::<CoordinationCase>() 或 parse::<CoordinationCaseV2>()。
// 两种映射分别严格解析，只有通过版本 DTO 后共用身份构造。
// Context 从 actual unit.text 的唯一 gold.quote 与紧邻背景整行重建；
// 不从作者 time 生成背景，不读取文件 metadata 作为事实时间。
// 稳定键最后一项在条件 unknown 时填实际 occurrence_id，其他为 null。
```

既有输出复制、96MiB/30s、source guards、最后结果核对与捕获结果撤去保留；禁止完整逻辑复制到 v2。需要共同报告保存层时先明确封闭版本枚举或有界泛型，旧报告模板保持。

- [ ] **Step 4: 运行所有新外部测试与覆盖原能力的回归。**

必须覆盖作者每个正常/needs_review case、顺序/显示名、同来源版本（有/无实际时间）、多支持原顺序、条件/例外篡改、背景时间/支持缺失、unknown 与 NA 混淆、相邻性/非零 UTF8 范围、非法/重复/缺字段/版本、输出碰撞/重叠与保存阶段守卫。测试从固定 expected 比较全部事实组、所有原出现和实际分母；同来源版本的独立来源数在调用前写为 1。

Run focused: `./scripts/cargo_local.ps1 test -p noetome-cli --test knowledge_variants`；原 `--test knowledge_coordinate` 和模块 guard 回归。稳定代码运行一次 `cargo fmt --all --check`、`cargo check --workspace`、`cargo test --workspace`（由 wrapper 启动，记录实际线程数、16+ suites 及分母）。

- [ ] **Step 5: 小步 commit、完整报告和任务审查。**

提交明确 Rust 文件和对应测试，不包含 controller 的文档/账本或用户删除。报告含 RED/GREEN 原件路径、全部检查实际输出与 CLI/source 指纹、self-review 及关注事项；controller 生成从 dispatch BASE 开始的 review-package 并安排新鲜 spec+quality reviewer。发现实质问题在同一内部任务修复和有界复查；尚未运行独立 Python 不称 B05-T02 完成。

## Task 2: 独立 Python v2 重建与一致伪造门禁

**Files:**

- Create: `scripts/verify_knowledge_variants.py`（独立 v2 case/source/context/facts/report/IR 校验及普通 audit）。
- Create: `scripts/knowledge_variants_audit.py`（冻结作者准备、真实 argv/输出/指纹/保护及矩阵，供 verifier audit 委托）。
- Create: `tests/test_knowledge_variants_verifier.py`（作者正常样例与同步伪造的行为测试）。
- Modify: `scripts/verify_knowledge_coordination.py`（仅严格 schema 路由到 v2，保持旧 v1 check/audit）。
- Read only: 全部作者 fixture、Task1 Rust 实现/报告及旧 Python 来源准入 helpers。

**Interfaces:**

- Consumes: Task1 的普通 Schema2 输出；`source_bundle(project,directory,pin)`、`actual_source`、`rebuild_source_report`、严格读文件/JSON/指纹/readonly 保护等已有 Python 原来源核对能力。不得调用 Rust 协调器计算 expected，不能共享 Rust v2 分组逻辑。
- Produces: 既有 `verify_report(project,case,output,gold)` 对严格 v1/v2 路由；新 `verify_knowledge_variants.verify_report(project:Path,case:Path,output:Path,gold:bytes)->dict`，合法返回 `{"passed":True,...}`，任何不符返回 `{"passed":False,"error":...}`，wrapper check 退出 0/1。
- Audit: `python scripts/verify_knowledge_coordination.py audit-v2 <absolute-cli> <nonexistent-root>`；旧 `audit` 仍为 v1。新脚本自身 `audit <absolute-cli> <nonexistent-root>` 和 `check <project> <case> <output> <gold>` 可独立使用。
- 作者 expected 固定 Schema2，包含 case 名、范围/状态、每组 values/units/fullconditions/exceptions/applicability/time/input及原 atom ID、原出现状态问题、pair 关系方向和每个 totals。只从作者文件/明确调用前参数读入。

- [ ] **Step 1: 写合法 v2 被旧 verifier 拒绝的真实 RED 与一致伪造测试。**

```python
def test_conditional_report_matches_frozen_authors(self):
    self.assertTrue(verify_report(self.project, self.case, self.output,
                                 self.expected_raw)["passed"])

def test_coherent_deleted_condition_is_rejected(self):
    forged = self.copy_to_new_output("deleted-condition")
    report = self.read_report(forged)
    report["facts"][0]["conditions"].pop()
    self.rewrite_report_markdown_ir_and_fingerprints(forged, report)
    self.assertFalse(verify_report(self.project, self.case, forged,
                                  self.expected_raw)["passed"])
```

Run: 选定 Python `-m unittest discover -s tests -p test_knowledge_variants_verifier.py -v`。Task1 实际合法输出在新生产 Python 接线前被旧 schema 校验拒绝，正例测试实际 RED；伪造测试不能仅因陈旧 MD/IR/artifact 失败。不要加入故意宽松的生产 verifier 来制造 RED。

- [ ] **Step 2: 从作者与实际来源分别重建，校验所有引用和字段。**

```python
# expected_raw 先严格验 Schema2/全部字段/作者文本/非负整数与 required null。
# 九件 source_bundle 独立重验，再实际 unit 与原 gold 唯一事实重建相邻背景。
# 原身份角色映射集合重新核对；v2 complete key 与 unknown 分区独立计算。
# 重建每对关系的公历区间和方向、四状态及全部 totals，逐字段 exact。
# 保存 owner/原字节副本/IR/完整 JSON 和 Markdown 独立检查，严禁仅 passed flag。
```

共用旧 helper 只限原 source_review 准入/IO；v2事实与关系重建用 Python 标准库独立实现。v2必须拒绝不支持/混合版本、重复 JSON 键、未知字段、bool计数、缺 null、原件替换、全部支持遗漏/重复、相邻性伪造、日期误配及不同单位伪同组。

- [ ] **Step 3: 普通 audit 从调用前作者 fixture 构造并保留完整证据。**

覆盖全部作者 case +同来源有/无时间 +顺序/显示名 +多支持 +coherent forgeries；保护原输入/项目真实资源/SQL/旧IR/gold及外部哨兵。新 audit 保存 CLI 前后 SHA/长度、每次实际 argv/exit/stdout/stderr/耗时、预期原字节指纹、逐场景 check 和保护前后；失败也保留 receipt，不能只记成功摘要。修改时间 metadata 只用明确选中测试资料与新案例，核对始终不改变来源时间分类。

- [ ] **Step 4: focused GREEN、旧模块和完整 Python 稳定门禁。**

Run new module；旧 `test_knowledge_coordination_verifier.py`；一次全部 `python -m unittest discover -s tests -p "test_*.py"`，实际记录数量和结果。普通 `audit-v2` 与旧 `audit` 各保存新根，不覆盖旧证据；只在新更改/失败时重复相关检查。

- [ ] **Step 5: commit、报告、任务审查后交 controller 完成全项。**

提交明确 Python 源和测试。报告保存 RED/GREEN、audit原件及严格指纹，controller 新鲜 task review 后追加标准/验收资产、运行devlog check与git diff --check，最终 whole-branch review及一波有界修复后 complete B05-T02、最终日志 commit、普通push并核对 exact远端HEAD。本计划不自动启动B05-T03。
