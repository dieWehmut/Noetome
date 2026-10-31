# 小步自动执行工作流程

全项目方案在[标准入口](../README.md)固定，任务在[账本](../../dev-log/tasks.json)。规划覆盖整个V1，实现一次一个结果；连续开发授权下完成后继续下一项。

## 接手

读[AGENTS](../../AGENTS.md)、STATUS，运行plan next。resume恢复历史任务；start读取show卡片；batch仅在当前批次全完成后切换。保留Git现有改动，每批从整合且验证过的HEAD建立codex分支，批次内逐任务commit。

任务卡包含先读规范、步骤、产物、验收、边界、失败处理和检查。调查已有实现再动手。可拆小未开始任务，但保留原验收和依赖，不能删范围。

## 实现

先写暴露关键故障的最小行为用例，再实现当前职责。状态、预算、发布和并发必须有故障测试；纯文案无须机械测试。未实现能力明确返回错误，不能以空success占位。

子代理只分工当前任务的不重叠文件，主执行者整合、验证与更新账本。副代理不git、不改任务状态。接口变化同时更新规范及受影响任务，不把约定留在聊天里。

按[失败处置](../execution/FAILURE_PLAYBOOK.md)有限修复，不删除测试、标skip或放宽阈值。没有新的假设与观察不算进展。

## 验证和完成

```text
python scripts/verify.py B01-T01
python scripts/devlog.py complete B01-T01 --note "已实现具体行为" --validation "实际边界与验收结果" --artifact crates/noetome-cli/src/main.rs --report .artifacts/verification/B01-T01.json
python scripts/devlog.py check
python scripts/plan.py check
python scripts/check_docs.py
git diff --check
```

先改完全部源码与规范再verify。工具执行卡片命令、检查测试数量和必交付文件，并绑定源码/定义指纹。complete复查报告，不接受旧版本或别的任务报告。源码修改须重验。

指纹覆盖Git跟踪及非ignore文件、删除项，排除账本/生成日志与证据自引用。重要源码必须入Git或非ignore；锁文件和正式发行证据应跟踪。这是防旧报告误用的校验，不是防执行者恶意篡改的签名。

真实质量、实机、容量及24h按[发行门禁](RELEASE.md)执行。自动脚本不能凭空生成专家判断或运行环境；未运行不能通过。

## 提交和继续

检查暂存diff，只stage本任务，分步commit，最后同步日志。检查通过后推送并核对远端hash。普通推送已有授权，不反复确认。权限失败不能把本地提交称已上传。

再次plan next；batch建议通过devlog batch执行，后续卡均已定义。用户要求全自动时继续，直到完成或没有可执行项；要求单项修复则在该项范围结束。外部条件不足时明确恢复方法，不用假证据绕过。

改任务定义后plan render、devlog render，再check。已有状态仅通过工具；历史不能改写。已完成内容有实质回归时从下游reopen或新建修复任务，保留旧证据。

用户说“继续这个项目”即授权完整循环。Agent使用 `python scripts/autotest.py --task <ID>` 自动运行通用和任务检查，只有[自动测试说明](../execution/AUTOMATED_TESTING.md)列出的必要人验点才请求用户参与。

回归需要返回更早批次时，先block当前活动任务、按下游到上游reopen受影响任务，再 `devlog.py batch <较早批次> --repair --note "复现及影响"`。repair只允许向后，不能跳过前进门禁。仅刷新新候选的正式验收证据可在当前门禁任务完成，不抹去历史真实完成；涉及行为回归才按以上流程修复。
