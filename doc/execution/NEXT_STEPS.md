# 近期开发步骤

真实状态以[任务账本](../../dev-log/tasks.json)和[当前状态](../../dev-log/STATUS.md)为准。B02三项已分别验收，执行卡归档于[B02步骤](B02_STEPS.md)，闭环证据见[B02-T03](../validation/B02-T03.md)。B03-T01的有界fake协议已验收，证据见[B03-T01](../validation/B03-T01.md)；T02真实PDF已按[实施计划](B03-T02_PLAN.md)接线并执行有限样例验证，最终验收以账本及验证记录为准。T03项目Schema升级与PDF登记仍未开始；没有运行规模性能基准。

**目标**：用一个有界 parser sidecar 将单份真实文本 PDF 变为可追溯的页与区域来源，在项目内安全登记并复用；每项交付独立验收。

**架构**：Rust Core 负责输入能力、进程生命周期、严格输出验证、来源身份和持久化；测试 fake 先固定进程契约，随后只接一个 Python 文本 PDF 解析器。真实解析先在全新沙盒完成，最后才升级来源项目的数据契约并接入 import/inspect。

**技术与依据**：已有 Rust CLI/SQLite/Windows x64；Python 运行时和唯一 PDF 库在 B03-T02 按实际试验选择及锁定。本计划依据 [PRD FR-01/02/03/06](../requirements/PRD.md)、[已采纳 D02/D12/D13/D14](../requirements/DECISIONS.md)、[B03 路线](ROADMAP.md)、[Sidecar 架构](../technical/ARCHITECTURE.md)、[数据契约](../technical/DATA_CONTRACTS.md)、[外部能力引入门槛](../standards/TESTING.md)及[任务模板](../templates/TASK.md)。

## 共同边界

- 每次只推进一项，依赖验收后才能start；T01已验收，T02按其实际账本状态交付，T03尚未start。本轮不自动启动下一项。
- 本批不接扫描 OCR、模型/API、知识抽取、项目 GUI、生产 Vault、完整多格式适配或通用远程任务总线。扫描及复杂页仍须作为失败/需复核边界样例，不能据此宣称支持。
- 只读用户明确提供的单文件，输出只在本轮明确且独占的范围内。资料中的链接和操作指令按内容处理，不能决定可执行程序、扩大文件范围或触发外发。受控 sidecar 的协议边界不等于已建立操作系统级沙盒。
- 当前来源 `project.sqlite` 为 Schema2；当前编译 `state/compile.sqlite` 为独立 Schema1。PDF 登记所需的下一项目 Schema 必须显式设计，不能修改 Schema2 的含义，也不能混用编译库版本。
- 页/区域定位、提取文本范围、原始 PDF 字节指纹是三个不同对象。旧 UTF8 `start_byte/end_byte/start_line/end_line` 只描述旧规范文本；PDF 的原生来源定位必须包含真实页和区域。
- fake 协议、真实解析、独立审阅、项目持久化分别保存证据。未来门槛和配置上限不是已测性能；不得以小样例替代 PRD 的知识召回、科学准确率、1000 页或 24 小时指标。

## B03-T01 用 fake 子进程验证有界单请求 parser sidecar 协议

**单一可见结果**：Rust 调用者能与实际 fake 小进程完成一次有界请求，或返回可复现的分类失败且结束进程。

**依赖与范围**：B02-T03 验收完成；部分覆盖 FR-06/36/40、NFR-14。只交付严格协议与进程宿主，不引入 PDF Python 库。fake helper 是测试工具，生产 CLI 不暴露 fake 开关。

**实际模块职责**：Rust `sidecar.rs`负责输入能力、独占快照及最终资源复核，`sidecar_model.rs`定义严格版本化 DTO，`sidecar_process.rs`管理原生 argv、并发管道、限额与生命周期；小平台crate独立封装Windows Job/句柄。`tests/sidecar.rs`为harness=false target，fake helper在tests/support私有自入口。接口见[协议v1](../technical/SIDECAR_PROTOCOL.md)，过程及裁定见[T01计划](B03-T01_PLAN.md)。

**版本裁定**：本项v1只验证input_bytes原始分区，不伪装PDF页或正文坐标。后续T02复用已验收进程宿主和资源保护，显式定义PDF协议版本与locator并重跑严格验证，不能扩大v1含义。

**消费与交付接口**：输入是 Core 验证并保持守卫的明确单文件或本轮自有只读快照，加上 request_id、原始 SHA/长度和新工作区能力；交付严格响应或稳定错误。响应包含 request_id、协议版本、producer、解析器/能力版本、完整来源单元、相对资源清单和问题。本项只检查原始 input_bytes 分区，不包含页坐标；PDF定位留给T02的显式新协议。单个进程只接一个请求；关闭 stdin 后，stdout 必须是唯一 JSON，允许末尾空白，退出码也必须成功。

1. 先写并实际运行 fake 进程负例，确认失败来自所要求的行为而非编译/setup：启动失败、空/截断/错误响应、额外 stdout、未知字段/版本、身份/指纹不符、合法 JSON 配非零退出和崩溃。日志必须走 stderr。
2. 固定请求、响应、诊断、运行时限的确定值及可注入测试配置；stdout/stderr 同时有界排空，避免填满管道死锁。超过任何上限必须失败并保留错误，不能截断后继续使用结果。默认值是协议政策，不是实测容量。
3. 在 Windows x64 明确受控进程树管理策略；真实 fake 子/孙进程握手后分别测试超时、取消、输出超限，终止并 wait，确认后代也退出。仅测试 `Child.kill()` 不足以证明树已清理；不把此项称为完整持久任务恢复或睡眠恢复。
4. Core 独立复核路径、引用、实际资源 SHA/长度/身份、完整清单和工作区所有权。负例覆盖 `..`、绝对路径、Windows 名称/碰撞、链接/junction、未知文件及既有根；哨兵字节不变。响应路径不能作为任意文件读取能力。
5. 运行定向进程组与适用统一检查，记录真红/绿、实际进程退出、错误码及 Windows 环境；同步协议和标准后审查提交推送。

**停止**：协议进程组通过且边界证据齐全即结束。本项不启动真实解析器；下一入口 B03-T02 必须另行 start。

## B03-T02 解析单份真实文本 PDF 并验证完整物理页与区域定位

**单一可见结果**：一个明确选定的真实文本 PDF 经 Python 进程解析，在全新测试工作区获得能回到真实页面的正文、完整页清单、区域及明确问题。

**依赖与范围**：B03-T01；部分覆盖FR-02/03/06、NFR-01/04/14。实际CLI为`source parse-pdf <PDF> <尚不存在的测试工作区> --python <绝对解释器路径> [--timeout-ms <0..30000>]`，不登记项目数据库。唯一pdfplumber adapter与PDF v2严格契约、实际模块及报告发布步骤见[T02计划](B03-T02_PLAN.md)和[PDF协议](../technical/PDF_PARSE_PROTOCOL.md)。扫描/复杂页用于支持矩阵，不同时开发OCR。

**预期模块职责**：在 `sidecars/pdf_parser/` 保存唯一 Python adapter、精确依赖与许可证说明；Rust `pdf_parse.rs` 负责候选来源 DTO、Core 验证和新根输出，复用 sidecar 宿主；`tests/pdf_parse.rs` 保存真实 CLI/篡改断言，`fixtures/pdf-text-v1/` 保存自造/授权样例及独立审阅说明。路径仅为实施候选，不在本轮创建。

**消费与交付接口**：消费已验收的有界 sidecar 请求。交付完整原始 PDF SHA/长度、parser/协议/选项版本、物理页清单、每页提取文本、区域、资源与问题；所有提取文本/资源均绑定实际 artifact 的相对路径和 SHA。整体存在 failed 或 needs_review 页时不报整份解析成功，仍可保留已确认范围。

1. 开始时核对候选库官方接口、许可证、安装与 Python 支持，实际试验后只锁定一个 adapter。记录真实 Windows x64/Python/库版本；不能直接将候选宣传能力列为支持。普通协议 fake 回归不依赖该库。
2. 先固定一页和多页单栏真实 PDF 的原始字节、页数、关键全文、阅读顺序与实际位置审阅预期，再取得真实 CLI 红灯。样例不含私密资料，预期不得从待测 parser 的输出生成；原 PDF 不修改。
3. 固定 `physical_page_index` 为 0 基，完整范围为 `[0,page_count)`，显示物理页为索引加一；`printed_page_label` 只有实际可验证依据才填，否则 null。PDF 页标签不自动等同教材印刷页号。
4. 固定 `pdf_page_region` locator 的 pt 单位、原点/Y轴、页面边界、CropBox/MediaBox与旋转变换。bbox须有限且在页内；无法可靠转换的页进入 needs_review。提取文本的 UTF8 半开范围必须同时指明 page/artifact/SHA，不能把 PDF raw offset、旧 `blocks-v1` 或文本行号当原生 PDF 区域。
5. Core 校验原始指纹、全部页索引、区域和文本/资源引用及真实字节。实际页面回看独立核对有限样例的关键正文、顺序和区域；再篡改漏页/错页、错误 bbox、缺正文/乱序、引用和资源 SHA，要求失败。仅检查 bbox 在页内不足以证明位置正确。
6. 小型支持矩阵分别实测扫描无文本层、空白、损坏/加密、表格、公式、图片、多栏和旋转页。无文本不自动等于合法空白；检测能力不足的视觉/顺序状态明确 needs_review，页面与原始证据仍在清单中。未支持的结构不得被普通文本拼接伪装为已正确保留。
7. 实际测试输入未读全/变化/超限、进程超时/取消、部分页失败和路径保护。保存 ParseOutcome 与可重读的范围/资源事实，原输入与外部哨兵不变。完成定向真实 CLI、定位/支持矩阵与适用统一检查，记录实际时间/资源及未运行项后提交推送。

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

T01的sidecar定向组及适用统一检查已经实际运行，T02的pdf_parse target已接真实Python及冻结作者样例；数量与结果见各项验证记录。pdf_source仍待T03创建，不能提前算执行。fake、真实parser、持久化测试分别记录，不以统一回归替代定位抽查。

```powershell
./scripts/cargo_local.ps1 test -p noetome-cli --test sidecar --locked --offline
./scripts/cargo_local.ps1 test -p noetome-cli --test pdf_parse --locked --offline
./scripts/cargo_local.ps1 test -p noetome-cli --test pdf_source --locked --offline
cargo fmt --all --check
./scripts/cargo_local.ps1 check --workspace --locked --offline
./scripts/cargo_local.ps1 test --workspace --locked --offline
python -m unittest discover -s tests -p 'test_*.py'
python scripts/devlog.py check
git diff --check
```

Python指本轮选定的可用解释器；使用工作区运行时时记录工具返回的绝对路径，不将开发机路径写死进仓库。真实 parser 环境单独锁定并说明安装步骤，普通 fake 测试不触发下载。每项记录实际版本、命令、退出码、数量、失败与未运行项、原始样例SHA、独立预期与产物；仅这些真实证据满足任务后才能 complete。

T01已完成行为验收；T02最终验收、账本与提交推送完成后停止。下一入口为B03-T03，仍为todo；执行者每轮先读日志、start单一任务，完成真实验收后complete并提交推送。T03依赖T02，不能提前开始。
