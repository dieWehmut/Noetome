# 近期开发步骤

真实状态以[任务账本](../../dev-log/tasks.json)和[当前状态](../../dev-log/STATUS.md)为准。B06 的主动比较表、有据过程流程、明确公式保真分别按[B06-T01](../validation/B06-T01.md)、[B06-T02](../validation/B06-T02.md)、[B06-T03](../validation/B06-T03.md)验收；设置侧栏和本地重装见[B06U-T02](../validation/B06U-T02.md)。执行范围归档于[B06 步骤](B06_STEPS.md)。

B06V 三项现已实际验收：[一般图像](../validation/B06V-T01.md)、[单页扫描公式](../validation/B06V-T02.md)、[最低与当前稳定兼容矩阵](../validation/B06V-T03.md)。T03 覆盖 Windows Obsidian 1.6.7/1.15.0 明暗64唯一组合，完整 Windows Rust726、Windows Python441（434通过/7跳过）、Linux Python380（361通过/19跳过）、五视觉317次CLI及有编译来源锚的Windows30目标命令和15来源setup；科学质量仍 not_run、知识仍 pending、生产发布未进行。T03 收束提交 b7f60ac 已普通推送并核对远端一致，失败和原件保留，归档卡见[B06V 步骤](B06V_STEPS.md)。Linux完整Rust、其他平台原生渲染及未运行规模指标不从这些结果推定。

依据[独立进入评估](../validation/assets/B06V-T03/review/b07-entry-assessment.md)，当前批次只展开[B07 三张有界可靠维护执行卡](B07_STEPS.md)。B07-T01 已按[实际验收](../validation/B07-T01.md)完成有限流程实现和最终回归；状态由日志工具维护，T01收束已推送并核对1612f20；本轮按持续开发指令通过日志工具启动T02，T03保持 `todo`。本轮执行细节见[T01计划](B07-T01_RUN_PLAN.md)及[检查点契约](../technical/RUN_CHECKPOINT_CONTRACT.md)。

## 1 B07-T01 一个固定有限流程的持久检查点与恢复

依赖B06V-T03。冻结一个确定性fixture有限工作图，新增严格Run/WorkUnit及输入、依赖、策略、版本、状态、输出和检查点。每个持久边界真实强制终止后恢复，复用已确认单元；规范内容和稳定知识/计划ID与从头运行一致，运行UUID、时间和attempt审计分别保留。暂停、取消、版本失效、未知检查点和输入/owner保护实际验证后，完成完整回归、独立审计、日志及提交推送。限定新测试项目/Vault，不扩大到全部知识命令或24小时恢复。

作者包 [run-checkpoint-v1](../../fixtures/run-checkpoint-v1/README.md) 与旧CLI真实RED先冻结；最终Linux矩阵95通过、Windows原生94通过/1 POSIX权限not_run，分别320/319普通CLI，39实际持久边界逐一强制终止恢复，独立双线程竞态及迟到控制确认通过。Windows完整Rust756、最终Windows Python477（469通过/8跳过）、Linux Python416（395通过/21跳过）与原件/来源审查均已实际完成；Native08 Python以四份精确不变Rust输入清单复用Native06 Rust，不声称再次运行Cargo。失败历史、当前完整程序体和14原始ZIP见[验收记录](../validation/B07-T01.md)。Linux完整Rust、24小时和生产Vault恢复not_run；当前T02已启动，T01实现及原件作为已验收依赖保留。

## 2 B07-T02 持久预算预留下的有界重试与隔离

本项现为 `in_progress`；执行细节见[预算方案](B07-T02_BUDGET_PLAN.md)和[预算契约](../technical/REQUEST_BUDGET_CONTRACT.md)。旧Core零请求预检已固定R8310/near9009，作者原件/故障expected正在先验冻结；代码和计划不算真实恢复验收。

依赖B07-T01。只接一个既有单请求适配器，使用人工凭据和固定数字环回synthetic HTTP。请求前持久预留，近限额并发、有限退避、次数/Token/耗时、未知完成及重启结算独立核对；幂等绑定逻辑attempt，不按相同响应SHA合并不同请求。单元隔离须服从Run/全局预算，到限停止所有新请求。真实模型/付费调用0，既有累计授权不重置；通过完整回归及独立预算审计后收束。

## 3 B07-T03 明确托管测试集合的人工审批与可恢复发布

依赖B07-T01和B07-T02。限定至多两笔记、一个PNG及明确链接/修改/删除/重命名操作；新的owner测试集合先建立并独立核验可信初始基准，已有陌生无基准Vault继续只读。比较基准、当前磁盘与新候选，审批绑定当前字节和影响范围，写前编辑令批准失效，未批准覆盖为0。暂存重验后日志落盘，每个边界实际终止或文件占用后恢复一致版本或待恢复；回滚也保护后续人工改动。通过独立三方审计和完整回归后再规划下一批。

仍未完成的正式门槛包括科学200原子/98%基准、通用OCR、完整多教材知识编译、生产Vault人工保护、千页/万原子、3000笔记/200人工改动及24小时规模。当前小型工程任务不代替这些实测。
