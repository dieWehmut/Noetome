# B06V-T03 最终能力与采集源码独立审查

审查结果：本任务新增的能力选择、独立来源校验和派生采集器保留严格门槛。没有发现以 profile 的 `verified`、支持标记、报告成功或版本范围代替来源语义核对的路径。审查只覆盖本任务明确列出的有界输出，不建立科学正确率、任意流程语法或未来应用版本的支持声明。

最终准备工具为 `scripts/prepare_visual_compatibility.py`，SHA256 `9c28fd119634c14d2c28d24451799175fe3fa9aceaeae10566f000e52767c83f`，68,922 字节；测试文件 SHA256 `324fa89215b147da96284f3e6d8a39cadc7ebceeef0bf10a25003449d2e1c796`，26,554 字节。最后一次变化只增加 `result.identity=identity;`，28 字节，保存已经实际读取并通过断言的配置目录、Vault 和插件值。原有所有权、安装、主题、Properties、Callout、标题及块链接检查完整保留。旧 b22 源及原捕获证据继续存在。

| 能力 | 最低版实测身份 | 当前版实测身份 | 核对位置与限制 |
| --- | --- | --- | --- |
| 原生公式、清晰扫描公式 | Obsidian 1.6.7 / MathJax 3.2.2 | Obsidian 1.15.0 / 实际 `window.MathJax.version` 4.1.3 | Rust `visual_capabilities::math` 与独立 Python `supports_native_math` 使用精确配对；原 native/verified、dfrac 和原图可用条件继续适用 |
| 有界流程 | Schema 1，Mermaid 10.6.1，HTML labels 开启 | Schema 2，Pele 1.8.0，HTML labels 关闭，Mermaid 版本显式 null | 精确 profile 身份、label codec、引擎名称/版本与 Pele 原脚本 SHA 均需匹配 |
| 有界 SVG 重绘 | Obsidian 1.6.7 | Obsidian 1.15.0 | 独立 SVG 实测后才分别登记；SVG flag、来源可读性、关系/几何/对比度和原图附件继续独立核对 |
| 表格、阅读格式及本地链接 | 干净 1.6.7 原生捕获 | 干净 1.15.0 原生捕获 | 使用冻结原 Markdown、真实字段控件/链接/附件观察，不从数学或流程引擎的能力表推断 |

MathJax 安装包中的静态 `VERSION=5.0.0` 与辅助版本字符串不能作为当前运行版本；登记值来自实际窗口 4.1.3 和实际执行脚本原字节。当前应用没有标准 Mermaid 全局或独立 `mermaid.min.js`，Pele 的版本不能填入 `mermaid_version`。最低版 Mermaid 全局没有暴露 `.version`，其 10.6.1 身份由安装成员的独立字面版本和实际执行 CDP 脚本 SHA 共同绑定。派生采集器只在实际非 null 全局版本存在时比较该值，仍强制实际脚本原件匹配，不把缺少全局版本当成默认成功。

当前流程引擎原件 SHA256 为 `9bf34b3a9a19351dc69d0d238e365560331a8ae392c41656df7f5090653253d0`。Schema 2 必须同时提供显式 null 的 Mermaid 版本和完整 `renderer{name,version,source_sha256}`；缺字段、null renderer、额外字段、大小写/长度错误 SHA、字符串或布尔 schema 都拒绝。Schema 1 禁止 renderer，即使该字段为 null 也不接受。Rust 的 wire/try_from 和独立 Python schema 采用同一严格边界，已测 app 与未测 engine 交叉配对保持回退。

公式和扫描仍先核对负号、下标、完整表达式及关键未知；图像仍先核对来源关系、原图可读性、形状和箭头；流程仍先核对来源条件、方向、决策完整性和连接关系。未经验证目标只改变明确的表达选择，不能把未知语义改为成功。新增 verifier `--target-profile` 先固定旧作者 baseline，再读取明确外部目标并独立重算决策；作者语义、正文、分母、源文件和成功库存不从目标报告生成。

本审查实际运行 8 项独立 registry/source 检查与 6 项 Flow Schema 2 检查，全部通过。另从冻结原产品输出建立临时取证副本，执行六项完整同步伪造：公式/图像的未知 99.0 profile、报告和输入 SHA 全重同步后仍拒绝虚假原生声明；未知 Pele 99.0 的 profile、plan、报告全同步仍拒绝流程图声明；已测目标仍拒绝负号、箭头头部和可见来源条件丢失。实际错误分别落在来源决策、完整显示意义、SVG 原件分母、流程问题码和完整语义正文检查，未用陈旧摘要或未更新指纹提前失败替代语义拒绝。

原命令、stdout/stderr、六项篡改原件及 19 个审查源文件见 [final-code-capability-audit.json](final-code-capability-audit.json) 和 [原件 ZIP](final-code-audit-originals.zip)，ZIP SHA256 `1a319aa760ce1dcdb527ab0bc4bd1f072d5ff35f30e0597300080c5e5041d025`。最后单行元数据变化的实际 21 项 focused tests、六类 JS syntax、diff 检查和新旧源见 [validation-05](preparation-tool-validation-05.json) 与 [原件 ZIP](preparation-tool-validation-05-originals.zip)，ZIP SHA256 `76c73a13ea001517e71962e177450587c3a1c646caa9588510eaca4265d9a46a`。这些准备/语法测试没有启动应用或请求模型；四项真实格式补测另按原生审查核对。

Pele 分支保留实际 `data-id`、文本、形状、线段及箭头三角原属性，独立核对方向、源靶边界、判据及促进/抑制标签，没有把新引擎 DOM 改写成旧 Mermaid ID。当前 SVG 使用原生 app-protocol 图片截图与同实例辅助图的完整 RGBA 对等；原生图片 src 不变。标签墨迹由同一内在尺寸 SVG 的全图与仅隐藏一个原 text 的截图差分取得，保留 nominal bbox、字体和全部原件，恢复后全 RGBA 必须与原图一致。source-associated 一像素上限没有扩大，来源 SVG 和独立 expected 没有修改。

最后完整 Windows Rust 原件独立重读得到 726 passed / 0 failed / 0 ignored，当前 1,825 项 Rust 依赖与 C 轮冻结源码逐字相等，实际最终 PE SHA256 为 `e42c534d07f66662411fc52f22561510b79ba1c63f14f8b6f419db7c4e0e3274`。原缓存并发测试修复只改 test helper：零容量 channel 先确认观察者就绪，原五秒 guard 期限从 release 后开始；真实 Win32 error 32、原注入及最终源库存断言保留，四种 mode 均有实际原 stderr。A 轮真实 exit 101 和后续限定诊断保留。[索引与 Rust 源审查](final-index-rust-source-audit.json) 不执行 Rust；Linux 完整 Rust test 为 not_run。

独立审查另外发现旧 19 CLI 的 ELF receipt 只有二进制 SHA 与 fixture/checker 源指纹，没有实际编译时 Rust 源前后锚，不能据此宣称编译源码绑定。父代理保留旧记录并安排使用最终 Windows C PE/源码强锚重跑这 19 项。全 Windows Python 新增 flow CLI test 复用冻结 Linux SQLite 的 import_path 被 Core 正确拒绝，后续只重建新的真实测试项目，不改冻结 DB 或产品路径保护。这些后续完整回归及提交验收由父代理独立记录，本文件不提前宣称其通过。

最终公开补证已读回：[final-capability-regression-readback.json](final-capability-regression-readback.json)直接核对WindowsC1,915编译原件/1,825依赖、实际build日志与e42c PE，新的30主+15设置CLI、7实质拒绝+2初步结构拒绝以及16份产品Markdown和全部相关资源成立。旧阶段ELF的编译源缺口按新的WindowsC证明补齐，旧原件不被推断或覆盖。最终9c prepare与42b flow测试在Windows441项和Linux第四380项全量中实际运行；前后源与PE/ELF清单相等，后续仅文案范围另记。主文档/合同最后审查及135本地链接见 [final-document-closeout-audit.json](final-document-closeout-audit.json)，未发现剩余源码、语义门槛或事实/链接问题；完整Linux Rust和科学验收仍not_run。
