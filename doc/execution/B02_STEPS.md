# B02 执行步骤归档

本文件保留B02三项执行卡。三项均已独立验收，当前入口见[近期步骤](NEXT_STEPS.md)与[任务账本](../../dev-log/tasks.json)，此处的执行指令不代表当前仍待开始。证据为[B02-T01](../validation/B02-T01.md)、[B02-T02](../validation/B02-T02.md)及[B02-T03](../validation/B02-T03.md)。

目标是用单份明确选取的文本和自造fixture完成本地确定性闭环。依据为[路线](ROADMAP.md)、[最小数据契约](../technical/DATA_CONTRACTS.md)、[测试标准](../standards/TESTING.md)及[决策基线](../requirements/DECISIONS.md)。fixture、fake和自动抽取的能力证明必须分开。

## B02-T01 登记单份UTF8文本来源版本与完整Span清单

**单一结果**：CLI可登记一份MD/TXT并只读查看原始/规范文本指纹及完整来源位置清单，知识处理仍为pending。

**依赖**：B01-T02、B01A-T01。部分覆盖FR-01/02/06/35：单文件登记、文本定位、版本与选取范围、最小导入状态；不代表PDF解析、知识编译或完整长任务恢复完成。

### 执行步骤

1. 在`crates/noetome-cli/src`建立独立来源模块，先用进程测试固定`source import <项目路径> <文件路径> [--source-id <既有ID>]`及`source inspect <项目路径> <import-id>`的输出与错误；帮助和版本继续无写入。新来源生成不透明身份；未指定ID时同项目同字节复用已登记来源及版本，重命名不按显示名称换号。指定ID时先固定该来源身份，在其版本内复用或新增；不得因SHA相同合并到另一source_id。身份与内容去重分开。
2. 先定义Schema升级和恢复方案，再改`project.rs`及数据契约。Schema 1只允许基础表，不能直接塞新表后让旧inspect忽略；事务升级保持project_id/created_at，配置与数据库版本失配和中断须显式拒绝，inspect不迁移。只在新的测试项目验证升级和失败恢复。
3. 独立读取明确选定的单文件，项目内保存不可变原始/规范快照和SourceVersion/SourceSpan/最小导入记录。规范化只移除首BOM及统一换行，不替换无效UTF8、不删除未理解文字。快照目录由程序生成ID，不能接受外部输出路径覆盖。
4. 分段器只识别确定性的标题、普通文本、围栏代码和空白，按规范UTF8字节半开区间完整覆盖。测试独立复核全部字符边界、行号、区间原文与SHA；来源登记不创建完整Run、IR、NotePlan或Release。
5. 在`crates/noetome-cli/tests/source.rs`用新的临时目录验证同字节重复、显式ID更新及跨来源同字节身份保护、重命名、中文/空格、BOM/CRLF/CR、无效UTF8、未读全文件、外部哨兵和junction。完整读到但无法解码时登记failed SourceVersion，仅保留真实原始SHA/长度及原始快照，规范指纹/规范快照/Span不存在；未完整读取时只登记失败导入尝试，不建立假SourceVersion指纹。
6. 同步CLI帮助、README、数据契约、设计及`doc/validation/B02-T01.md`，运行本页统一检查、日志完成和分步提交推送。只展示来源清点与pending知识状态，不展示知识覆盖100%。

### 验收与停止

逐项满足任务账本验收，真实原始/规范快照及Span清单可重读复核，旧项目和哨兵受保护。完成该项后停止普通开发请求；下一入口为B02-T02，不自动启动。

## B02-T02 校验明确fixture的IR NotePlan与独立必要原子清单

**单一结果**：有效fixture契约校验通过，缺少必要原子、证据、覆盖记录或正确引用的fixture明确失败，不生成Vault。

**依赖**：B02-T01。部分覆盖FR-04/08/09/17/18：fixture覆盖对账、固定身份、原子条件/证据、规范位置与引用校验；不宣称自动消歧、自动抽取或通用科学正确率。

### 执行步骤

1. 在`fixtures/`建立自造且可审计的短来源、case清单、IR、NotePlan及独立expected，记录来源/审阅范围和限制。使用`producer=fixture`固定ID；生产导入生成的身份与fixture声明身份不得相互伪装。复用B02-T01分段实现，独立复核fixture的原始/规范指纹和定位。
2. 在CLI添加`case verify <case目录>`，在`src`建立fixture/契约校验模块；输入路径只能为case根内普通相对文件，严格拒绝未知字段/枚举、未来版本、越界和源指纹不符。验证流程保持只读，输出真实分项结论及非零失败退出码。
3. 校验完整Span与CoverageRecord一对一映射；included有知识映射，excluded有可复核理由，duplicate保留证据位置并拒绝自引用/环，pending/failed/needs_review不能算完成。
4. 核对Concept/Evidence/Atom全部引用与片段原文，必要原子保留条件/单位/例外及非空支持证据；独立expected给出必要原子及表达约束，不由待测程序重写。未验证必要原子阻断成功。
5. 核对NotePlan绑定完整IR版本/指纹，规范位置唯一、节点及表格映射完整、标题链接目标唯一。用固定有效/无效fixture分别覆盖缺原子、缺证据、覆盖缺漏/重复、过期IR、引用歧义、非法版本与路径。
6. 在`tests/fixture.rs`保存有区分力的契约/进程验证；同步契约Schema、README和`doc/validation/B02-T02.md`，运行统一检查后独立提交推送。

### 验收与停止

有效和各类失败case返回可复核且不同的结果，源/IR/计划/expected保持只读；日志不能把fixture标记当作模型质量证据。下一入口为B02-T03。

## B02-T03 将已校验fixture编译到全新临时Vault并生成双报告

**单一结果**：CLI将通过校验的fixture编译为两个关联笔记与比较表，输出到全新测试目录，并给出相同事实的JSON/Markdown报告。

**依赖**：B02-T02。部分覆盖FR-18/25/27/28/30/33/34：有限Markdown与标题链接、测试Vault及双报告、新目录拒写和未发布保护；不代表完整Obsidian兼容、增量保护或回滚。只交付明确标为fixture的本地闭环，不写用户生产Vault、不接模型/PDF/OCR/项目GUI。

### 执行步骤

1. 固定`case compile <case目录> <尚不存在的测试输出根>`进程行为；输出根内隔离本轮状态、暂存及Vault，不污染fixture根。既有目录、输入输出重叠、未知内容及Windows路径风险在创建输出前拒绝；没有force接管入口。
2. 从通过校验的fixture建立Run及逐Span CoverageRecord，保存政策/解析/IR/计划/渲染版本和各阶段真实状态；Run结束不直接代表Release已发布。
3. 在独立渲染模块按NotePlan顺序生成至少两份Markdown，保留必要原子、条件、单位和例外，覆盖标题链接、中文路径和比较表别名/转义。规范目标必须唯一，不将复杂块链接或嵌入默认为支持。
4. 在发布前用独立expected核对实际内容，并重读输出SHA/清单/全部本地引用。failed、not_run、incomplete均阻断发布成功，失败返回非零并保留明确原因和已生成的测试残留；不声称多文件发布在任意断电时原子。
5. 成功与失败运行的JSON/Markdown报告共用事实数据，分别表达source_coverage/knowledge_compilation/quality_validation/optional_research的状态、原因及计数。未启用研究为not_applicable，未运行模型没有假用量或性能数字。
6. 在`tests/compile.rs`验证正常新目录、同fixture两个新目录的内容字节及Concept/Atom/Note/Section身份一致，run_id/release_id/时间独立；另测缺内容/断链/路径风险失败、既有目标和外部哨兵保护。显式注入渲染故障及暂存后中断/登记失败，保留可诊断的staged/recovery_required，不能出现published成功；对应测试标准T13/T14。保存可复现产物与`doc/validation/B02-T03.md`。统一检查、独立验收和日志完成后分步提交推送。

### 验收与停止

完成来源与必要知识对账、实际输出与双报告一致性及人工内容保护；仅此时完成B02。之后按真实结果滚动规划B03文本PDF，不自动启动后续阶段。

## 每项统一检查

```powershell
cargo fmt --all --check
./scripts/cargo_local.ps1 check --workspace --locked --offline
./scripts/cargo_local.ps1 test --workspace --locked --offline
python -m unittest discover -s tests -p 'test_*.py'
python scripts/devlog.py check
git diff --check
```

执行者使用本轮选定解释器，不把本机路径写死到仓库。修改PRD时才运行build_prd，源Markdown为事实源。每项另外必须运行自己的CLI行为及失败保护检查，上述统一回归不能代替验收。任务定义和状态只通过账本及日志工具维护。
