# 近期开发步骤

真实状态以[任务账本](../../dev-log/tasks.json)和[当前状态](../../dev-log/STATUS.md)为准。B04/B04R执行范围归档于[B04R步骤](B04R_STEPS.md)，来源核对和单行窗口实机结果分别见[B04-T03](../validation/B04-T03.md)及[窗口记录](../validation/B04-T03-ui-single-row.md)。

B04-T03已完成日志、审查修复、实际安装和普通推送；收束提交`b3d40a02b0eb3a8905c190b2b78442a4c9121c29`与远端相等。最后完整Rust532、Python108、前端10及原生16项和用户托盘/拖动确认有真实证据。所选模型配置/DPAPI继续保存并复用：累计30097、余额9969903、pending0，T03新增模型请求0。

当前批次 **B05** 只含下面三张顺序执行卡。B05-T01已完成作者化输入、Rust协调、独立Python、整分支审查修复与最终限定复查，并用devlog工具记录完成；实际Rust561、独立模块35、完整Python143和33场景/56次普通CLI有原始证据。最新生产修正为 `2618752028557d15e8fb5a1828f243cf3f25f935`；代码、日志与证据收束提交 `19cde25006e7e4d17f9c4fad69fdb28d54dcb3e8` 已普通推送至 `codex/b05-t01-knowledge-coordination`，git ls-remote实际返回相同HEAD，见[原始推送核对](../validation/assets/B05-T01-push-checkpoint.json)。B05-T02已通过devlog工具验收：最终坐标修正 `3b6a59c`、DTO排版 `01742df`，新鲜Rust591、Python157、v2审计42场景/126命令及v1回归33/56通过，一次整分支审查后的同一修复限定复审确认R1/R2全部ADDRESSED；原作者/来源/失败和14615件最终证据已核对保存。当前仅剩本项Git收束和远端核对，完成后顺序推进B05-T03；其保持todo，切批次或工具complete不会自动start。完成状态由账本生成，详见[B05-T01验证](../validation/B05-T01.md)和[B05-T02验证](../validation/B05-T02.md)。

**目标**：在已验收的受控单来源核对上，先合并重复并保留互补，再区分条件和历史，最后呈现真正冲突与证据不足。

**架构**：Rust复用实际来源读取与输入/输出守卫，消费2至3份各含一个原子的成功source_review输出及核对依据。独立Python从作者gold、实际SQL/资源及原始输入验证协调结果；使用新的协调交换契约和输出根，保持B04原IR与项目Schema不变。

**技术**：现有Rust workspace/SQLite/严格JSON、Python标准库及现有CLI测试，不引入模型、向量库或完整图谱框架。

**依据**：[PRD FR-08/09/10/26/40](../requirements/PRD.md)、[已采纳决策](../requirements/DECISIONS.md)、[数据契约](../technical/DATA_CONTRACTS.md)、[来源核对契约](../technical/EXTRACTION_REVIEW_CONTRACT.md)及[测试标准](../standards/TESTING.md)。这是这些需求的有界工程实现，完整概念身份撤销、真实三教材融合、科学质量与长期维护仍需后续任务验收。

> 执行者：每项start后使用superpowers:subagent-driven-development或superpowers:executing-plans，先写当前单一账本任务的具体实施计划和接口，再落实真实RED、实现、独立审查和收束。子代理只分担当前任务，不各自启动后两张卡；每项complete、检查、提交推送后才顺序start下一项。

## 全局约束

- 同时最多一个in_progress；本批最多三张详细卡。既有失败和完整历史保留，状态通过devlog工具维护。
- B05先限定一个明确测试项目中的2至3份有界source_review依据，每份一个原子；新case清单上限64KiB，单份IR/候选/所选来源继续有界，不扩展任意长资料或全学科语言。
- 明确身份映射是本批前提；不凭名称、缩写、相似度或模型置信自动认定相同概念。显示名称与输入顺序不能换ID，原ID和每个独立来源位置必须保留。
- 只读明确选取的测试来源、原gold、IR和核对产物，输出只到新测试根。生产Vault、未知文件、人工内容和外部哨兵保持，普通CLI不提供强制覆盖或测试故障开关。
- 不读取凭据/模型配置，不发新模型或研究请求；保存服务和累计预算以后继续复用，不重复索要。fake/作者工程样例如实标记。
- 分别统计输入出现次数、规范事实、条件变体、来源位置和未解决项；没有可确认数组/字段时保持未知，不把成功解析、重复引用或零例外当成正确率。
- scientific_quality=not_run、research_status=not_requested、published=false；来源一致及协调通过不能宣称科学正确、98%正式准确率、200原子基准或完整覆盖。
- 未实现的单位换算、未知适用条件，以及需要时间分类却没有时间证据的差异均保持需复核。已证明的重复/互补或完整条件差异可保留未知时间，但不能据此分类历史先后。新旧来源、票数或排序不能自动裁定权威；没有解析/来源证据的条目先失败。
- 共享交换字段变化先明确版本及旧读取器影响，不向B04严格IR混入未知字段；不预建未来阶段空表。

## B05-T01 多来源重复归并与互补事实保留

**单一结果**：三份实际来源核对输出中的两个相同事实合为一个，第三份互补事实独立保留；规范身份和三处来源证据可独立对账。

**依赖**：B04-T03。关联FR-08/09/10/26/40、NFR-03/07的本项范围。

**文件责任**：新增`doc/technical/KNOWLEDGE_COORDINATION_CONTRACT.md`和本项实施计划；Rust协调/严格模型分别放`crates/noetome-cli/src/knowledge_coordinate.rs`及`knowledge_coordination_model.rs`，仅在CLI分发及必要输出owner增加接线。普通入口测试放`crates/noetome-cli/tests/knowledge_coordinate.rs`；独立作者三来源、身份映射和gold放`fixtures/knowledge-coordination-v1`。独立校验放`scripts/verify_knowledge_coordination.py`和`tests/test_knowledge_coordination_verifier.py`，真实验证放`doc/validation/B05-T01.md`及其资产。

**接口目标**：普通`knowledge coordinate <project> <coordination-case> <new-output>`读取严格版本清单、明确身份映射及2至3份已保存source_review依据；重新打开实际SourceSession并核对来源/四输入/IR/双报告关系。成功生成未发布`coordination-ir.json`与`coordination.json`/`coordination.md`；不能只读取报告passed标志。第一步先在新契约中冻结这些字段、状态、限额和失败输出，再写生产接线。

**顺序与验收**：

- [x] 作者先固定三份公开工程短文、明确canonical ID和gold，再生成synthetic候选并用实际`extract verify`取得三份依据。两个同对象/属性/值/单位/完整条件/例外的事实应归并，第三个不同属性的互补事实应保留：规范事实2、独立来源出现3。
- [x] 写普通CLI测试并在接线前取得真实行为RED；正确样例预期exit0和完整证据集合，错误样例必须有具体失败或需复核。编译/设置失败单独记录。
- [x] 实现有界守卫与显式身份协调；规范ID不依赖显示名称/输入顺序，原ID可追。跨来源同SHA保留各自身份，同一输入重复引用不能冒充独立来源。未处理的条件/单位/时间差异保持需复核。
- [x] 独立Python从固定gold和实际源SQL/资源核对两事实/三来源及JSON/Markdown/IR；篡改错来源/引文、丢第二证据、错误归并、数量填零、重复输入和既有/内嵌/链接输出必须被检测。输入和源前后不变。
- [x] 运行针对性普通入口、独立脚本及必要完整回归，新鲜审查修复后用devlog complete附代码/真实产物，分步提交推送并核对远端。此时不把明确身份原型算成通用实体识别/归并撤销或真实教材质量验收。

预定行为验证：`./scripts/cargo_local.ps1 test -p noetome-cli --locked --offline --test knowledge_coordinate '--' --test-threads=4`；`python scripts/verify_knowledge_coordination.py audit <absolute-noetome> <nonexistent-root>`；保存后只读复核为 `check <project> <case> <output> <gold>`。独立校验器用真实一致伪造报告先RED再GREEN。

## B05-T02 条件差异与历史观点的并列变体

**单一结果**：同一对象/属性在不同适用条件或有来源依据的时间背景下保持并列结论，不被错误去重或二选一。

**依赖**：B05-T01。关联FR-09/10/26/40、NFR-03的本项范围。

**文件责任**：扩展T01协调契约、逻辑、模型、普通CLI测试和独立校验，追加调用前作者化条件/时间样例；不改B04原IR或旧gold。验证记录放`doc/validation/B05-T02.md`及新资产。

严格版本、背景语法、单位/日期边界与内部步骤见[v2契约](../technical/KNOWLEDGE_VARIANTS_CONTRACT.md)和[本项计划](B05-T02_VARIANTS_PLAN.md)。普通新行为测试单独放`crates/noetome-cli/tests/knowledge_variants.rs`，原`knowledge_coordinate.rs`继续承担v1回归；独立v2实现与audit独立成模块，由同一校验入口分派。

**接口目标**：保留T01普通CLI与依据链；协调报告逐条表示条件变体/历史演变、原值/单位、时间证据与需复核项。若需新字段，先版本化严格协调交换，不让旧读取器静默接受未知字段。

**顺序与验收**：

- [x] 先固定同对象同属性的T20C/T30C、样例适用范围与例外差异、明确时间和未给时间的独立expected，再写真实CLI和一致伪造报告RED。
- [x] 条件差异保留原值与完整条件/例外；unknown/not_applicable分开，不能把未知条件当相等空集合，也不凭接近数值归并不同单位。
- [x] 历史演变仅依据实际可追溯时间；导入mtime、文件名或响应时间不能充当观点新旧证据。没有时间依据或换算依据的关系保持需复核。
- [x] 普通CLI/独立校验分别检测删除条件、丢历史证据、伪造时间、单位误归并；输入顺序与显示名称变化后ID/来源不丢。双报告和成功时IR同事实、真实数量与状态可对账。
- [ ] 相关回归、守卫、新鲜审查、日志及Git检查通过后complete、提交推送。任意条件理解、自动单位换算和科学裁决继续按后续实测处理。

预定行为验证沿用`tests/knowledge_coordinate.rs`普通入口与独立`scripts/verify_knowledge_coordination.py`，新gold在任何候选/CLI前写入；不再运行模型抽取。

## B05-T03 真实冲突与证据不足的可追踪报告

**单一结果**：真正不相容的有据结论和证据不足分别定位，保留各方原值、条件、历史与反证，无法裁定时准确显示需复核。

**依赖**：B05-T02。关联FR-10/26/40、NFR-03/07的本项范围。

**文件责任**：扩展同一协调契约/实现/普通入口/独立校验；作者工程冲突及证据不足样例只追加新case。真实记录放`doc/validation/B05-T03.md`及新资产，不引入联网研究、科学权威评分或生产发布。

**接口目标**：同一CLI报告重复/互补/条件差异/历史演变/真实冲突/证据不足及逐对象处理记录，未解决必要冲突exit3、无成功协调IR/发布；无冲突项和争议项都在双报告中保留。

**顺序与验收**：

- [ ] 作者先固定同对象/属性/单位/完整条件的10与12、第三份重复支持和缺证据样例，取得真实CLI与错误裁定报告RED。解析/引用错误先作为输入失败，不能包装成两种有效观点。
- [ ] 分类按实际对象、条件、时间和原值判定；冲突保留各方来源版本/位置。默认保留并需复核，来源排序、新旧或票数不构成自动胜出理由。
- [ ] 删除反证/历史依据、把未知填零、伪造已核实或无必要冲突却判失败均应被独立gold/来源核对检测；已通过无冲突项不丢失。
- [ ] JSON/Markdown相同表示真实分母和四状态、问题位置、依据及未解决原因；冲突或证据不足不产出成功协调IR。来源/原IR/作者gold和新根外哨兵保持。
- [ ] 普通CLI、独立篡改矩阵、必要完整回归和新鲜审查实际通过后complete、提交推送。B05三项全验收后再依据结果细化下一批最多三项。

预定行为验证继续同一普通入口/独立校验，加入正确冲突、条件差异非冲突、历史非冲突和不足证据等明确expected；工程冲突样例不证明自然科学裁决准确。

## 统一检查与交接

```text
./scripts/cargo_local.ps1 fmt --all --check
./scripts/cargo_local.ps1 check --workspace --locked --offline
./scripts/cargo_local.ps1 test --workspace --locked --offline '--' --test-threads=4
python -m unittest discover -s tests -p 'test_*.py'
python scripts/devlog.py check
git diff --check
```

Python使用本轮工具实际选定解释器；路径不硬编码进仓库程序。完整回归在相应产品改动后执行，不用重复B04实机安装证明新协调功能。每卡独立完成并推送后再start下一卡；当前进行中及下个入口以任务账本和生成STATUS为准。
