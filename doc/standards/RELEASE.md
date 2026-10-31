# Noetome Windows发行与发布门禁

本文件规定个人可用V1的构建、安装、真实验证和封版条件。技术依赖以 [STACK](../technical/STACK.md) 和锁文件为准，产品边界以 [PRD](../requirements/PRD.md) 为准。本文是实施契约，不是已经执行的CI、安装或性能报告。

开发构建、内部安装候选、个人可用V1和公开签名发行是不同结果。未满足V1门禁时可以交付明确标注的开发构建；不得改变名称暗示已通过。普通Git推送不等同于公开发布、创建GitHub Release或授予许可证。

## 1 Windows目标与安装形式

V1目标为Windows 11 24H2及以上x64。正式候选在最低目标环境和验收时当前稳定Windows版本实测，记录完整build号；不宣称支持未测试的Windows 10或ARM。基准机使用8核处理器、16GB内存及SSD，记录实际型号；这是测试基线，不代表已证明最低硬件要求。

桌面通过Tauri NSIS生成每用户安装包。默认无需修改全局PATH，不安装系统服务，不启用登录自启动。程序目录、用户应用配置、项目工作区及Vault分离；卸载默认保留用户项目、来源和Vault，并列明程序缓存清理范围。

安装包必须在没有Rust、Python、Node、Tesseract和开发工具的干净机器上运行。所有运行必需资产随包提供，安装和第一次启动不得临时从互联网拉取未声明的运行依赖。

## 2 分发资产与依赖来源

| 资产 | 分发方式 | 核验要求 |
| --- | --- | --- |
| Tauri桌面与Rust CLI/worker | Windows x64 release二进制 | 工具链与Cargo.lock绑定，文件SHA256和版本可查 |
| Python解析侧车 | CPython3.12与PyInstaller onedir产物 | 自含解释器、锁定Python依赖和动态库，不依赖系统Python |
| PDF文字解析 | 固定pdfplumber及依赖 | 版本与源码来源记录于uv.lock和依赖清单 |
| PDF栅格化 | 固定pypdfium2及PDFium运行库 | 记录二进制来源、版本、许可证和校验和 |
| OCR | Tesseract5及固定eng、chi_sim语言数据 | 从官方源码经已锁定构建流程产生，不抓取不明exe |
| WebView2 | 固定版本运行时随离线安装包分发 | 完整版本、来源、许可和SHA256；不只依赖在线bootstrapper |
| 前端资源 | 编译后的静态资源与本地Mermaid/MathJax等 | 无CDN、无远程字体、无动态下载执行 |
| 第三方声明 | SBOM及THIRD-PARTY-NOTICES | 与实际打包文件逐项对应 |

Node、pnpm、Playwright及测试浏览器仅为开发和CI依赖，不随运行包增加独立Chromium渲染服务。桌面使用WebView2，CLI只做已实现的确定性与语义结构检查；真实视觉门禁另行执行。包体积如实测量，不预先承诺“极小安装包”。

release/dependency-manifest.json至少包含组件名、准确版本、来源URL、源码或二进制类型、构建方式、直接/传递依赖、许可证标识、许可证文件路径、打包路径、SHA256和必要再分发说明。不以Python/npm顶层包名替代PDFium、OCR模型等实际运行资产清单。

依赖固定至锁文件或明确提交及校验和。许可证未知、来源不可验证、与实际包不一致、已知不可接受漏洞无处置说明时阻断package门禁。架构已选择避免引入未经审查的AGPL解析路径；不能只因“本机能用”就把用户已有软件复制进安装包。

## 3 构建与CI职责

B16-T01实现.github/workflows/ci.yml、scripts/release_gate.py和对应行为测试。CI从新clone按锁文件构建，默认无私有凭据；PR回归不运行收费模型或上传私人语料。

CI至少包含文档/执行计划/开发账本一致性、SQL契约、Rust格式与静态/行为测试、Python侧车测试、前端类型与组件测试、Windows路径与进程集成、依赖许可证与漏洞检查。失败保存日志和最小复现产物，不能把空测试、被跳过测试或命令不存在视为成功。

实现后的标准构建命令如下；本文件不声称这些命令现在已经可运行或已通过。

~~~text
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
uv sync --project sidecar/parser --locked
uv run --project sidecar/parser --locked python -m unittest discover -s sidecar/parser/tests -p "test_*.py" -v
pnpm --dir apps/desktop install --frozen-lockfile
pnpm --dir apps/desktop run typecheck
pnpm --dir apps/desktop run test:ci
pnpm --dir apps/desktop run build
python -m unittest discover -s tests -p "test_*.py"
python scripts/devlog.py check
~~~

前端package scripts在实现时必须采用这些固定名称。SQL和执行计划检查的真实命令由仓库对应工具提供，不用一条echo替代。安装包由scripts/package.ps1在隔离构建目录产生；签名密钥只在明确的发行环境注入，不保存仓库或日志。

## 4 发布候选与证据公共结构

release_gate.py是证据校验器，不是测试结果猜测器。各测试和harness先执行实际工作、保存原始结果，门禁随后读取并验证。check命令不修改产品、不自动补“通过”、不以dev-log中的done状态代替证据。

每份release/evidence/*.json使用以下公共包络。

| 字段 | 类型与要求 |
| --- | --- |
| schema_version | integer，固定1 |
| evidence_type | string，与文件和门禁类型匹配 |
| candidate_id | string，绑定同一构建候选 |
| source_commit | 完整Git提交ID，必须存在；不得用任意短标签 |
| source_tree_sha256 | 所有声明构建输入路径及内容的清单哈希 |
| build_inputs | path、sha256数组；含实际产品源文件、锁文件、工具链和构建配置的完整闭包 |
| gate_tools | 检查器、harness、测试与数据清单的path、sha256及版本；与产品构建输入分别记录 |
| binary_or_package_sha256 | 对应实际二进制或安装包；不适用时null并说明 |
| config_sha256 | 实际运行配置快照哈希，不含明文凭据 |
| environment | OS完整版本、arch、CPU、RAM、disk、工具版本、VM/实体机、网络条件 |
| producer | fixture、local_stub、real_provider、installed_app、obsidian或build |
| started_at ended_at | 带时区时间戳，先后关系有效 |
| elapsed_seconds | 实际测得非负数；长任务另有单调时间证据 |
| status | passed、failed、not_run、blocked_external、not_applicable |
| checks | 非空数组，含check_id、预期、观察、状态、原始记录引用 |
| metrics | 指标值及单位、分子、分母；不写无来源成功百分比 |
| artifacts | 仓库内或受控证据根下路径、SHA256、媒体类型与用途 |
| issues | 问题ID、严重性、影响、状态、处置依据 |
| limitations | 明确不支持或未验证范围的字符串数组 |
| reviewer | kind=automatic或human、执行者与复核者标识、复核时间及依据；gold确认与最终体验须有真实human记录，其他自动检查可为automatic |

source_tree_sha256按路径排序，包含产品构建输入的完整集合；新增产品源文件不能因漏列而被忽略。该集合来自实际构建入口，包括Rust workspace、侧车、桌面、运行资源及打包配置，不把产物本身或证据文件作为自己的输入。单纯增加harness、测试、说明与保存证据的提交可以在候选提交之后，条件是产品构建输入集合及哈希未变；相关gate_tools和数据发生变化时重做该项验证。

涉及产品代码、依赖、运行配置或输出规则变化后必须重建受影响候选及证据，不接受“旧报告大概仍适用”。旧安装、容量或24h证据过期时，须先在更新候选上实际重跑对应安装过程或harness，再运行check；只重新读取旧JSON不能刷新证据。B18修复导致已完成B16/B17证据失效时，在当前门禁任务内刷新证据并保留历史；实质行为回归按WORKFLOW重新打开任务或建立修复任务。重新check不更改原始开始结束时间。

原始日志、测试报告、截图及时间序列必须实际存在并通过哈希检查。门禁能验证结构、完整性与相互一致性，不能独立证明人工填写事实；真实执行由运行器、安装环境与独立审阅证据共同支持。公开仓库仅保留可公开的脱敏证据，不泄露私有资料和凭据。

## 5 固定门禁命令

命令在仓库根目录执行。默认证据根为release/evidence，manifest为release/manifest.json；可通过显式--evidence-dir指定隔离测试证据目录，但不改变检查要求。完整命令为：

~~~text
python scripts/release_gate.py check engineering
python scripts/release_gate.py check package
python scripts/release_gate.py check install
python scripts/release_gate.py check benchmark-harness
python scripts/release_gate.py check capacity
python scripts/release_gate.py check soak
python scripts/release_gate.py check real-quality
python scripts/release_gate.py check obsidian
python scripts/release_gate.py check release
~~~

| 门禁与任务 | 必需证据 | 必须核验的内容 |
| --- | --- | --- |
| engineering / B16-T01 | engineering.json及测试运行器原始报告 | 新clone锁定构建、全部工程检查、每个必需测试套件实际运行且至少1例、失败0、无跳过必需检查 |
| package / B16-T02 | package.json、release/dependency-manifest.json、实际安装产物 | 构建来源、二进制与资源哈希、SBOM/许可、离线资产、包尺寸、签名状态 |
| install / B16-T03 | install.json、desktop-smoke.json | 两目标Windows环境的真实安装、离线首次启动、CLI/worker/解析侧车、桌面闭环、升级、卸载保护 |
| benchmark-harness / B17-T01 | benchmark-harness.json、tests/benchmarks/manifest.json、gold及harness单测结果 | 200人工审阅原子、40关键、20身份/冲突、20图公式、冻结指纹、计量公式与坏gold拒绝 |
| capacity / B17-T02 | capacity.json、interaction.json及原始采样 | 两个规定规模数据集实际处理，P95≤2秒、暂停2秒内停发、无无界资源增长 |
| soak / B17-T03 | soak.json、完整时间序列和故障事件 | 实际清醒负载≥86400秒、真实墙钟、恢复、一致性、在途预算、无假重复发布 |
| real-quality / B18-T01 | provider.json、quality.json、research.json | 真实provider能力与调用、两种模式、独立gold、质量阈值、批准范围及完整证据链 |
| obsidian / B18-T02 | obsidian.json及逐例证据 | 最低支持与当前稳定两个真实Obsidian版本、无社区插件、关键语法与100%正确链接 |
| release / B18-T03 | acceptance.json、release/manifest.json及上述全部证据 | 40FR/40AC/17NFR追溯、同候选版本、所有硬门禁passed、用户手册、迁移和限制说明 |

check返回0仅当该门禁全部必需证据通过；2表示参数或证据Schema错误；3表示证据缺失、not_run、blocked_external、未达阈值或验证失败；1表示门禁内部错误。失败摘要必须列具体check_id、文件、原因和缺失条件，JSON输出同样提供明确结果。任何异常都不能落入默认成功分支。

门禁检查自身可在20分钟以内完成，但它验证的24h原始实验需事先实际运行。不能让soak检查在20分钟内模拟一天。--skip、空测试、伪造时间差或手工把status改成passed都不能满足检查。

## 6 各证据的专用字段

公共包络之外，下列字段必须出现；无实际数据时对应门禁未通过，不填零表示成功。

### 工程 包与安装

engineering.json含commands及argv、退出码、测试套件名、collected/passed/failed/skipped数量、锁文件哈希、CI运行定位和依赖审计结果。required_suites由发布版本固定，不能由被测命令自行返回空列表。

package.json含package_path、package_bytes、package_sha256、target_triple、tauri_version、sidecar_manifest、webview2_version、offline_assets、sbom_ref、license_review、vulnerability_review、signature.mode及验证结果。mode为internal_unsigned或signed_distribution，二者不混淆。

install.json含至少两个environment_results，分别标识minimum_target和current_target，记录安装前无开发工具、断网验证、安装/启动/升级/卸载各步骤、退出码、残留文件和保留数据指纹。若两个目标恰是同一完整版本，仍须分别完成干净安装与升级场景，不用一个空对象代替。

安装、升级、卸载和桌面旅程优先由Agent在可用隔离环境自动执行；缺少系统镜像、VM或权限时才报告精确环境缺口。不得把提供一串安装测试命令当作已经交付的自动验收。

desktop-smoke.json含安装包哈希、真实界面旅程、core事件、输入与输出清单、审批与重启结果、截图。只跑源码dev server不合格。

### 金标准与质量

benchmark-harness.json含corpus_manifest_sha256、gold_sha256、gold_version、reviewed_atom_count、critical_atom_count、identity_conflict_case_count、visual_formula_case_count、reviewer_evidence、计量单测及不合法gold拒绝结果。

provider.json含真实provider/model、capability_profile、请求和响应身份、适配器版本、实际费用或未知项、发送范围与预算配置。不得保存密钥、完整认证头或私有原文。

quality.json含逐个gold判定、recall分子分母、critical_recall、至少200条事实样本及准确率、来源叶单元对账、证据映射与定位抽查、链接验证、条件与视觉核验、排除抽查、未处置严重错误数。门槛为召回≥98%、关键100%、准确≥98%、来源清单对账100%、实质原子映射100%、抽查误定位0、本轮发布链接100%、关键图公式逐项通过、严重未处置错误0。小样本或fixture producer不合格。

research.json含source与research两种真实流程的run_id、批准主题、query/page统计、引用证据、超范围候选、停止原因、写作反馈问题与重编译依赖。缺少检索凭据或未执行真实研究时不得以本地stub报告通过。

### 容量与交互

capacity.json分别记录集合A和B。集合A至少3份实际文件、1000实际内容页、10000不重复必要原子、1000实际输出笔记；集合B为3000实际笔记、200篇预先修改文件和差异清单。输入、数据库实际计数、输出文件计数、指纹和运行日志必须能对账。

容量数据可以自造，模型可使用fake，但producer与用途必须明确；不证明真实资料质量。禁止只改报告数字、用未解析空页补数量或把一条原子复制10000次凑数。

interaction.json对打开项目、搜索、打开问题、启动扫描与分页至少各100个有效样本，记录测量起止、采样原始值、P50/P95/P99及暂停停发延迟。热/冷缓存分别说明；超时不能从样本剔除。常规操作到可用反馈P95≤2秒，长扫描/编译分别记录首次状态与总耗时，暂停2秒内停止新派发。

### 24小时

soak.json包含UTC开始结束、monotonic_elapsed_seconds、awake_workload_seconds、sleep_intervals、固定seed、工作负载清单、时间序列引用、资源趋势、Job与发布数量、故障事件、重启检查点和预算一致性结果。

elapsed与awake_workload都必须≥86400秒；睡眠单独计，不用关机或长时间空等替代负载。每60秒至少一个采样，缺口必须对应记录的睡眠、受控故障或重启并有解释，不能接受整天只有首尾两条日志。使用本地API stub避免默认烧真实费用。

必须包含睡眠唤醒、429、超时、worker终止和发布killpoint。所有故障有可解释终态，无不可恢复DB/对象损坏、未批准覆盖、重复发布、未知费用预留错误释放。稳态资源增长判定按TESTING执行，不接受只报告“内存正常”。

### Obsidian与完整验收

obsidian.json含两个准确应用版本、干净配置说明、同一Vault哈希、逐例语法和目标定位、关键图表公式对照、截图、失败和处置。HTML解析或Tauri预览不能替代。

acceptance.json含40个FR、40个AC和17个NFR的逐项追溯，关联实际test_ids、证据文件、实现范围、结果及限制。每个条目至少1项可核验适用证据；工程初始化不能代替完整FR。另含user_experience_acceptance，记录用户查看的实际候选、代表性导入/查看/修订体验、结论与时间；没有实际用户反馈不得由Agent填accepted。release/manifest.json绑定candidate、版本、build_inputs、安装包与所有证据哈希、配置/存储迁移范围、已知限制、用户手册及署名/许可状态。

## 7 离线安装与生命周期检查

在干净且断网的Windows目标环境安装候选，验证程序、worker、解析侧车、Tesseract语言数据、PDFium与WebView2都来自打包资产；无系统Python/Rust/Node仍可执行本地导入和fixture编译。真实模型功能可以因无网络显示不可用，但不能阻止本地项目查看和已有笔记阅读。

检查中文与空格安装目录、长项目路径、标准用户权限、文件占用、现有版本升级、升级中断和卸载。升级前备份配置与数据库，不自动迁移后再用旧版继续写入；失败留明确恢复入口。卸载前后核对项目、来源和Vault哨兵，未授权删除零次。

验证窗口关闭和托盘语义、暂停退出、在途请求及重启恢复，不创建隐藏常驻服务。资源缺失或哈希不符应阻断相关能力并提供可诊断错误，不能静默改用网络下载或用户目录中的不明二进制。

## 8 签名 公开发布与阻塞

无签名证书时可以产出internal_unsigned内部候选，报告准确的签名状态；不能伪造签名、禁用系统安全保护或声称可公开签名发行。signed_distribution必须有真实签名、时间戳与验证记录。签名证书缺失只阻断签名发行，不抹去其他已通过工程证据。

缺少授权语料、人工gold、真实API或检索凭据、明确预算、目标系统环境时，相关门禁blocked_external。离线开发继续推进；最终release必须等待全部V1硬门禁真实通过。代理不能自行编造审阅、放宽预算或把真实未运行项变成不适用。

用户参与限制在必要的权限/预算配置、gold知识确认和最终体验验收；已有配置和证据继续复用。其余构建、测试、安装、截图、对账、失败修复与报告整理由Agent推进。向用户请求参与时同时提供已经准备好的具体候选与简短验收点，不能让用户先完成研发或代替Agent采集可自动获得的技术证据。

封版只表示本地候选与验收证据齐全。是否公开发布、创建远程Release、变更许可证或共享私人语料仍按用户明确范围执行；不把CI通过当作外发授权。
