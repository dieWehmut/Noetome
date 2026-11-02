# 近期开发步骤

真实状态以[任务账本](../../dev-log/tasks.json)和[当前状态](../../dev-log/STATUS.md)为准。B02三项已分别验收，执行卡归档于[B02步骤](B02_STEPS.md)，闭环证据见[B02-T03](../validation/B02-T03.md)。B03-T01交付有界单请求sidecar的真实fake验证；B03-T02交付单一pdfplumber的真实PDF解析沙盒、页/区域Core复核和只读重查，证据见[B03-T02](../validation/B03-T02.md)。下一入口B03-T03仍未启动：显式项目Schema升级、PDF来源登记与解析缓存复用尚未实现。

**目标**：用一个有界 parser sidecar 将单份真实文本 PDF 变为可追溯的页与区域来源，在项目内安全登记并复用；每项交付独立验收。

**架构**：Rust Core 负责输入能力、进程生命周期、严格输出验证、来源身份和持久化；测试 fake 先固定进程契约，随后只接一个 Python 文本 PDF 解析器。真实解析先在全新沙盒完成，最后才升级来源项目的数据契约并接入 import/inspect。

**技术与依据**：已有 Rust CLI/SQLite/Windows x64；B03-T02实测锁定Python3.12.14、pdfplumber0.11.9/pdfminer.six20251230及全部运行依赖。只消费已提交可信payload的Rust构建不自动下载/安装Python；真实PDF测试需明确配置锁定运行时。本计划依据 [PRD FR-01/02/03/06](../requirements/PRD.md)、[已采纳 D02/D12/D13/D14](../requirements/DECISIONS.md)、[B03 路线](ROADMAP.md)、[Sidecar 架构](../technical/ARCHITECTURE.md)、[数据契约](../technical/DATA_CONTRACTS.md)、[外部能力引入门槛](../standards/TESTING.md)及[任务模板](../templates/TASK.md)。

## 共同边界

- 每次只推进一项，依赖验收后才能 start；B03-T01/T02的实施和失败修复保留真实记录，B03-T03必须另轮start。真实状态以任务账本为准；完成当前任务不自动启动下一项。
- 本批不接扫描 OCR、模型/API、知识抽取、项目 GUI、生产 Vault、完整多格式适配或通用远程任务总线。扫描及复杂页仍须作为失败/需复核边界样例，不能据此宣称支持。
- 只读用户明确提供的单文件，输出只在本轮明确且独占的范围内。资料中的链接和操作指令按内容处理，不能决定可执行程序、扩大文件范围或触发外发。受控 sidecar 的协议边界不等于已建立操作系统级沙盒。
- 当前来源 `project.sqlite` 为 Schema2；当前编译 `state/compile.sqlite` 为独立 Schema1。PDF 登记所需的下一项目 Schema 必须显式设计，不能修改 Schema2 的含义，也不能混用编译库版本。
- 页/区域定位、提取文本范围、原始 PDF 字节指纹是三个不同对象。旧 UTF8 `start_byte/end_byte/start_line/end_line` 只描述旧规范文本；PDF 的原生来源定位必须包含真实页和区域。
- fake 协议、真实解析、独立审阅、项目持久化分别保存证据。未来门槛和配置上限不是已测性能；不得以小样例替代 PRD 的知识召回、科学准确率、1000 页或 24 小时指标。

## B03-T01 用 fake 子进程验证有界单请求 parser sidecar 协议

**单一可见结果**：Rust 调用者能与实际 fake 小进程完成一次有界请求，或返回可复现的分类失败且结束进程。

**依赖与范围**：B02-T03 验收完成；部分覆盖 FR-06/36/40、NFR-14。只交付严格协议与进程宿主，不引入 PDF Python 库。fake helper 是测试工具，生产 CLI 不暴露 fake 开关。

**实际模块职责**：Rust `sidecar_model.rs` 定义严格版本化DTO，`sidecar_process.rs` 管理原生argv、并发管道、限额与生命周期，`sidecar_workspace.rs`保持输入/目录/资源守卫。库测试编译`tests/support/sidecar_tests.rs`和标准库fake helper；`tests/sidecar.rs`独立证明普通入口拒绝fake。准确接口见[协议](../technical/SIDECAR_PROTOCOL.md)。

**消费与交付接口**：输入是Core验证并保持守卫的明确单文件或本轮自有只读快照，加上request_id、原始SHA/长度和新工作区能力；交付严格响应或稳定错误。响应包含同一request_id、协议版本、producer、解析器/能力版本、完整来源单元、相对资源清单和问题。fake的合成单元与资源文本范围只证明协议检查，不定义PDF物理页或bbox。单个进程只接一个请求；关闭stdin后，stdout必须是唯一JSON，允许末尾空白，退出码也必须成功。

**实际验收**：79项宿主边界用例和3项普通入口通过，全workspace329项Rust回归、12项Python、格式、locked离线check及日志检查通过。真实进程负例先红再实现；Windows根/子/孙PID先确认存活再确认退出，涵盖超时、取消、输出超限、父进程早退及独立终止兜底。Core复核输入、资源字节/身份/清单和路径哨兵。独立审查的重复UTF8扫描、主终止失败阻塞兜底、BrokenPipe延迟三项均先复现后修复；复审重新链接普通库探针并实跑关闭三项。实施步骤归档于[实施方案](B03-T01_PLAN.md)，真实命令、失败与未运行项见[验收](../validation/B03-T01.md)。

**停止**：协议进程组通过且边界证据齐全即结束。本项不启动真实解析器；下一入口 B03-T02 必须另行 start。

## B03-T02 解析单份真实文本 PDF 并验证完整物理页与区域定位

**单一可见结果**：一个明确选定的真实文本 PDF 经 Python 进程解析，在全新测试工作区获得能回到真实页面的正文、完整页清单、区域及明确问题。

**依赖与范围**：B03-T01；部分覆盖 FR-02/03/06、NFR-01/04/14。普通 CLI `source parse-pdf <PDF> <尚不存在的测试工作区>` 和只读 `source inspect-pdf <测试工作区>` 已实跑。新根拒写既有路径，不登记项目数据库。唯一文本解析器为pdfplumber，扫描/复杂页保存问题与原始证据，不报完整解析或OCR成功。

**实际模块职责**：`sidecars/pdf_parser/` 保存唯一Python adapter、SHA绑定压缩payload、精确依赖和许可；`pdf_model.rs`固定严格DTO，`pdf_validate.rs`复核物理页/字符/区域/文本/状态，`pdf_workspace.rs`维护新根和报告守卫，`pdf_parse.rs`复用sidecar全过程控制并保存/重查报告。`tests/pdf_parse.rs`、`tests/support/pdf_parse_tests.rs`/`pdf_validate_tests.rs`保存真实CLI、过程边界和Core负例；`fixtures/pdf-text-v1/`及独立验证脚本保存作者输入/位置预期。准确接口见[PDF契约](../technical/PDF_PARSE_CONTRACT.md)。

**消费与交付接口**：消费已验收的有界 sidecar 请求。交付完整原始 PDF SHA/长度、parser/协议/选项版本、物理页清单、每页提取文本、区域、资源与问题；所有提取文本/资源均绑定实际 artifact 的相对路径和 SHA。整体存在 failed 或 needs_review 页时不报整份解析成功，仍可保留已确认范围。

**实际验收要点**：唯一运行时按hash锁定后在新venv实际安装；14自造PDF有独立原始操作符/字形/正文预期和25页Poppler渲染。独立29次普通CLI核对24确认物理页、1025字形、15行完整正文、全部资源SHA与只读往返。页索引0基、显示页索引+1，印刷页标签保持null；UTF8半开范围绑定逐页artifact，区域保留真实CropBox/MediaBox/旋转/pt矩阵。扫描、未知空白、视觉关联和不可靠顺序需复核，损坏/加密/部分页异常failed。

**故障与审查**：真实CLI/定位负例先红；Core同时检查精确页清单、字符包络/分配、全文、语义及路径引用，真实进程测试覆盖限额、超时/取消、占用/硬链接/junction和新根保护。初次审查的错开栏、纯空格和ToUnicode边界空白三项P2已独立复现、红绿修复和原四PDF复审关闭。最终PID握手测试竞态也稳定先红后绿；364 Rust/43 Python、离线check和格式检查通过。过程/语义/真实样例及未运行项分别归档，不用小样例声明性能或科学质量。

**停止**：有限真实文本解析及诚实的失败边界通过即结束；不宣称知识覆盖/科学质量，也不写项目 Schema。下一入口 B03-T03 独立 start。

## B03-T03 安全登记单份 PDF 来源版本及解析尝试并验证复用与失败保护

**单一可见结果**：项目中能 import/inspect 单份 PDF 的原始版本和真实解析尝试，同字节可复用，失败或升级中断不会覆盖旧证据。

**依赖与范围**：B03-T02 和 B02-T01；部分覆盖 FR-01/02/06/35/40、NFR-07/08/14。只扩展来源登记，不创建知识 IR、NotePlan、Run/Release 或 GUI。Schema 迁移是该行为的必要安全前提，同属于本项；本项开始后先审查迁移方案再修改 writer。

**预期模块职责**：复用 `source.rs` CLI、`source_model.rs`、`source_schema.rs`、`source_write.rs` 的来源身份/候选事务/严格重读边界；只在职责需要时拆出 PDF parse 持久模型。新增精确 Schema 和快照布局说明、迁移与 `tests/pdf_source.rs` 进程断言；更新既有 `project.rs` 只读版本路由。不得用修改旧 Schema2 的枚举绕过升级。

**消费与交付接口**：消费 B03-T02 经 Core 验证的解析结果，将原始字节版本与解析尝试/结果分开存储。`source import <项目> <PDF> [--source-id <既有ID>]` 和 `source inspect <项目> <import-id>` 延续现有身份与审计语义，独立报告版本明确表达原始获取、PDF 解析、pending 知识状态。原始版本复用按 raw SHA；解析缓存另外绑定 raw SHA、parser、协议及影响输出的选项版本。

1. 编码前冻结 Schema2 到下一显式项目 Schema（候选3）的精确表/约束、JSON版本、状态、快照布局、迁移和回退。旧表/记录及 UTF8 语义保持可重读；PDF原始字节、页/区域索引、文本 artifact 与资源另有准确契约。旧 `ready` 必需 normalized.txt、旧 failed 仅 UTF8错误的规则不能被隐式放宽。
2. 先写真实迁移/登记红灯。使用新建 Schema1/Schema2 测试项目验证合法升级、旧项目文本历史保存、未知版本或坏记录拒绝；保持 project_id/created_at、Source/Version/Span/Import身份及内容指纹。候选 SQLite 事务和配对配置完全复核后才发布；inspect只读、不自动升级。
3. 登记一份已确认 PDF 原始版本和解析尝试，并从持久快照重读页/区域/资源，不只返回内存 sidecar 成功。输入路径供本地审计，原文件移走仍可检查已保存的原始与解析证据；不扫描未选取资料。
4. 固定重复语义并实跑：同字节/重命名复用原始来源版本；内容换版保留历史；回到旧字节复用旧版本；显式 source_id 不能按跨来源相同 SHA 合并。每次 import 有新身份，相同成功 parser/协议/选项可复用解析结果；版本或选项变化产生新 parse 身份，旧区域不重写。
5. 完整 raw 已存但解析失败时记录失败 parse/未完成页，保留可重试原始版本；raw 未完整读取时只记录失败尝试，没有 version_id、完整SHA、空 normalized 或假 locator。失败缓存不得强制阻止后续有效重试；失败后再导入有效文件仍可成功。
6. 先用候选写入阶段的实际故障测试升级/登记中断、数据库或文件占用、配置配对失败、未知/替换缓存和路径/junction；不报成功，不覆盖人工内容，保留明确待恢复现场。此处只承诺测过的受控故障，不承诺全部断电场景自动恢复。
7. 真实 CLI核对每次报告与数据库、原始/解析快照的状态和计数一致；failed/needs_review不算解析完成，knowledge继续pending。完成PDF重复/失败/迁移用例，已有MD/TXT、fixture、桌面/帮助的适用回归和统一检查后保存证据、更新标准、提交推送。

**停止**：三项各自真实验收后才收束 B03；按事实滚动规划后续，不自动启动 B04 API 或扫描 OCR。若遇到不能满足当前文本 PDF 定位的解析器限制，保留 failed/needs_review 和可复现原因，先处理本批问题，不叠加其他格式。

## 执行时的验证与交接

sidecar宿主与真实pdf_parse/统一检查已分别在B03-T01/T02实际执行。`pdf_source`仍是B03-T03后续门槛，文件未创建也未运行，应在对应任务创建后执行。fake、真实parser、持久化分别记录，不以统一回归替代定位抽查。下面包含后续门槛命令，不能把列出命令视为运行。

```powershell
./scripts/cargo_local.ps1 test -p noetome-cli --lib sidecar_process::tests --locked --offline
./scripts/cargo_local.ps1 test -p noetome-cli --test sidecar --locked --offline
./scripts/cargo_local.ps1 test -p noetome-cli --test pdf_parse --locked --offline
# B03-T03尚未创建/执行：
./scripts/cargo_local.ps1 test -p noetome-cli --test pdf_source --locked --offline
cargo fmt --all --check
./scripts/cargo_local.ps1 check --workspace --locked --offline
./scripts/cargo_local.ps1 test --workspace --locked --offline
python -m unittest discover -s tests -p 'test_*.py'
python scripts/devlog.py check
git diff --check
```

Python指本轮选定的可用解释器；使用工作区运行时时记录工具返回的绝对路径，不将开发机路径写死进仓库。真实 parser 环境单独锁定并说明安装步骤，普通 fake 测试不触发下载。每项记录实际版本、命令、退出码、数量、失败与未运行项、原始样例SHA、独立预期与产物；仅这些真实证据满足任务后才能 complete。

本批T01/T02产物和实际验收分别归档；下一入口B03-T03要先冻结显式Schema升级方案和真实迁移红灯。执行者每轮先读日志、start单一任务，完成该项真实验收后complete并提交推送；T02验收后不在同轮start下一项，也不切换批次。
