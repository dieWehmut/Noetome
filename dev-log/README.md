# 开发日志使用规则

`tasks.json` 是开发状态的唯一事实来源。`TODO.md`、`DONE.md` 和 `STATUS.md` 由 `scripts/devlog.py` 生成，不手工编辑。每一小轮先明确范围、依赖及验收条件，再启动任务；日志工具本身不启动产品开发，也不会根据 Git 提交猜测测试是否通过。

日志启用前的两次已完成提交记录在 [BASELINE.md](BASELINE.md)。后续不在基线文件中追加日常状态。

## 账本结构

首版使用 `schema_version: 1`。顶层包含 `current_batch` 和非空 `tasks` 列表；可选顶层 `history` 记录批次切换。每个任务包含以下字段。

| 字段 | 含义 |
| --- | --- |
| id | 全局唯一且稳定的任务编号 |
| title | 任务名称 |
| batch | 所属小轮编号 |
| status | todo、in_progress、blocked 或 done |
| depends_on | 依赖任务编号列表，可为空 |
| requirements | 关联需求编号列表，工程治理任务可为空 |
| acceptance | 至少一项可验证的完成条件 |
| history | 按时间追加的状态变更事件列表 |

新规划任务的初态为 `todo`，历史可以为空。预先标成 `blocked` 的任务也必须有一条说明原因的 `block` 历史，不能只改状态字段。已开始及已完成任务同样必须有完整历史链。

任务事件字段为 `at`、`action`、`from_status`、`to_status`、`note`、`validation` 和 `artifacts`。工具在账本中写入带时区的 UTC 时间，面向用户的生成视图显示 Asia/Shanghai（+08:00）。完成事件必须有非空验证说明和至少一个产物记录；产物记录为仓库相对路径 `path` 与当时内容的 `sha256`。批次事件另用 `from_batch` 和 `to_batch`。

## 每轮操作

以下命令在仓库根目录执行；将示例 `python` 替换为当前环境选定的 Python 解释器。

```text
python scripts/devlog.py start B00-T01 --note "开始本轮已规划工作"
python scripts/devlog.py complete B00-T01 --note "已完成约定范围" --validation "具体检查方法与实际结果" --artifact doc/requirements/PRD.md
python scripts/devlog.py block B01-T01 --note "写明阻塞原因与解除条件"
python scripts/devlog.py reopen B01-T01 --note "阻塞已解除，说明依据"
python scripts/devlog.py batch B01 --note "上一轮已完成，开始下一轮已评审计划"
python scripts/devlog.py render
python scripts/devlog.py check
```

`--validation` 和 `--artifact` 可以重复使用。完成前执行者必须实际检查验收条件，将执行命令、检查对象和结果写入验证说明；工具只保存这些陈述，不代替测试执行。产物必须是当前仓库内已经存在的文件，不能是目录、缺失文件或指向仓库外的路径，也不能是 `dev-log/tasks.json` 或生成的 `TODO.md`、`DONE.md`、`STATUS.md`，以免形成自引用。工具记录内容哈希，不读取 Git 状态来推断完成。

`start` 只接受当前批次的待开始任务，要求所有依赖已经完成，并限制全库同时一个 `in_progress`。`complete` 只接受进行中任务。`block` 接受待开始或进行中任务，并释放活动任务名额；阻塞期间可以开展当前批次中不依赖它的工作。`reopen` 将阻塞或完成任务转回待开始，保留此前所有历史。若仍有已完成或进行中的任务依赖它，应先从下游处理依赖，不能使已有工作失去有效前提。

切换批次前，当前批次的全部任务必须完成。若需要调整计划，应先评审范围和验收影响，更新计划与账本中的任务定义，再重新生成视图；不通过直接修改状态绕开历史约束。

`DONE.md` 保留每一次完成事件。任务重新打开后，过去的完成事件仍保留，并标明任务当前状态。“产物与完成时指纹”记录当时文件的哈希，不保存文件历史副本；历史文件内容由 Git 版本记录保留，产物链接指向当前文件。后续合法修改文件不会自动改变过去记录，`check` 不要求历史哈希等于文件现在的内容。需要复查历史产物时，应结合版本控制中的对应文件和日志事件进行核对。

## 检查与故障恢复

`check` 完全只读：校验结构、唯一编号、依赖、循环、活动任务数、状态历史和三个生成视图是否对应当前账本。可在提交前检查中调用，检查不自动修复或改写文件。发现视图过期时，显式运行 `render`，再检查并提交相应视图。`check` 验证记录格式及完整性，不证明验证说明真实，也不替代产品验收。

所有写命令使用操作系统独占文件锁，避免两个进程覆盖彼此的更新。进程退出或崩溃后操作系统释放锁；`.devlog.lock` 文件可以保留，不需要日常删除。账本和各视图先写同目录临时文件、刷新文件内容，再原子替换目标，写入失败不会把旧文件截断。锁文件及临时文件应由 Git 忽略。

账本先落盘，视图随后刷新。如果报告“账本已安全更新，但视图刷新失败”，状态事件已经保存；应修复磁盘或权限问题后运行 `render`，不要重复提交同一个状态变更。生成视图不是跨文件事务，因此中断后可能暂时过期，但 `check` 会检出差异。未经恢复和检查，不把日志状态作为提交通过依据。

新增任务、修改任务标题或验收条件属于计划变更，应在本轮规划文档和 Git 差异中审查。状态和完成证据通过工具维护。任何自动化仅可记录实际执行结果，不得把提交成功、文件存在或模型自行判断直接等同于验收通过。

## 完整计划与实际验证（plan_version 1）

64个产品任务附带specs、steps、outputs、checks、non_goals、failure_policy、scenarios和quality。任务定义与状态共用tasks.json，plan工具生成18批任务卡和FR/AC/NFR追溯表；生成视图禁止手改。

```text
python scripts/plan.py next
python scripts/plan.py show B01-T01
python scripts/plan.py render
python scripts/plan.py check
python scripts/verify.py B01-T01
python scripts/devlog.py complete B01-T01 --note "实际成果" --validation "行为验收结果" --artifact crates/noetome-cli/src/main.rs --report .artifacts/verification/B01-T01.json
```

plan next只读返回resume/start/batch/blocked/complete，不自动改状态。verify实际执行卡片checks，缺命令、缺产物、退出非零、针对性测试数不足或验证期间源码变化都会失败。完成计划任务必须提供对应报告，并复查当前源码与任务定义；不能拿旧报告完成新代码。历史治理任务兼容旧完成方式。

源码指纹覆盖Git可见文件及删除，排除账本、派生日志、.artifacts和dev-log/evidence，避免状态写入自引用。正式release/evidence必须跟踪，不在排除范围内。报告只保留命令、结果、数量和哈希，不持久保存可能含教材正文的控制台输出。完整验证摘要随完成事件保存在账本，原报告可留本地.artifacts。

此机制防止误用证据，不防执行者蓄意篡改报告。科学判断、真实环境和性能证据仍按TESTING/RELEASE审阅。禁止以工具完成记录代替这些验收。

一键入口：`python scripts/autotest.py`检查当前治理工具，`python scripts/autotest.py --task <ID>`增加指定任务的实际检查。用户说“继续这个项目”后由Agent自动执行，不要求用户运行这些命令。

更早批次存在回归时可使用 `batch <较早批次> --repair --note "原因"`；前提是没有in_progress且目标已有非done任务（先按依赖reopen）。它保存历史，只允许后退，不能越过未完成批次向前跳。正常前进仍要求当前批次全部完成。
