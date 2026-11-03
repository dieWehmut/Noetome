# B04-T03 来源知识核对 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将已保存的第六次真实候选与未改变的调用前gold和实际来源独立核对，保存有界未发布IR及一致的双报告。

**Architecture:** SourceSession继续持有实际来源能力，四份明确review输入用有界守卫只读。严格候选校验与受控单测量知识判据分开，只有通过才输出source_review IR；独立Python从原gold/SQLite/字节验证行为与分母。

**Tech Stack:** 现有Rust 1.98.1/serde/serde_json/sha2/same-file/SQLite依赖，不加库；CPython3.12与现有本机cargo包装。

**Spec:** [有界来源核对契约](../technical/EXTRACTION_REVIEW_CONTRACT.md)。原B04-T02候选契约继续约束不可放宽字段。

## Global Constraints

- 当前唯一活动任务B04-T03；用户2026-11-03明确要求去掉系统标题栏、仅保留菜单一行，追加下述Task3窗口栏纠正。除此以外不推进GUI/OCR/完整图谱或生产Vault。
- 普通CLI四路径参数；新输出根；原来源、原gold、六次响应和已保存预算只读不变；本任务新网络/付费调用0。
- Schema1 review/decision/保存API包装严格，gold1保持原调用前字节；gold2专用于独立作者例外/未知工程样例。
- selection恰一单位；候选1 MiB、其余文件64 KiB；每类候选实体1024；条件/例外每类8、字符串1024 UTF8字节；来源输入1 MiB；30秒。
- expected_atoms=1，passed+failed+needs_review+unprocessed=1，实际候选/多余/语义重复另外计数，空例外not_applicable。
- scientific_quality=not_run、research_status=not_requested、published=false；原atom始终unverified/required=false。
- 用户两份docs/Noetome-PRD-v1.0文件删除保持；只显式暂存本任务文件。当前独立任务分支codex/b04-t03-knowledge-verify，无force/global身份改动。
- 当前选定Python由工具实际返回绝对路径；不得把该本机路径硬编码进仓库程序。

---

### Task 1: 普通CLI、严格有界核对与IR/双报告

**Files:**
- Create: `crates/noetome-cli/src/extraction_verify.rs`（运行协调与输入保护）
- Create: `crates/noetome-cli/src/extraction_review_model.rs`（严格review/gold/decision/IR/report DTO）
- Create: `crates/noetome-cli/src/extraction_review.rs`（来源/概念/事实/条件/单位/例外/覆盖及分母纯核对）
- Modify: `crates/noetome-cli/src/extraction_output.rs`（显式source_review owner与仅捕获结果清除）
- Modify: `crates/noetome-cli/src/lib.rs`, `src/main.rs`（extract verify参数/帮助/退出码接线）
- Modify: `.gitattributes`（新作者fixture及本任务原始审计副本的字节/SHA保存）
- Create: `crates/noetome-cli/tests/extraction_verify.rs`
- Create: `fixtures/extraction-review-v1/README.md`, `source.txt`, `gold.json`, `review.json`, `decision.json`, `candidates.json`（独立作者含例外基准及明确synthetic回放；动态身份由测试取得）

**Interfaces:**
- Consumes: `SourceSession::open(project: &Path, import_id: &str, selection: &[Selection], limits: &ExtractionLimits) -> Result<SourceSession, ProjectError>`、`verify()`、`binding`、`units`；`extraction::validate_candidates(...)`；`ApiCandidates`。
- Produces: `pub fn extraction_verify::run(project: &Path, import_id: &str, case: &Path, output: &Path) -> Result<VerificationOutput, ProjectError>`，`VerificationOutput { pub json: String, pub failed: bool }`；上述契约严格四份输入与knowledge-ir.json/verification.json/verification.md/source-snapshot.json及原输入字节副本。
- Produces: report.scope/binding、origin、rules、expected_results、candidate_results、checks、totals、quality、artifacts。两类结果对象给出object_id/unit_id/status及问题码；checks按category/object_id/status/expected/actual记录，quality由checks真实归类。
- Internal names: DTO及纯核对函数可由Task1自行选择，Task2只消费JSON外部契约及普通CLI，不导入生产实现。

- [x] **Step 1: 先作者化含例外fixture与集成测试。** 源文写清同一subject/property/value/unit/两条件/一个例外及测试身份；gold2在生成synthetic候选之前保存。真实sample继续读取未改的extraction-gateway-v1/expected.json。普通CLI断言示例：

```rust
let run = Command::new(env!("CARGO_BIN_EXE_noetome"))
    .args(["extract", "verify", project, import_id, case, output]).output().unwrap();
assert_eq!(run.status.code(), Some(0));
let report: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
assert_eq!(report["totals"]["expected_atoms"], 1);
assert_eq!(report["totals"]["passed"], 1);
assert_eq!(report["published"], false);
assert!(Path::new(output).join("knowledge-ir.json").is_file());
```

变体必须分别断言VERIFY_CONDITION/VERIFY_UNIT/VERIFY_EXCEPTION/VERIFY_FACT或VERIFY_CONTENT/VERIFY_CONCEPT/VERIFY_COVERAGE/VERIFY_DUPLICATE/VERIFY_MISSING，以及实际对象和IR不存在。重复新atom_id也不能通过。wrong fact保持正确原quote来检验真正的知识判据；unit/condition删除同时删measurement与qualifiers以绕过仅格式一致性检验。

- [x] **Step 2: 保存真实RED。** 在任何生产接线前运行：

```text
./scripts/cargo_local.ps1 test -p noetome-cli --locked --offline --test extraction_verify -- --test-threads=4
```

预期正确基准失败于普通CLI尚无verify入口，负变体缺少必须的对象报告/问题码而失败。保留原stdout/stderr/命令/exit、源/测试SHA；若编译或设置错误，先修设置再取得真实行为RED。

- [x] **Step 3: 实现接口与有界语义。** 严格逐类型解析文件，verify调用前后输入/SourceSession守卫；gold SHA、基准原文、受控正文语法、关联概念、完整condition/exception/unit集合及decision分别核对。内部事实语法只能接受spec定义的连接符和受控前缀；它由gold字段构建判据，禁止用模型输出补gold。

```rust
pub struct VerificationOutput { pub json: String, pub failed: bool }
pub fn run(project: &Path, import_id: &str, case: &Path, output: &Path)
    -> Result<VerificationOutput, ProjectError>;
```

基准无效作为输入失败；正确基准配错误候选保存逐对象报告，退出3；preflight错误退出2或现有保护错误码。源/unknown/decision复核保持needs_review或unprocessed，退出3且无成功IR。严格DTO/基本错误仍保存可确定分母，不能伪造通过项。输出改变后只清除捕获结果文件，保留未知文件和输入副本。

- [x] **Step 4: focused GREEN与保护测试。** 正常原gold概念组合/允许context、含例外、unknown、非零UTF8Span通过相应状态核对；错误来源/parse、source/quote/hash、原文无关引文、断链concept、错误排除、遗漏/语义重复/重复ID及严格字段拒绝。用测试内部阶段callback检验真实源/输入/输出新增/变化、Windows deny-write、已有输出/根内输出/链接与真实硬链接拒绝；callback不进入CLI。每次读取/变更前后核对原源SQL/原字节和selected输入不变。只按当前改动focused迭代，不让测试超时掩盖失败。

- [x] **Step 5: 自审、fmt/check、提交并报告。** 只暂存Task1所有文件，不碰dev-log四份start状态及用户删除；提交包含实现/fixture/测试。完整workspace回归由根在最终集成代码上运行一次，Task1提供全部focused证据，不额外重跑无关全库。报告写本plan SDD workspace/task-1-report.md，含RED/GREEN原日志路径和命令、提交、接口字段/保护/局限。根随后生成新鲜task-scoped spec+quality审查并修必要问题。

### Task 2: 独立核对矩阵、真实第六输入、标准及最终验收

**Files:**
- Create: `scripts/verify_extraction_knowledge.py`（独立普通CLI与真实字节/SQL/语义/分母/双报告验证）
- Create: `tests/test_extraction_knowledge_verifier.py`（仅验证validator实际识别坏报告/坏事实，不镜像Rust实现）
- Modify: `doc/technical/DATA_CONTRACTS.md`, `ARCHITECTURE.md`, `doc/standards/TESTING.md`, `doc/execution/NEXT_STEPS.md`（本项接口与真实范围状态）
- Create: `doc/validation/B04-T03.md`, `doc/validation/assets/B04-T03-*`（实际运行日志/真实原输入/最终报告/审查/manifest；不要猜造文件）

**Interfaces:**
- Consumes: 普通 `extract verify`、review-case四份严格输入、Task1实际JSON外部字段、原API payload与source_snapshot。原gold与原response只读。
- Produces: `python scripts/verify_extraction_knowledge.py --binary <absolute-noetome> --output <nonexistent-root>` 离线矩阵；另可 `--live-project <source-project> --live-import <id> --live-candidate <original-candidates> --live-gold <original-gold>` 在独立新根仅复核保存的真实候选，四参数成组且必需真实指纹。audit.json保存calls/rows/checks/actual fingerprint，失败也保存已完成调用和真实错误。
- Produces: JSON与Markdown解析后同事实检查；实际原candidate/gold副本相等、source SQL/字节/mtime/目录清单保持。science/research/network状态明确；expected/candidate真实分母及独立事实用预读gold与实际数据算出。

- [x] **Step 1: 作者基准和行为RED。** 验证器独立从original gold先预读expected/value/unit/conditions，再执行CLI；使用真实Task1输出复制为新测试报告，分别破坏facts/单位/条件/例外/分母/IR/快照/JSON与MD，断言validator拒绝，先运行新test模块取得RED，再实现所需检查。

```python
with self.assertRaises(AssertionError):
    verify_report(bad_report, original_candidate, frozen_gold, actual_source)
```

- [x] **Step 2: 实现独立矩阵。** 至少normal/compound或关联components/允许context，condition/unit/exception丢失、fact变更/否定、concept断链、wrongquote/无关quote、missing/duplicate、wrongexclude/pending/review/unknown、错误绑定/parse、strict未知/重复/缺nullable/额外字段/尾随、已有输出/嵌入来源或输入/实际硬链接。复用Python stdlib独立SQLite只读快照和资源SHA，不调用Rust生产校验器作为oracle。非零Span及PDF依据实际登记资源测试，不声称任意PDF质量。每项输出保存实际命令/exit/report等文件fingerprint。

- [x] **Step 3: 实际原六输入只读核对。** 根提供original live root/6candidate/gold路径，不发模型请求；新case按原字节复制，review解释时间保持after_model_response、actor automated_source_review。比较source绑定/实际quote与gold范围/内容，记录允许context和必要组合概念，expected1结果真实通过或失败；不要通过改候选/gold补通过。

- [x] **Step 4: 更新标准并提供实际验证报告。** independent单元GREEN和矩阵普通CLI实际运行后填B04-T03，不写未跑数字。Task2提交脚本/测试/标准与真实小证据；最终完整Rust/Python、真实原源/六响应/模型保存配置+预算的前后不变、鲜审查由根执行并归档。脚本审查不重跑模型或解密凭据。

- [ ] **Step 5: 根最终完成。**

```text
./scripts/cargo_local.ps1 fmt --all --check
./scripts/cargo_local.ps1 check --workspace --locked --offline
./scripts/cargo_local.ps1 test --workspace --locked --offline -- --test-threads=4
python -m unittest discover -s tests -p "test_*.py"
python scripts/devlog.py check
git diff --check
```

只在实际本任务完整检查/独立原六核对/新鲜whole-branch review通过后用devlog complete附真实代码/验收产物，分步commit；`git push -u origin codex/b04-t03-knowledge-verify`后核对ls-remote与HEAD。B04R全部完成后最多展开下一批三项并batch，只规划不自动start。SDD暂存恢复归档保留；此前清理策略拒绝不重试或绕过。

## 根预检及审查安排

### 用户追加 Task 3：只保留一行窗口栏

用户的最新完整窗口截图显示第一行仍是带Noetome标题及原生按钮的Windows标题栏，第二行才是已实现的Codex菜单。用户明确要求“只要第二行”，因此先前B04U-T01保留原生decorations的选择不再适用。这一纠正在当前唯一活动任务内独立实现、验证和提交；不另行启动后续产品阶段。Task2审查修复只改Python/测试，Task3只改桌面前端及窗口配置，文件和运行资源分别约束，Git暂存/提交由控制代理串行协调。

**单一结果**：已安装Noetome窗口的顶部只有现有36px菜单栏，Windows的Noetome原生标题行消失；同一行右侧保留最小化、最大化/还原、关闭按钮，空白处可拖动窗口。

**Files:**
- Modify: `crates/noetome-desktop/tauri.conf.json`（main窗口decorations=false）
- Modify: `crates/noetome-desktop/capabilities/main.json`（仅main窗口所需的最小窗口操作权限）
- Modify: `apps/desktop/src/AppMenuBar.tsx`, `apps/desktop/src/styles.css`（现有菜单同行的拖动空白及窗口按钮）
- Create if needed: `apps/desktop/src/WindowControls.tsx`（有界窗口操作/失败反馈，不扩大现有App逻辑）
- Create: `doc/validation/B04-T03-ui-single-row.md`、`doc/validation/assets/B04-T03-ui-*`（实际RED/前端/原生窗口及安装证据）
- Root updates: `doc/standards/DESIGN.md`及本计划、最终完成日志；不改API/source/知识契约或桌面Rust窗口关闭偏好逻辑。

**Interfaces and acceptance:**
- 使用已锁定@tauri-apps/api2.12.4的getCurrentWindow及最小window权限；close仍触发现有CloseRequested和隐藏托盘/直接退出偏好，minimize保持任务栏语义。
- 拖动只来自菜单以外的空白区域，菜单、导航、窗口按钮及下拉项不能启动拖动。空白双击可最大化/还原。三个按钮保持可键盘使用的明文名称与焦点；还原图标/名称随真实窗口状态更新。
- 浏览器预览无Tauri宿主时窗口按钮禁用或隐去，不伪报原生操作成功；原生失败保留可见反馈。既有File/Edit/View/Help、历史导航和侧栏仍可用。
- 实机验收检查完整窗口及操作系统窗口装饰状态，不能只裁剪WebView菜单行作“一行”证据。记录修正前真实标题栏存在的RED，修正后完整顶部截图、拖动、最小化、最大化/还原、关闭/托盘与原菜单至少一条真实IPC。

**Sequence:**
- [x] Fresh implementer先保存现有配置/实际UI的单行目标RED，再做最小改动；只运行针对性TypeScript/前端检查，保留日志与源指纹。不得运行cargo或安装、修改独立Python、dev-log、凭据、预算；Git提交待root协调。
- [x] 新鲜Task3 spec+quality审查并修复必要问题，根执行实际NSIS构建/更新安装、完整窗口/操作验证与现有菜单回归，保存原生证据及被测程序指纹。
- [ ] 最终完整Rust/Python检查及most-capable整分支审查包含这一用户纠正；仅在知识验收和单行窗口栏均满足后complete当前卡、提交推送。随后最多规划下一批三项。

Task1→Task2仅外部CLI/严格JSON接口相依，文件责任不重叠，按顺序fresh implementer/fresh task reviewer。真实gold之前已存在，新语法只声明来源解释规则并明示时间；本范围拒绝排除源内已纳入事实。独立判据允许文本相同含义的受控标点/空白/条件连接符，不增大正式准确率声明。最终review使用most-capable模型只读精确提交diff/证据，禁止额外网络或生产资料访问。
