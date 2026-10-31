# Vault发布 人工保护与恢复

本文件固定V1对文件系统的副作用。领域实体以[DATA_CONTRACTS](DATA_CONTRACTS.md)/[schema.sql](schema.sql)为准，运行状态见[RUNTIME_PROTOCOL](RUNTIME_PROTOCOL.md)。下述Windows操作是必须实现并经故障测试证明的方案，不是已测结论。保护门禁通过前只写新的临时Vault。

## 1 不变量与可发布单元

1. 原始资料只读；不改未知文件、`.obsidian`或其他项目的归属标记。
2. 已发布基线B、发布时实际文件C、候选N分别存hash和对象；C≠B意味着可能有人改，不用模型判断“改动不重要”。
3. 审批绑定确切B/C/N及影响闭包、配置revision、校验版本；批准后任何相关输入改变即过期。
4. 所有替换有可读原件备份及持久journal；恢复绝不覆盖未知外部新内容。
5. 发布不是跨文件原子事务。UI、报告必须允许中断和部分完成状态，不能把单文件rename称为整个Vault原子发布。
6. 输出按实际最终文件再次检查覆盖、资源、文件/标题/块链接；新计划全部合格不代表新旧混合结果合格。

Release状态固定`staged → verified → publishing → published`。staged/verified校验失败保留原状态及issue；写磁盘后失败进入`recovery_required`。恢复可继续到published或完成补偿到rolled_back。published是历史终态；用户撤销产生新的逆向Release，不篡改旧Release状态。

最小可发布单元为依赖闭包：笔记正文、引用目标、资源、索引、canonical placement及其映射一起考虑。拒绝/未批条目及所有依赖它的变更从候选子集排除，再为“旧基线+获批子集”重建索引和映射并重新校验。不能仅删一行diff后沿用原覆盖结果。不能形成合格闭包则保持待处理，不发布断链结果。

## 2 B C N与人工编辑判定

| 情况 | 处理 |
| --- | --- |
| B不存在、C不存在 | 新文件候选，可按普通审批级别处理 |
| C=B且未触及锁/重要结构 | 普通自动或待批，取决于CONFIG |
| C≠B，包括用户删除文件 | manual_conflict，必须具体审批；自动模式也不能豁免 |
| C存在且从未在项目release_file中拥有 | unmanaged，禁止覆盖或默认接管；换新路径重新规划 |
| N=B或N=C且引用/映射也不变 | 幂等无字节修改，仍校验归属和引用 |
| 文件/目录结构锁或影响大规模重构 | 绑定影响闭包的重要审批，不逐文件绕开锁 |
| B对应原路径但C重命名至未知位置 | 未能证明身份前按缺失及未知文件处理，不凭标题猜测 |

V1采用整文件保守保护，不自动做三方语义合并。正文中的生成标记仅辅助定位，删除或伪造标记不取消人工保护。保留用户内容可选择拒绝、另存候选、有限编辑候选或明确接受整文件变更；报告显示将失去的C内容。删除与重命名同样需要B/C校验。

FR-32有限编辑候选入口固定为`changes edit --id <id> --file <candidate-json>` / IPC `changes.edit`。只编辑本ChangeSet的typed NoteNode内容、排序和可编辑元数据，不是任意文件浏览器。保存为新PlanRevision/ChangeSet、记录actor=human及人工来源；新增实质陈述建立manual Evidence/Claim或保持unverified，不能冒用原证据。重新计算节点/原子映射、正文关联、覆盖、语义、视觉需求及B/C/N；旧审批转stale。UI提供结构化文本/公式/表格字段及预览，完整Markdown编辑器仍可后续，但不可省略此功能。

## 3 审批CAS

ChangeSet状态`candidate/pending_approval/approved/rejected/stale/applied`，Approval状态`pending/accepted/rejected/stale`。审批请求携带change_set_id、expected_change_set_hash、actor和decision；hash覆盖排序后的file_change、change_dependency、B/C/N哈希、snapshot/plan/config/validation及锁的revision。

接受审批的短事务比较当前revision/hash与请求，不一致返回`APPROVAL_STALE`并展示新差异。approved只表明接受当时的候选，不能跳过发布前磁盘复查。自动审批也生成同样记录，actor=policy且规则版本固定；manual_conflict/锁冲突不允许policy接受。

发布前在可靠句柄取得后重读C与文件身份，核对批准指纹；路径、正文、资源、源revision或配置发生变化使整个受影响审批过期。没有“只要几秒内就算新鲜”窗口，也不使用mtime/文件大小代替hash。

## 4 发布前准备

1. 取得项目worker独占权和Vault发布锁，验证owner.json的project_id/格式版本；未知归属不接管。
2. 验证本地NTFS同卷、普通目录、父链无reparse、所有路径合法、空闲空间可容纳候选+备份+日志。拒绝硬链接计数>1、junction、symlink、ADS和未知目标，避免修改其他名字指向的数据。
3. 生成不可变manifest，含所有操作顺序、相对路径/path_key、B/C/N、对象hash/长度、依赖、审批、配置/计划/校验ID和允许的最终状态。写项目publications与同卷临时区，flush后重读hash。
4. 将候选复制到Vault同卷`.noetome-stage/<release-id>/new/`，先写临时名、FlushFileBuffers、校验再定位；不在正式目标上边写边验证。
5. journal第一项prepared包括manifest hash及当前release基线；持久化后才有正式文件操作。预检不足仍保持verified，不触碰正式文件。

`.noetome-stage`需项目归属标记，不能清理碰巧同名的用户目录。保留备份与journal直至数据库提交和最终复核完成；失败现场不自动删。文件对象和源区域原件仍由project objects保存，不依赖这个临时目录维持长期回滚。

## 5 Windows句柄级写入算法

本算法专门避免“检查hash、关闭句柄、路径覆盖”的TOCTOU。所有已有受影响目标按稳定path_key顺序一次取得句柄；取得失败则在尚未修改正式文件时退出并报`VAULT_BUSY`，不杀用户程序、不放宽共享模式、不无限重试。

- 目标文件用CreateFileW打开，desired access至少`GENERIC_READ | DELETE`，share mode仅`FILE_SHARE_READ`，OPEN_EXISTING，打开时防止跟随reparse。`DELETE`权限用于后续同句柄rename；不授予外部write/delete共享。
- 父目录以目录句柄打开并验证卷ID、文件ID、reparse信息，持有防目录删除/替换的句柄；已有目标文件与父链身份均再次核对。目录句柄不等于锁住所有子文件，不能据此宣称没人能在空名上新建。
- 候选也保持具备DELETE权限的句柄，校验hash后不再允许改写。日志和输出的句柄访问/共享须兼容，不得因自身重复打开产生死锁。
- 持有deny-delete的目标读句柄时再调用路径`ReplaceFileW`会与自身共享约束冲突，明确禁止该方案；也禁止为了ReplaceFileW先关闭保护句柄。
- 使用持有目标句柄调用`SetFileInformationByHandle(FileRenameInfo)`，把旧文件移动到同卷唯一backup名，`ReplaceIfExists=false`，目的目录使用已验证目录句柄作为RootDirectory。移动意图必须先flush journal。原目标句柄一直保持，不释放对旧内容的保护。
- 旧名腾空后，使用候选句柄将候选移动到正式目标名，同样`ReplaceIfExists=false`。如果其他进程在间隙创建同名文件，安装必须失败并保留外部文件、原备份及候选，转recovery_required；不得改为覆盖或删除那个新文件。
- 新建文件没有C，仍使用no-replace安装。删除操作只是旧文件移到backup，不永久删除；重命名/路径交换先把所有旧项移到唯一中间位置，再安装新项，包含Windows大小写仅变化场景。

每一步核对返回错误码、目标file ID/hash及期望存在关系后才记录完成。所有正式已有文件先取得锁并完成CAS，再备份旧项；安装顺序为资源→普通笔记→索引，删除关联已在计划闭包中校验。不能把“先备份、后安装”中途状态展示为published。

若句柄级rename、RootDirectory、共享模式或所需权限在最低支持系统上实测不满足，禁用生产发布并修复这一路径；不退回关闭锁后rename、复制覆盖或“最后写入者胜”。B11必须有独立外部进程持写句柄、原子保存、人改、路径重建、磁盘满、访问拒绝及每步崩溃注入。

此策略保护可检测竞态，不能阻止管理员、恶意内核或断电设备违背持久化要求；发布后外部编辑仍属人工内容。官方API行为支撑选型，但不替代本仓库测试：[CreateFileW](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)、[FILE_RENAME_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_rename_info)、[SetFileInformationByHandle](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle)。

## 6 Journal与跨DB文件提交

journal是UTF-8 JSONL，每行有release_id、sequence、operation_id、kind、expected_before/after、prev_record_hash、record_hash、UTC；整行flush后才执行其副作用。尾部半行按未提交记录处理，不能跳过中间坏hash。`publication_step`保存已导入镜像，磁盘journal为文件动作恢复依据，DB不可伪造尚未发生的动作。

事件固定`prepared, locks_acquired, operation_intent, old_backed_up, candidate_installed, operation_verified, all_verified, db_committed, completed, recovery_required, compensation_intent, compensated`。每个动作有单调operation_id；同动作恢复先核对实际路径/ID/hash，再决定是否已完成，不能仅因上次没有“完成”行就重做。publication_step只镜像文件动作：operation_intent映射intent，candidate_installed映射new_installed，operation_verified映射verified，其余文件状态使用同名值；全Release事件留journal及audit_event，不能向SQL CHECK写入其他kind。

执行顺序：候选+manifest持久化 → journal prepared → 锁/CAS → 逐操作intent/副作用/核验/完成 → 全部实际输出hash/链接/覆盖复核 → journal all_verified → 单SQLite事务写Release published、release_file、ChangeSet applied、事件及project_head → journal db_committed/completed。project_head按准备阶段记录的revision做CAS，成功后递增并指向实际发布的snapshot/plan/release；不能按创建时间猜当前基线。structure_lock变更也递增revision，纳入审批hash。数据库提交后日志尾写失败，重启按release和manifest核对并补尾，不重复发布。

FlushFileBuffers用于文件内容和journal持久化；目录项耐久性及断电恢复在NTFS上实测，不能仅以flush函数返回宣称硬件断电绝不丢数据。[FlushFileBuffers](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers)

## 7 故障恢复与回滚

启动发现publishing或无completed journal，进入recovery_required并阻止新发布。`vault recover`读取manifest、有效journal及DB，在同一保护条件下核对old/new/backup各路径身份和hash：

| 实际状态 | 行为 |
| --- | --- |
| intent有，动作未发生，所有预期仍匹配 | 幂等执行该动作 |
| 动作已发生且新位置ID/hash匹配 | 补记完成，继续 |
| 全部N正确而DB尚未提交 | 重做全库校验后提交同release |
| C仍是预期旧内容，未开始安装 | 可安全保持旧基线，释放锁 |
| 备份完整且已安装候选未被外部更改 | 允许按逆序补偿，每步也先journal再no-replace |
| 任一目标为未知hash/新file ID、备份丢失或journal破损 | 保留全部现场，blocked_input，列出需处理文件；不自动覆盖 |

补偿先把仍匹配N的正式条目移动到唯一恢复位置，再将B备份no-replace还原；间隙若被新建占用仍失败保留，不能强行还原。恢复动作可以多次中断并续作。成功补偿标rolled_back且生成报告，历史操作和问题不删除。

用户主动rollback不把整个旧目录快照覆回。以最近已发布状态为B、磁盘当前为C、目标历史Release为N建立逆向ChangeSet，重新跑锁、依赖闭包、覆盖、审批及发布；回滚期间的人改仍受保护。恢复旧数据库不能替代Vault回滚。

## 8 迁移 备份与导出

数据库迁移取得项目独占权，无活动Run和待恢复发布；关闭工作连接，checkpoint可执行时完成，然后使用SQLite在线Backup API产生一致备份，执行integrity_check/foreign_key_check及项目ID核对。直接复制WAL模式中的主db不算备份。[rusqlite backup](https://docs.rs/rusqlite/latest/rusqlite/backup/index.html)

schema0→schema1及以后逐版本迁移在事务内，校验已应用迁移checksum，失败回滚保留旧schema及备份。迁移成功后更新project.json身份版本，配置替换用临时文件和恢复记录；启动遇到DB/config版本不一致进入明确恢复分支，不能默默挑一个值。更新的数据库版本拒绝旧程序写入。备份保留策略以CONFIG/RELEASE为准，不因升级成功立即删唯一旧备份。

导出写新目录：固定Release文件、IR/来源索引、相对可移植路径、版本/校验manifest和可选原件；去除密钥、用户绝对路径及私有运行日志。重新打开导出包离线核验链接、hash和对象引用。导出成功不表示用户生产Vault已更新。恢复测试和迁移失败测试必须覆盖已有数据与来源hash不变。
