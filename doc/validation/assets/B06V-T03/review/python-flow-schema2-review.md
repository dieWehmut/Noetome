# 独立流程 profile Schema 2 读取与未批准回退

本记录接续 [Python 能力门禁基线](python-capability-baseline-review.md)，只增加严格 Schema 2 读取。能力允许表未增加当前稳定组合，作者 expected 及两份兼容冻结包未修改。原基线记录保留其当时源码与 141 项实际结果，不覆盖为本轮状态。

`verify_process_flow.profile_schema` 仍要求完整字段清单和严格布尔值。Schema 1 的 `mermaid_version` 必须是显式字符串，`renderer` 字段完全不能出现，连 `renderer:null` 也拒绝。Schema 2 必须保存 `mermaid_version:null` 和完整 `renderer:{name,version,source_sha256}`；renderer 名称、版本为明确字符串，来源 SHA 必须恰为 64 位小写十六进制。未知 Schema、整数/布尔/浮点混用、字段缺失、多余字段、非法 SHA 与两版字段混配均拒绝。

新的完整候选 profile 在没有能力登记时，明确过程仍输出有据 `ordered_list` 并记录 `FLOW_TARGET_UNVERIFIED`。renderer 版本、SHA、app、profile_id 或 HTML 标志的替换不能批准目标；来源只说明相关性、关系方向未知、缺判据或分支不完整时继续语义阻断。读取合法 profile 不等于其原生渲染能力已验收。

新增 6 项 Schema 2 测试及原回归实际执行结果为：流程 34 项、registry 8 项，共 42 项，42 通过、零跳过、零失败。完整 stdout/stderr、运行前后源码/旧流程作者/原 13 项与补充 3 项冻结包指纹见 [actual-checks.json](python-flow-schema2/actual-checks.json)，`git diff --check` 退出 0。

尚未执行 Schema 2 普通 CLI 审计：本轮不能使用旧二进制证明新 DTO。新 CLI 由根代理编译并明确提供后，再运行合法未测目标的实际回退和造假拒绝检查。当前稳定原生语义显示、能力登记、完整平台回归及模型请求均不从这 42 项测试推定；此分工实际模型请求为零。
