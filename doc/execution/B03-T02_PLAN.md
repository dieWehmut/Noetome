# B03-T02 单份真实文本PDF实施计划

> 执行方式：使用subagent-driven-development按当前任务内分工，Root持有账本、共同宿主、CLI和最终验收；各组件先实际行为红，再实现、审查、定向绿及统一检查。

**目标**：真实Python进程把明确选取的单份文本PDF解析到新测试根，完整页清单、正文及真实区域可独立复查；不登记项目。

**架构**：独立PDF v2 DTO和字形证据层复用现有有界传输/快照/资源保护；Core依据原始页盒、字形、实际资源重算正文与区域。作者样例和Poppler回看另外证明有限实际定位。

**技术**：Rust 1.98.1 Windows x64、选定Python3.12.14、唯一pdfplumber adapter；精确依赖按官方/本机核查。ReportLab/Poppler仅作者和独立回看工具。

**规范**：[PDF协议v2](../technical/PDF_PARSE_PROTOCOL.md)、[近期任务卡](NEXT_STEPS.md)、[测试标准](../standards/TESTING.md)。当前任务已授权自动实现与普通提交推送；无需把既有设计/默认值再次作为审批障碍。

**实际收束**：全部实现步骤及独立审查、10组PDF行为、22项模型、6项新增平台保护和统一361项Rust/12项Python检查已执行，真实结果见[B03-T02验收](../validation/B03-T02.md)和[机器证据](../validation/B03-T02.json)。原步骤清单保留设计顺序，不承担日志状态职责。审查中的报告取消缺口使最终实现增加`.parse.pending.json`与保持句柄、不覆盖的原子交付；native退出码0/1/2分别按实际观察记录。项目PDF登记仍未启动。

## 固定职责与交付接口

| 执行者 | 文件 | 接口/责任 |
| --- | --- | --- |
| Root | sidecar.rs、pdf_parse.rs、lib.rs、main.rs | 抽出内部InputSnapshot/Workspace复用；v1行为保持；parse_pdf编排、真实runtime路径、报告/退出码 |
| Rust模型分工 | pdf_model.rs | 上述全部严格DTO；validate_pdf(request,response,resource_bytes,limits,cancel)返回结构化问题，复算坐标/正文/状态 |
| adapter分工 | sidecars/pdf_parser/adapter.py、requirements.txt、runtime-lock.json、README.md | 真实PDF primitives/逐页失败、确定资源、单JSON stdout、受限诊断，精确runtime/依赖/许可证 |
| 作者/测试分工 | fixtures/pdf-text-v1/、tests/pdf_parse.rs、tests/support/pdf_fake.rs | 作者固定文字/坐标/原始SHA，实际CLI/篡改/保护和有限支持矩阵；fake仅协议负例并标记测试 |

PdfParserSpec只有python/adapter两条可信程序路径。PdfLimits含transport: SidecarLimits和policy: PdfPolicy。数据DTO和行/坐标规则以协议文件为准；跨文件不另起名称。v1的Artifact/InputFingerprint/ArtifactPolicy/WorkspacePolicy/Producer/ResponseStatus/UnitStatus可共享严格类型，v2页结构独立。

## 步骤一：作者预期和真红

- [ ] 核查官方固定版本、许可证、依赖和Python隔离调用，保存实际元数据及环境探针；不以这些探针证明PDF性能。
- [ ] PDF技能author marker成功一次后，在fixtures/pdf-text-v1生成自造样例。作者先固定文字、页数、原始用户空间/CropBox/Rotate/UserUnit及独立区域窗口，再保存原始SHA；不调用待测adapter生成expected。
- [ ] Poppler渲染正常页并人工回看文字/順序/位置；异常页记录独立页数/解码输出及不能渲染的真实原因。
- [ ] 对旧CLI执行`source parse-pdf`期待成功，实际退出2形成命令真红；模型固定字形/资源与正确区域期待通过，stub验证形成行为红。DTO未知/重复字段、数组/枚举形态及必填null分别拒绝。

## 步骤二：有界接线与真实PDF

- [ ] sidecar内部抽出选定输入守卫及Workspace对Artifact清单的通用复核，v1原始字节契约不扩展。新根拥有者与parse.json独占写入有最终身份/SHA复核。
- [ ] adapter以`-I -B`启动，严格读取单请求，核对实际输入完整SHA；完整Page树枚举后才宣告页数，逐页读真实字形/原始Page attrs，保留解码失败页。
- [ ] 保存固定inventory与每页文本资源；明确视觉/顺序/无文本问题。当前没有成功OCR、表格/公式或图片语义标志。
- [ ] Core严格解码v2及inventory，复核原始/协议/producer/运行时身份、页数双向清单、页盒四角变换、字形覆盖、逐行文字与实际UTF8字节、资源引用和问题状态。
- [ ] 写Host parse.json并重读，stdout使用同一结果；实际partial/failed返回3，输入/协议/路径2，内部或stdout1。路径/资源失效时保存真实现场，不附成功页。
- [ ] 分组件通过真实红绿后提交实现，避免多个代理编辑lib.rs、main.rs或账本。

## 步骤三：故障、支持矩阵和结束

- [ ] 实际单页/多页单栏逐字、全部物理页、页内真实区域窗口及渲染回看通过。修改漏页/错页、实际页内错误bbox、缺正文/乱序、UTF8范围或资源SHA必须失败；expected不改来迁就结果。
- [ ] 扫描、空白、损坏、加密、表格、公式、图片、多栏、旋转、非对称CropBox/非零原点、UserUnit!=1及真实混合页解码失败分别保存支持状态与原始证据。
- [ ] 实际Python超时/取消/非零失败与Host输入/资源超限、未知文件、已有根、链接/junction、硬链接及外部哨兵保护通过，继承v1进程树回归。
- [ ] 从真实CLI测总耗时、输入/资源字节、文件/页/字形/区域数量；只声明当前样例。库import时间与默认上限不冒充PDF性能。
- [ ] 运行cargo fmt/check/test workspace locked/offline、Python unittest、devlog check、git diff --check；修复实际失败，保存源码/样例/检查指纹与未运行范围。
- [ ] 独立审查修复后complete B03-T02，提交标准/验收/日志，push codex/b03-t02-text-pdf并核对远端HEAD。T03仍todo，不自动启动。

## 可复现入口

```powershell
./scripts/cargo_local.ps1 test -p noetome-cli --test pdf_parse --locked --offline
./scripts/cargo_local.ps1 test -p noetome-cli --lib pdf_model --locked --offline
cargo fmt --all --check
./scripts/cargo_local.ps1 check --workspace --locked --offline
./scripts/cargo_local.ps1 test --workspace --locked --offline
python -m unittest discover -s tests -p 'test_*.py'
python scripts/devlog.py check
git diff --check
```

python指本轮实际选定运行时，仓库不写死开发机解释器路径。规划勾选不代替命令结果、产物或实际页面验收；失败和修正历史持续保留。
