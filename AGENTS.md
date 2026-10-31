# Noetome 开发指引

自动使用完成当前任务所需的工具和适用插件。Git工作按任务拆成可审查的小步提交，适当按任务建立分支，全部相关检查通过后推送远程。用户已授权普通提交和推送，无需重复询问。

## 每轮先读

1. `dev-log/STATUS.md`：当前批次和进行中任务。
2. `dev-log/TODO.md` 与 `dev-log/tasks.json`：真实待办、依赖及验收。
3. `doc/execution/NEXT_STEPS.md`：当前任务的执行步骤。
4. `doc/README.md` 及与本任务相关的标准文件。

不要仅凭聊天摘要判断“上次已做完”。不存在的产物和未运行检查都不能算完成。

## 标准文件路径

| 类别 | 路径 |
| --- | --- |
| 产品开发需求 | `doc/requirements/PRD.md` |
| 已采纳决策 | `doc/requirements/DECISIONS.md` |
| 工程架构 | `doc/technical/ARCHITECTURE.md` |
| 最小数据契约 | `doc/technical/DATA_CONTRACTS.md` |
| 用户界面和输出设计 | `doc/standards/DESIGN.md` |
| 测试与验收 | `doc/standards/TESTING.md` |
| 开发工作流程 | `doc/standards/WORKFLOW.md` |
| 分阶段路线 | `doc/execution/ROADMAP.md` |
| 最近最多三步 | `doc/execution/NEXT_STEPS.md` |
| 新任务卡模板 | `doc/templates/TASK.md` |
| 日志规则和工具 | `dev-log/README.md`、`scripts/devlog.py` |
| 已完成事项 | `dev-log/DONE.md` |

当前用户指令优先；产品需求和已采纳决策约束技术、设计及执行计划。发现实质冲突要记录并修正相关文件。D01至D15已经接受建议，照基线推进；明确要求以后实测的数值在对应任务补齐，不把它们重新变成待审批障碍。

## 范围与节奏

- 一次普通开发请求默认只推进一个可验收任务，完成该任务必要的实现、验证和修复后汇报。用户要求连续推进时可顺序做多个，每项仍独立验收和记录。
- 同时最多一个 `in_progress`；子代理只做当前任务内分工，不各自推进后续产品阶段。
- 每批最多3个详细任务，远期只维护阶段目标。当前批次完成后按真实结果滚动细化。
- 当前计划从CLI和本地确定性闭环开始。不要顺手加入GUI、全部模型SDK、OCR系统或完整知识图谱框架。
- 样例和fake provider要明确标记；它们不证明真实PDF、模型抽取或科学正确率已经通过。
- 临时Vault原型只写新的测试目录。人工保护未验收前不写用户生产Vault，不扫描未经选取的私人资料。

## 开发日志必须随工作维护

`dev-log/tasks.json` 是唯一事实源。`TODO.md`、`DONE.md`、`STATUS.md` 为生成文件，禁止手工改状态。每轮执行者自动完成以下步骤，无需用户提醒：

```text
python scripts/devlog.py start <ID> --note "本轮范围"
python scripts/devlog.py complete <ID> --note "完成内容" --validation "实际运行的检查和结果" --artifact <仓库内产物>
python scripts/devlog.py check
```

失败可修复则继续修复；真实阻塞用 `block`，解除用 `reopen` 后重新 `start`。新增任务只编辑定义并运行 `render`；已有状态和历史通过工具维护。当前批次验收后用 `batch` 切换，不自动启动下一项。历史只追加，不能抹去失败与重新打开记录。

完成说明必须附真实验证和文件产物，不能用计划、提交成功或文件存在代替验收。不要把账本和生成日志作为自身完成证据。工具保存证据和指纹，不会自动证明测试真实。

## 验证与Git

- 先看Git状态，保留用户改动。分支默认 `codex/<task-id>-<topic>`，避免多个代理同时改同一文件。
- 当前文档和日志工具检查：`python -m unittest discover -s tests -p "test_*.py"`、`python scripts/devlog.py check`、`git diff --check`。
- 改PRD后执行 `python scripts/build_prd.py`；只修改Markdown源，不手改生成HTML。
- Rust工具链建好后按任务运行 `cargo fmt --all --check`、`cargo check --workspace`、`cargo test --workspace`；新增能力还需任务指定的行为验证。
- 命令里的 `python` 指可用的选定解释器。使用Codex提供的工作区运行时时用其返回的绝对路径，不把本机路径写死进仓库文件。
- 一项工作可拆多个有意义commit，最终提交同步标准和日志。检查通过后 `git push -u origin <当前分支>`，核对远端结果；不force push，不擅自修改全局身份。
- 结尾说明改了什么、如何验证、仍未完成什么及下一步。不能宣称未执行的兼容性、真实性或性能测试通过。
