# B02-T01 文本来源登记实施方案

目标与验收来自[近期任务卡](NEXT_STEPS.md)、[数据契约](../technical/DATA_CONTRACTS.md)及[测试标准](../standards/TESTING.md)。本方案只落实来源登记，不建立Run、IR、NotePlan或Release。按当前用户授权执行例行实现选择、独立验收与提交推送。

## 接口和文件分工

- `src/text.rs`：纯UTF8规范化、SHA256和完整分段；不生成身份或写文件。规范化仅去首BOM、统一换行；空文本为0段；未闭合围栏保留Code区间并标为needs_review。
- `src/source_schema.rs`：Schema2精确表形状、约束及候选库升级。只含project_metadata、sources、source_versions、source_spans、source_imports。
- `src/source_write.rs`：受保护的候选库、备份、快照发布与失败清理；不接受用户给出的内部输出路径。
- `src/source_model.rs`：持久实体和最小导入报告、身份/引用/修订/计数校验、不可变快照与完整Span重读；不访问原文件或执行迁移。
- `src/source.rs`：读取明确MD/TXT、分配并持久保存UUID、版本复用及最小导入记录、真实快照与位置重读校验。
- `src/project.rs`：保留Schema1初始化，支持严格只读Schema1/2检查，提供受限的writer内部验收及既有路径保护。
- `src/lib.rs`、`src/main.rs`：纯参数解析、受限命令分发与JSON结果；桌面预览仍只接受帮助/版本。
- `tests/source.rs`及模块测试：进程行为、源字节、独立位置/SHA、升级失败和真实中断验证。

CLI为`source import <project> <file> [--source-id <existing-id>]`和`source inspect <project> <import-id>`。原生命令参数不拼接成shell；路径保留中文和空格。结果stdout为报告Schema1 JSON，包含项目/导入/来源/版本身份、实际状态、reused、pending知识状态、span_count、pending_extraction_count、版本与快照定位、完整Span和错误。两计数均为实际持久Span数量，不冒充已完成的知识覆盖。有效failed记录的只读inspect退出0；导入失败已登记时返回3，并输出报告及稳定诊断。非法路径、类型和身份在写入前拒绝。

## 来源和版本

原文件只读。单文件当前限制64 MiB；超过限制或未能完整读取只记录失败尝试，不截断为完整文本。完整读到但无效UTF8保存failed版本的真实原始SHA、长度和raw.bin；没有规范指纹、规范快照或Span。

快照根固定为项目内`.noetome-sources`，owner文件绑定项目ID，版本子目录仅用程序创建的UUID，保存raw.bin与合法UTF8的normalized.txt。检查拒绝junction、未知所有权、未知缓存内容及文件替换，不接管人工目录。

指定source-id先固定既有来源身份，再在该来源内按原始SHA复用或增加revision。未指定时优先保留此前明确选中同一路径的来源关联，否则按已有内容复用或生成新来源身份；名称不参与身份生成。相同字节不新增版本/Span，但每次选择都有新的import-id。旧版本和原位置保持不可变。

## Schema和写入保护

project init继续生成Schema1。首次导入在候选SQLite中事务重建metadata为Schema2，保持project_id、created_at及initialized基础状态；配置和数据库版本同时升级。initialized只表示项目建立完成，不再对Schema2固定声称空项目。

每次写入独占创建`.noetome-writing`事务目录，保存操作说明、旧配置/数据库备份及候选。Windows准备阶段保持拒绝写入/删除的读句柄，切换前释放它们，保留共享的身份句柄作复核。候选事务开启外键，发布前执行Schema、外键、记录、快照和完整区间验证。新的快照目录不覆盖目标。

替换配置和数据库前检查旧字节及身份；成功重读两文件和快照后才清理候选、备份及写目录。公开inspect不迁移、不恢复，遇写目录或旧初始化残留明确拒绝并保留。writer仅可忽略自己身份匹配的写目录。普通写入失败恢复原配置/数据库并清理仍属自己的文件；未知内容、替换、真实进程中断或清理失败保留标记/备份，返回未完成。来源读取失败记录的成功持久化不应被误当写失败回滚。

该协议约束合作CLI并识别可观察的文件变更，不承诺任意外部同时编辑或任意断电下多文件原子提交。失败证据分别列出已验证与未验证范围。

## 实施和验收顺序

1. 先观察文本和进程最小用例的真实断言红，再实现纯解析与参数接口；缺模块或编译错误不算行为红。
2. 以Schema2检查、失配、未知表/约束和写标记用例驱动只读检查与候选升级。
3. 以正常导入、复用、换版、无效UTF8、未读取完整、真实Windows占用、快照碰撞及哨兵驱动存储。
4. 注入候选/发布/清理失败；真实终止子进程验证写目录、备份、未完成拒绝和新项目重试。检查原配置/数据库及人工内容指纹，不只看命令返回。
5. 同步数据契约、README、CLI设计和独立`doc/validation/B02-T01.md`，运行Rust fmt/locked-offline check/test、Python日志测试、devlog check、本地链接和diff检查。分步提交、推送并核对远端。完成事件仅记录实际执行证据。

上述分工属于同一B02-T01，不为子代理另启动产品任务；B02-T02仍依赖本项独立验收完成。
