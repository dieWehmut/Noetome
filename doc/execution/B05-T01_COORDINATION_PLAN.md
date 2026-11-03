# B05-T01 多来源重复与互补协调 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 从重新核验的 2 至 3 份单原子 source_review 依据产生保留全部来源出现的重复/互补协调结果，通过普通 CLI、独立作者 gold 和只读 Python 验证。

**Architecture:** Rust 复用 SourceSession、已有测量来源检查和受守卫的 OutputRoot，在新的协调模块里加载严格 case/映射、重建旧依据、按显式 UUID 与完整条件归并事实。所有原对象和来源位置逐字节保留；无法处理的差异返回 needs_review 且没有成功 IR。Python 从原字节和只读 SQLite 重新重建，源码和预期独立于 Rust 结果。

**Tech Stack:** 现有锁定 Rust workspace、serde/serde_json、sha256、uuid、same_file、SQLite；选定的工作区 Python 标准库；无新外部依赖。

**Spec:** [有界协调契约 v1](../technical/KNOWLEDGE_COORDINATION_CONTRACT.md)，并受 [当前 B05 执行卡](NEXT_STEPS.md)、[来源核对契约](../technical/EXTRACTION_REVIEW_CONTRACT.md) 及根 AGENTS 约束。

## Global Constraints

- 当前唯一产品任务为 B05-T01；不启动 B05-T02/T03，不引入 GUI、模型调用、凭据读取、生产 Vault、完整实体图或冲突裁决。
- 普通入口为 `noetome knowledge coordinate <project> <coordination-case> <new-output>`，动词后可加 `--`；参数解析不读写文件。
- case 是同项目 2 至 3 份真实 extract verify 成功依据；严格 JSON schema_version=1，九文件精确清单、manifest 指纹、旧 IR/报告/源资源均需重新核验。
- manifest/映射各 64 KiB，映射数组各 1024 项，source_review 原限额保持，全部输入合计 96 MiB，总体 30000 ms；输入 ID、文件名、UUID 和文本规则按 Spec 原值。
- 完整同键才重复，键是 `[object_id,property_id,normalized_value,normalized_unit,sorted_normalized_conditions,sorted_normalized_exceptions]`；移除 Unicode 空白，不换算、不数值化、不删条件/例外。
- fact_id=`fact-`+紧凑 UTF-8 JSON 键 SHA256；occurrence_id=`occ-`+`[project_id,source_id,version_id,parse_id,unit_id,start_byte,end_byte,original_atom_id]` 紧凑 JSON SHA256。
- 名称与输入顺序不参与身份。每份原 IR 的概念、原子、支持证据、实际 binding 和 UTF8 定位保留；复制同一实际来源单位不能成为第二个独立来源。
- 不支持的同对象/属性语义差异或同来源不同版本/parse 身份为 needs_review/COORD_UNHANDLED_DIFFERENCE；canonical_facts=null，原组与证据保留，退出3且无协调IR。
- 知识 pending、published=false、科学 not_run、研究 not_requested、network_calls=paid_calls=0。作者样例不能证明真实教材或科学正确率。
- 新输出 root 独占、不得重叠；守卫贯穿保存之后，只撤去当前捕获的三个结果文件，不递归清理、不覆盖/删除未知内容。
- author gold/expected/map/source 在候选之前冻结；先普通 CLI 行为 RED，再生产代码；独立验证器先观察真实伪报告被错误接受的 RED，再实现检查。
- 任务小步提交，保留两项用户 docs 删除；普通推送已授权，检查通过后核对远端。任何失败原日志只追加、不抹去。

## 文件和责任

| 文件 | 本任务责任 |
| --- | --- |
| `crates/noetome-cli/src/knowledge_coordination_model.rs` | 所有严格输入/输出 DTO、固定字段与必填 nullable |
| `crates/noetome-cli/src/knowledge_coordinate_input.rs` | case/九文件能力、指纹及重新计算的 SourceReview 依据；保留 SourceSession |
| `crates/noetome-cli/src/knowledge_coordinate_facts.rs` | 显式映射、稳定 ID、出现分区、重复/互补/待复核关系与真实分母 |
| `crates/noetome-cli/src/knowledge_coordinate.rs` | 正常运行编排、受保护保存、双报告、最后守卫 |
| `crates/noetome-cli/src/extraction_verify.rs` | 小范围导出共同报告/Markdown构造与 unique-key 解码，不复制旧检查大块、不放宽旧行为 |
| `crates/noetome-cli/src/extraction_output.rs` | 新 owner 模式和只撤去协调三个结果的守卫行为 |
| `crates/noetome-cli/src/lib.rs`、`src/main.rs` | 最小正常参数解析/帮助/调用与非零退出 |
| `crates/noetome-cli/tests/knowledge_coordinate.rs` | 普通 CLI RED、真实来源依据、失败与保护集成；记录实际 root |
| `fixtures/knowledge-coordination-v1` | 已冻结的三来源作者 gold、身份映射和外部 expected，不改为实际输出 |
| `scripts/verify_knowledge_coordination.py` | 独立只读验证与普通 CLI 有界审计驱动 |
| `tests/test_knowledge_coordination_verifier.py` | 假成功、完整伪造、分母、身份和原证据破坏的行为测试 |
| `doc/validation/B05-T01.md` 与 `assets/B05-T01-*` | 实际执行、可复查证据与范围；控制器收尾 |

## 执行记录和授权解释

本计划的 SDD 文件只写 `.superpowers/sdd/B05-T01_COORDINATION_PLAN/`，不读取或改写其他计划的工作目录。每个内部 Task 用一个新鲜实现者和一个 task-scoped spec+quality reviewer；修复前三轮恢复原实现者，最多5轮；最终只做一次整分支审查、一波修复和一次限定复查。不重复运行同代码已有回归来替代读证据。

普通任务分支按项目 WORKFLOW 在已有 checkout 建立：`codex/b05-t01-knowledge-coordination`，基线 `ea4d900d14f47f453855ad1277afb3a808dbec0a`。用户已经要求持续开发并授权小步普通提交/推送；已采纳 B05 执行卡的局部契约实施无需重新审批。可选临时目录清理已有自动审批拒绝，保留审查目录与恢复归档，不重试删除。

### Task 1: 正常 Rust 协调 CLI、来源依据重验和保护

**Files:** 新建上表四个 knowledge_* Rust模块和普通CLI集成测试；只修改 extraction_verify/extraction_output/lib/main 的必要接口。作者 fixture 文件在控制器已冻结，读而不改。测试辅助文件只写新 target/compiler-temp 根；必要新增测试专用 hook 在本模块 cfg(test) 内，不创建 CLI/env 绕过入口。

**Interfaces:**

```rust
// knowledge_coordinate.rs，main 的唯一调用点
pub struct CoordinationOutput { pub json: String, pub failed: bool }
pub fn run(project: &Path, case: &Path, output: &Path)
    -> Result<CoordinationOutput, ProjectError>;

// knowledge_coordinate_input.rs
pub(crate) struct CaseInputs {
    pub(crate) config: CoordinationCase,
    pub(crate) map: IdentityMap,
    // 私有根能力、两个原字节和原目录清单，不能丢弃守卫
}
impl CaseInputs {
    pub(crate) fn open(path: &Path, control: &Control) -> Result<Self, ProjectError>;
    pub(crate) fn root(&self) -> &Path;
    pub(crate) fn original_bytes(&self) -> (&[u8], &[u8]);
    pub(crate) fn verify(&self, control: &Control) -> Result<(), ProjectError>;
}
pub(crate) struct VerifiedReview {
    pub(crate) input: InputSpec,
    pub(crate) source: SourceSession,
    pub(crate) review: Review,
    pub(crate) gold: Gold,
    pub(crate) candidate: SavedCandidate,
    pub(crate) checks: Vec<Check>,
    pub(crate) files: BTreeMap<String, Vec<u8>>,
    // 私有依据目录/文件守卫贯穿最终保存
}
impl VerifiedReview {
    pub(crate) fn open(project: &Path, input: &InputSpec, case: &Path,
        control: &Control, cancel: &Arc<AtomicBool>) -> Result<Self, ProjectError>;
    pub(crate) fn verify(&self, control: &Control) -> Result<(), ProjectError>;
}

// knowledge_coordinate_facts.rs
pub(crate) fn coordinate(map: &IdentityMap, reviews: &[VerifiedReview])
    -> Result<CoordinatedFacts, ProjectError>;
// CoordinatedFacts 包含 canonical_concepts/inputs/occurrences/facts/relations/
// totals/status，类型字段严格按 Spec 第6节，供 run 写 report 和 IR。

// extraction_verify.rs，小范围复用现有 fn report 的精确构造
pub(crate) fn report_from_fingerprints(r: &Review, s: &SourceSession,
    a: &Assessment, c: Option<&SavedCandidate>,
    inputs: BTreeMap<String, Fingerprint>, artifacts: BTreeMap<String, Fingerprint>)
    -> Result<ReviewReport, ProjectError>;
pub(crate) fn review_markdown(report: &ReviewReport) -> Result<String, ProjectError>;
pub(crate) fn recover_unambiguous_json(bytes: &[u8]) -> Result<Value, serde_json::Error>;

// extraction_output.rs，沿用原身份/字节守卫
impl OutputRoot {
    pub(crate) fn create_coordination(path: &Path, request_id: &str)
        -> Result<Self, ProjectError>;
    pub(crate) fn discard_coordination_result(&mut self) -> Result<(), ProjectError>;
}
```

所有新 DTO 名称为 `CoordinationCase,InputSpec,IdentityMap,CanonicalConcept,ConceptMapping,AtomMapping,CoordinatedFacts,CoordinationInput,Occurrence,Fact,Relation,CoordinationTotals,CoordinationReport,CoordinationIr`。字段及固定值逐项按 Spec 第2/3/6节，不添加宽松 Value 元数据容器；SourceBinding/RequestUnit/CandidateAtom/Concept/Evidence/Check/Fingerprint沿用原严格类型。用于固定 String 或 1 的 serde deserialize_with 与已有 DTO 保持同一形式。

- [ ] **Step 1: 写普通 CLI 首个行为测试并实际观察 RED。** 辅助 `PreparedCase` 保存 root/project/case/output/作者原字节和保护指纹，`PreparedCase::baseline()` 先读取 fixture 的原文、三个gold、map、expected并记录其SHA，再用普通 project init、source import、extract verify 准备九文件依据；所有 CLI stdout/stderr/exit 保存到 root。按实际 import 返回构建 SourceBinding；候选 request_id 为 `synthetic-coordination-<input_id>`，两个concept ID为 `<id>-subject`、`<id>-property`，原子/证据 ID按冻结map/expected；全文支持quote采用真实 UTF8开始/结束和SHA，decision included、单atom。review subject=样例 A、property为速度/质量、allowed_contexts=[]、timing=before_synthetic_response。每份 extract verify 必须退出0，再冻结 manifest 九指纹。

```rust
#[test]
fn ordinary_cli_coalesces_duplicate_preserving_three_sources() {
    let f = PreparedCase::baseline();
    let result = f.coordinate(); // std::process::Command 调 CARGO_BIN_EXE_noetome
    f.save_cli("first-coordinate", &result);
    assert_eq!(result.status.code(), Some(0), "{}", String::from_utf8_lossy(&result.stderr));
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["totals"]["canonical_facts"], 2);
    assert_eq!(report["totals"]["source_occurrences"], 3);
    assert_eq!(report["totals"]["independent_sources"], 3);
    assert_eq!(report["totals"]["evidence_links"], 3);
    let velocity = report["facts"].as_array().unwrap().iter()
        .find(|x| x["value"] == "10" && x["unit"] == "m/s").unwrap();
    assert_eq!(velocity["occurrence_ids"].as_array().unwrap().len(), 2);
    f.assert_protected(); // 原source project/IR/gold/map/作者文件与外部哨兵逐字节
}
```

运行 `scripts/cargo_local.ps1 test -p noetome-cli --test knowledge_coordinate ordinary_cli_coalesces_duplicate_preserving_three_sources -- '--test-threads=1'`，PowerShell把 Cargo 分隔符写为字面 `'--'`。记录真实 CLI缺失行为的失败（预期CLI_INVALID_ARGUMENT/2）；编译/路径错误不能算 RED。此时不改生产解析/协调逻辑。

- [ ] **Step 2: 实现严格模型与保存依据重验。** CaseInputs先捕获根/父目录/manifest/map，严格unique JSON+DTO，再核对单层清单和声明限额。VerifiedReview依次capture九文件、复查指纹、解析owner+四原输入，SourceSession按manifest绑定import/原review selection重开；控制检查每阶段执行。旧报告 helper 从原 report 构造提取，原 run 继续调用同 helper，原source_reviewMarkdown字符串逐字节保持。完整 assess/IR/snapshot/report/MD重建不读取 passed 作为oracle。

```rust
let assessment = extraction_review::assess(&review, &gold, &decision,
    Some(&candidate), &source.units[0], &source.binding, None, None);
if !assessment.passed || candidate.payload.atoms.len() != 1 {
    return Err(ProjectError::new("COORD_SOURCE_REVIEW", 3, "single passed atom required"));
}
// 调 assess 前须 validate_gold/validate_candidates、完整绑定、timing、included、
// candidate/entity/string限额全部检查；这段不替代这些具体契约。
let expected = report_from_fingerprints(&review, &source, &assessment,
    Some(&candidate), original_role_fingerprints, saved_artifact_fingerprints)?;
if serde_json::to_value(&saved_report)? != serde_json::to_value(&expected)?
    || saved_markdown != review_markdown(&expected)?.as_bytes() {
    return Err(ProjectError::new("COORD_SOURCE_REVIEW", 3, "saved source review differs"));
}
```

在调用coordinate前，按实际 source单位元组拒绝重复；保留 input_id 与原错误原因。所有 helper需要的原角色指纹按review原文件名取得（不是copy文件名），artifact七指纹按原report正式文件名取得。JSON错误转换为 ProjectError，不 panic。

- [ ] **Step 3: 实现映射和事实组、双报告与正常CLI。** 完整核对每份概念/原子映射恰好一次和对应主体/属性组件，不按canonical显示名猜角色。构建每个 occurrence后先排序，事实键和ID按以下完整纯算法实现，source单位元组和所有出处另保留。

```rust
fn fact_key(o: &Occurrence) -> Value {
    let sorted = |items: &[String]| {
        let mut values: Vec<String> = items.iter().map(|x| extraction_review::normalized(x)).collect();
        values.sort();
        values
    };
    json!([o.object_id, o.property_id, extraction_review::normalized(&o.value),
        extraction_review::normalized(&o.unit_value), sorted(&o.conditions), sorted(&o.exceptions)])
}
fn stable_id(prefix: &str, key: &Value) -> String {
    format!("{prefix}{}", text::sha256(serde_json::to_vec(key).unwrap().as_slice()))
}
```

值都是经过验证的 JSON字符串/数组，生产错误处理不可引入不可信JSON unwrap。每对出现按同source不同版本/parse优先 needs_review，再按不同对象 separate_object、不同属性 complement、同键duplicate、其余needs_review。全部关系ID有序且恰好一次，标记相关事实/出现needs_review。

真实分母从实际 vector/partition/unique source_id重算，needs_review的canonical_facts=None。保存新的OutputRoot owner、两原输入、每份九文件copy，成功IR与完整JSON/MD，并重读类型/内容核对。最後保持所有input/source/output guard直到返回，失败只discard当前三结果。main退出值按输出failed决定0/3，ProjectError原exit；帮助清楚说明此有界范围和未发布。

- [ ] **Step 4: 实际运行失败和保护集成。** 新测试覆盖以下具体输入，并保存真实root/CLI日志：

| 输入 | 必须观察到 |
| --- | --- |
| 基线a/b同SHA且不同source | 2facts/3positions/3sources/3supports，a/b并组，c互补 |
| 反向input、mapping数组；改显示名 | canonical/occurrence/fact IDs及关系完全相同，原名称未改 |
| 只保留a/b的合法two-source case与映射 | 1fact/2positions/2sources，非三项硬编码 |
| manifest的b binding偷换a，字节SHA仍相同 | 非零COORD_SOURCE_REVIEW，无成功IR |
| 同步改b原IR或candidate内容/测量/第二证据，并重算全九指纹/旧报告指纹 | 重验真实来源或完整旧report拒绝，不靠pin碰巧失败 |
| b旧报告原candidate计数null/0/错误值；MD同步 | 拒绝假原分母，无成功IR |
| 复制a九文件成b，改input/map并重算指纹 | COORD_DUPLICATE_INPUT/2，不多计独立来源 |
| 新作者b短文/gold在候选前冻结，条件20→30或单位m/s→km/s | 来源依据真实通过，协调needs_review/3、canonical=null、3原事实/出处仍保留且没有协调IR |
| 同source-id普通import的新真实版本，加其成功依据 | needs_review，真实独立source计数减少，不编造时间 |
| map缺项/错角色/未知ID/重复项，extra/duplicate JSON键/缺nullable | COORD_IDENTITY或COORD_SCHEMA/2，原输入哨兵不变 |
| 已有输出/祖先或子路径重叠，链接/硬链接/文件别名，原目录或字节在results_saved阶段变动 | 拒绝且保留未知内容；已捕获的三个成功结果撤去 |

适当加 cfg(test) stage hook `inputs_held`、`results_saved` 复用已有真实guard模式；通过自造新目录验证，不让生产CLI接受hook。测试命令结束后汇总实际passed/failed/ignored和原指纹；首次失败与修复记录不抹去。

- [ ] **Step 5: 检查、提交和自审。** 先聚焦新knowledge_coordinate和受影响extraction_verify targets，然后一次 Rust workspace fmt/check/test（Cargo锁定离线，literal分隔符，NOETOME_TEST_PYTHON/NOETOME_PDF_PYTHON选定解释器）。把命令argv、CLI SHA、源码指纹前后、真实stdout/stderr/exit保存到本计划SDD目录。git diff --check，审查只stage授权代码/测试，不包含用户docs删除/target/SDD。可两步提交测试RED与实现GREEN，但最终对外报告必须说明所有已知失败是否解除。写task-1-report.md完整报告和正例root，短答DONE/commits/实际测试/concerns。控制器安排新鲜spec+quality审查。

### Task 2: 独立 Python 来源/协调核验、伪造回归和CLI审计

**Files:** 新建 `scripts/verify_knowledge_coordination.py`、`tests/test_knowledge_coordination_verifier.py`。只读作者 fixture和Task1实际源码/证据；不改Rust、旧verifier或已有作者预期。控制器将Task1实际基线保留为 `doc/validation/assets/B05-T01-source-review-fixture.zip`，zip中的规范项目/case/output/gold均来自普通CLI，所有成员公开自造材料。

**Interfaces:**

```python
def verify_report(project, case, output, gold_bytes):
    """只读实际来源和全部保存结果；返回核验summary，假成功抛AssertionError。"""

def main(argv=None):
    # check <project> <case> <output> <gold>：只读核验，成功0，失败1
    # audit <cli> <new-test-root>：新根内实际CLI准备/运行/破坏/保护，成功0，失败1
    # --help 正常0；参数非法2
```

仅复用 `verify_extraction_knowledge.strict_json,fingerprint,actual_source,verify_report` 的独立只读来源算法，导入采用仓库scripts相对位置，不使用Rust协调函数/内嵌expected。源review.verify_report读取九件依据并按实际source解释；仍须补齐该旧函数不承诺的当前case九fingerprint、owner严格字段、严格类型/limits/路径、当前coordination schema、原角色边界及源IR的完整关系要求。

- [ ] **Step 1: 解包真实基线到新临时目录并先观察 verifier 行为 RED。** 一份临时初始verify_report仅返回保存JSON而不验证；它代表协调检查尚缺失，用下列测试证明可接受的结构伪造会导致真实AssertionError测试失败。初始普通 CLI基线已有Task1的GREEN，勿重新用“无法导入函数”当RED。

```python
# 临时RED骨架；GREEN实现必须替换它，不提交为完成验证器
def verify_report(project, case, output, gold_bytes):
    return json.loads((Path(output) / "coordination.json").read_text(encoding="utf-8"))

def test_missing_second_occurrence_is_rejected(self):
    self.remove_second_occurrence_and_rewrite_report_ir_md_fingerprints()
    with self.assertRaises(AssertionError):
        verify_report(self.project, self.case, self.output, self.gold_bytes)
```

unittest辅助从公开zip解包到新的临时根，解包前reject绝对/..路径。变异同步IR对应字段、coordinationJSON/MD事实及artifact指纹，保留case作者gold/实际源项目，避免只检查自洽性。测试基线必须通过；坏例包括删b全部出现/证据、把速度并入质量、伪值/单位/条件/例外、删除原对象、改源/版/UTF8边界/绑定、复制源却宣称3independent、改身份ID或角色、偷改原IR、虚假canonical/count/状态/published/scientific、漏原nullable/extra/duplicateJSON、错误或附加output清单。原firstRED日志必须保留。

- [ ] **Step 2: 实现完整独立重建。** 先读gold原字节并严格校验字段与预期，再读case、map、manifest九fp和SourceReview原件；source用只读immutable SQLite重新定位原资源/Unit/UTF8，调用旧独立verify_report证明全部来源依据实际passed。一份IR候选必须真有一个atom和每个支持证据，actualsource/原gold/原角色字节不可互换。

独立按原map建全概念及主体/属性角色，按原gold明确value/unit/conditions/exceptions建事实key/ID和实际occurrence ID，从实际原子而非报告建立出现、完整原字段和证据列表。按Spec对每对出现重建duplicate/complement/separate_object/needs_review和逐出现/fact状态；重建全部计数和原分区。

```python
def normalized(value):
    return "".join(value.split())

def compact(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode("utf-8")

key = [mapping["object_id"], mapping["property_id"], normalized(gold["value"]),
       normalized(gold["unit"]), sorted(map(normalized, gold["conditions"])),
       sorted(map(normalized, gold.get("exceptions", [])))]
fact_id = "fact-" + hashlib.sha256(compact(key)).hexdigest()
```

外部作者expected还独立约束真实速度/质量、条件例外、原ID/source成员和分母。报告/IR各严格字段、fixed值、bool/int/nullable类型、完整同事实Markdown、所有artifact副本fp、唯一output清单与实际所需IR存在关系逐一核对。需要复核时没有IR且canonical=null；known空值、unknown伪零、非整数/布尔count不可通过。输入/source/output只读inventory前后核对；失败抛明确AssertionError，CLI check诊断不含凭据。

- [ ] **Step 3: 实际普通CLI审计矩阵。** audit驱动先确保全新测试根，外部哨兵在该根外；保存作者gold/fixture指纹先于候选。准备普通三source/source_review/case并保存每个Command实际argv/stdout/stderr/exit/time，运行coordinate后调用独立verify_report。至少正常三源、两源、顺序/显示名变更、条件差异和单位差异needs_review、同source不同版本needs_review；派生作者expected在候选前保存，不从Rust报告反填。

对原/派生依据做binding同SHA偷换、IR/candidate+全部fingerprint同步伪造、第二证据丢失、原unknowncount、复制输入、未知/重复键和输出碰撞。每个场景使用新根或新case/output，不改前个原样例；拒绝路径检查真实非零和无成功IR。审计summary只汇总实际执行，不把内部assert数量称为产品测试数。

保存原source project/SQL/原IR/gold、作者fixture、外部哨兵的前后完整SHA；发生预期拒绝仍要核对保护。守卫 race 若已由Task1 cfg测试执行，记录其命令/结果，不把“驱动无法注入”称为再次CLI race通过。保留actual read-only check结果及CLI SHA；成功和失败原日志均不覆盖。

- [ ] **Step 4: 聚焦 GREEN、模块回归和提交自审。** 按新增unittest执行聚焦测试、完整新模块与audit；只在失败或新更改后重复受影响检查。控制器在最终源码上执行一次完整Python/Rust/log/Git回归和远端核对。git diff --check，小步提交授权两文件，写task-2-report.md完整TDD/实际审计/证据位置/局限，短答状态/提交/真实测试/concerns。控制器做Task2新鲜spec+quality审查，最终整分支审查只有一个席位。

## 控制器收尾门槛

- [ ] 对两个Task的代码和每条spec/quality观察作实际证据检查，修复重要问题并限定复查；全部接口/分母/原保护需求在验收表有对应真实命令与文件。
- [ ] 适当完整 Rust fmt/check/workspace test、完整 Python unittest、devlog check、git diff --check：记录当前HEAD/源码/CLI指纹、命令、实际输出、exit与范围，保留首次失败。
- [ ] 保存公开自造的审查、CLI和独立核验证据到assets，写真实 B05-T01 验收；标准与入口同步，PRD未变不重建。
- [ ] devlog complete B05-T01使用实际产物与验证，check生成账本视图；仍仅B05-T02/T03待办，不切批次。
- [ ] 最后分步commit普通push当前分支，git ls-remote核对 exact HEAD；结尾交代实际完成、局限、剩余下一步及全部控制器rulings。审查临时目录和可恢复归档保留，不再递归删除。
