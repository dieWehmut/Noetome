# 固定技术栈与依赖锁定

本文件是V1选型权威，不是安装或测试报告。2026-10-31至2026-11-01核对了官方文档和包注册表；下表固定库及兼容版本系列。首次引入该能力时锁定该系列中通过构建/fixture的补丁版本并提交锁文件，不使用浮动latest，不擅自换库或升主版本。0.x库按表内minor系列固定，因为minor可能不兼容。

## 1 平台与构建

| 项目 | 固定选择 | 锁定与交付 |
| --- | --- | --- |
| 首个支持目标 | Windows 11 24H2及以上，x64，NTFS | 发布前在最低版本及当前版本实测；不宣称Win10/ARM已支持 |
| Rust | stable 1.99系列，edition 2024，MSVC x86_64-pc-windows-msvc | rust-toolchain.toml写确切patch，Cargo.lock入库；官方stable已核到1.99.1 |
| Python | CPython 3.12系列 | .python-version及uv锁定；用户安装包不要求系统Python |
| Python依赖管理 | uv 0.13系列 | uv.lock提交，CI uv sync --locked |
| Node开发运行时 | Node 24 LTS | 精确patch写版本文件；官方已核到24.22.2，不随产品交付 |
| 前端包管理 | pnpm 10系列 | packageManager写确切patch，apps/desktop/pnpm-lock.yaml；已核到10.34.9 |
| 安装 | Tauri 2 NSIS per-user x64 | 固定WebView2离线运行资源、parser与OCR资源，安装/更新见RELEASE |
| C/C++依赖 | MSVC及vcpkg manifest | 为Tesseract固定vcpkg baseline/overlay与源码hash，记录编译工具链 |

依赖只在用到的批次引入。先提交最小可编译依赖集，再引入下一个能力，不在B01安装整个桌面/PDF栈。Cargo和uv使用--locked、pnpm使用--frozen-lockfile，防止源码与锁文件不一致；离线测试需要预先缓存依赖，但不需要访问模型/搜索服务。

## 2 Rust库

| 职责 | 固定库/系列 | 具体决定 |
| --- | --- | --- |
| CLI | clap 4 | derive子命令，稳定帮助及退出码 |
| JSON | serde 1、serde_json 1 | 显式类型、拒绝未知字段；入口另拒重复键 |
| 异步与取消 | tokio 1、tokio-util 0.7 | worker调度、命名管道、CancellationToken |
| SQLite | rusqlite 0.41，bundled/backup/functions | 随应用SQLite，foreign_keys/WAL/事务、在线备份；不用SQLx/ORM |
| HTTP/TLS | reqwest 0.13，rustls | OpenAI兼容Chat Completions、Brave和有界页面抓取；无SDK额外网关 |
| ID与内容hash | ulid 3、sha2 0.11 | ULID身份、SHA256原始字节/规范JSON |
| 错误与日志 | thiserror 2、tracing 0.1、tracing-subscriber 0.3 | 类型化错误、脱敏JSON日志 |
| 临时文件 | tempfile 3 | 同卷临时候选、关闭失败保留恢复现场 |
| 文件变化观察 | notify 8 | 仅触发复查；事件丢失靠发布前hash补偿 |
| Markdown AST | pulldown-cmark 0.13 | 本文/表格/代码块等结构解析，Wikilink另外明确解析 |
| JSON Schema | jsonschema 0.60 | Schema版本化，模型候选二次校验 |
| 差异 | similar 3 | B/C/N文本diff，不能自动替用户合并 |
| Windows原生能力 | windows 0.62 | 文件句柄、Job Object、SID/DACL、Credential Manager |

未列入的纯工具小依赖也须说明用途、许可、锁定和最小测试；不得用“依赖替换”引入第二数据库、服务框架、向量库、Agent框架或新语言运行时。SQLite FTS5用于本地来源检索，中文双字索引规则见KNOWLEDGE_PIPELINE，不引入向量数据库。

## 3 PDF与OCR主路径

| 职责 | 固定库/系列 | 实施约束 |
| --- | --- | --- |
| 文本/字符/表格几何 | pdfplumber 0.11 | 读取页/字符/线段/表格候选，保留原区域和阅读顺序；不把抽取顺序当学科正确 |
| PDF渲染与裁剪 | pypdfium2 5 | PDFium渲染，保存原图/原区域；不用PyMuPDF |
| OCR | Tesseract 5 + pytesseract 0.3 | OCR显式页/区域，默认eng+chi_sim，300DPI；结果带置信/失败标记 |
| 图像处理 | Pillow 13 | 固定色彩/裁剪/尺寸，源图hash保留 |
| Python边界模型 | pydantic 2 | 校验JSON消息及候选；Rust仍独立验证 |
| 分发 | PyInstaller 6 onedir | 可定位的parser.exe及依赖目录，避免onefile每次自解压；无系统Python前提 |

Tesseract从官方源码用已固定vcpkg配置构建，不从不明镜像下载EXE；tessdata_fast的eng和chi_sim固定commit/hash随安装包提供，不运行时静默下载。PDFium采用pypdfium2官方wheel提供的对应binary，记录wheel hash和平台。字体/语言包/PDFium第三方许可随包保留。

主路径不引入PyMuPDF、Ghostscript或OCRmyPDF，也不引入AGPL解析依赖。pdfplumber的MIT、pypdfium2的Apache/BSD组合及PDFium第三方许可、Tesseract Apache2、PyInstaller bootloader exception都须在发行时核对完整传递依赖；选型信息不是已完成法律/供应链审计声明。

## 4 桌面与预览

| 职责 | 固定库/系列 | 具体决定 |
| --- | --- | --- |
| 桌面壳 | Tauri Rust 2、@tauri-apps/api 2、@tauri-apps/cli 2 | Rust壳连接同一worker，能力白名单，不另建核心 |
| UI | React 19、TypeScript 5.9 | strict，中文默认；不因TS7已发布自动迁移 |
| 构建 | Vite 8、@vitejs/plugin-react 5 | 单apps/desktop项目，离线静态资源 |
| 状态 | Zustand 5 | 仅UI及服务端事件快照；不能把UI缓存当真实任务状态 |
| 大列表 | @tanstack/react-virtual 3 | 账本/差异/来源虚拟列表，稳定ID |
| 对话框 | @tauri-apps/plugin-dialog 2 | 用户显式路径选择；不暴露任意shell插件 |
| 样式 | 普通CSS、CSS variables | DESIGN的字体/颜色/密度/可达性规则，不追加UI套件 |
| Markdown预览 | markdown-it 15、DOMPurify 3 | 先禁HTML和危险URL再消毒，Wikilink本地安全解析 |
| 图表 | Mermaid 11 | strict安全模式、离线资源；只接核心typed图表生成字符串 |
| 公式 | mathjax-full 3 | TeX输入/受控输出、禁网络与任意扩展；符号/单位语义由核心检查 |
| 单元测试 | Vitest 5 | UI状态、IPC假件、渲染与可访问性行为 |
| Schema类型生成 | json-schema-to-typescript 15 | 开发依赖，B14生成TS后运行tsc核对；官方15.0.4要求Node≥16、MIT，无明确TS5.9冲突但兼容须实测 |

Mermaid/MathJax运行在Tauri已有WebView2中。CLI只校验typed内容、Markdown AST、链接及资源，不做像素渲染，不宣称视觉正确。未完成真实视觉核验保持needs_review或保留原图退路；B18实际Obsidian逐项视觉门禁不可省略。

WebView2采用Tauri支持的Fixed Version离线打包方案，确切runtime版本/架构/hash在B16锁定并作为发布产物记录；不依赖用户在线下载。后续安全更新随应用版本更新。安装体积、启动耗时、杀毒误报及Python原生库加载均须实测。

## 5 固定自动UI测试栈

V1固定Windows外部驱动路线：`tauri-driver 2` + Microsoft Edge WebDriver + `webdriverio 9`、`@wdio/cli 9`、`@wdio/local-runner 9`、`@wdio/mocha-framework 9`、`@wdio/spec-reporter 9`。官方注册表已核到tauri-driver2.1.0、webdriverio9.32.0；首次落地锁定同系列兼容patch。

不采用embedded WebDriver产品插件、不把测试端口编译到生产包。测试脚本显式启动tauri-driver并把已安装应用绝对路径放`tauri:options.application`；匹配WebView2运行时版本的msedgedriver由开发测试准备脚本从Microsoft官方取得并记录hash。端口仅测试loopback，进程归测试Job Object。

固定入口`pnpm --dir apps/desktop test:e2e`，配置`apps/desktop/wdio.conf.ts`，首例`apps/desktop/test/e2e/smoke.spec.ts`；完整契约见[UI_AUTOMATION](UI_AUTOMATION.md)。Playwright不作为另一条必需主路径；若开发辅助引入，仅测试依赖，不随产品交付。UI测试自动采证，用户只参加确有必要的最终体验/科学判断确认。

## 6 锁定记录与验证

首次引入依赖提交锁文件与工具链精确版本，并在该任务验证报告记录：官方来源、解析到的版本/hash、许可证与原生组件、最低系统、实际构建和针对性fixture结果。第三方版本升级另开维护任务，先更新最小组件再跑受影响回归；不一键升级所有latest。

建议命令是实现任务的要求，不表示当前已运行：`cargo test --workspace --locked`；`uv run --locked --project sidecar/parser python -m unittest discover -s sidecar/parser/tests`；`pnpm --dir apps/desktop exec tsc --noEmit`；`pnpm --dir apps/desktop test --run`；`pnpm --dir apps/desktop build`；`pnpm --dir apps/desktop test:e2e`。具体任务可选择有意义的针对性用例，不能以零测试成功冒充验收。

## 7 官方核查来源

- Rust stable工具链：[官方channel manifest](https://static.rust-lang.org/dist/channel-rust-stable.toml)；crate版本来自[crates.io API](https://crates.io/data-access)，运行时文档参考[Tokio](https://docs.rs/tokio/latest/tokio/)、[rusqlite](https://docs.rs/rusqlite/latest/rusqlite/)、[reqwest](https://docs.rs/reqwest/latest/reqwest/)。
- Node来自[官方发布索引](https://nodejs.org/dist/index.json)；JS包版本/engines来自[npm Registry](https://registry.npmjs.org/)，不能用首页latest覆盖本表固定系列。
- Python库来自[PyPI JSON API](https://docs.pypi.org/api/json/)；[pdfplumber官方仓库](https://github.com/jsvine/pdfplumber)、[pypdfium2说明和许可](https://pypdfium2.readthedocs.io/en/stable/readme.html)、[Tesseract编译文档](https://tesseract-ocr.github.io/tessdoc/Compiling.html)、[官方tessdata_fast](https://github.com/tesseract-ocr/tessdata_fast)。
- 桌面：[Tauri sidecar](https://v2.tauri.app/develop/sidecar/)、[Windows安装器](https://v2.tauri.app/distribute/windows-installer/)、[官方WebDriver说明](https://v2.tauri.app/develop/tests/webdriver/)、[WebdriverIO示例](https://v2.tauri.app/develop/tests/webdriver/example/webdriverio/)。官方页面也列embedded路线；Noetome明确采用外部tauri-driver路线，避免生产应用测试插件。

官方文档证明接口与版本来源，不能证明本项目互操作、解析质量、费用、许可清单完整或真实Obsidian兼容。相应事实须保留待实测状态直至任务门禁给出证据。
