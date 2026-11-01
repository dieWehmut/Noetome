# 离线 fake 响应模板 v1

这是显式模拟Provider响应材料，供私有 `model_candidate` 测试target重放。它由作者在冻结独立 `model-candidate-v1/source.md` 与 `expected.json` 后，逐句审阅并手工编写；它不是待测抽取结果，也不是独立预期清单。冻结的作者原文和expected保持不变，测试不得用此模板替换或反向生成expected。

模板只含12条 `statement`、完整 `qualifiers` 和精确 `support_quote`。私有测试子进程只从stdin收到的 `ExtractionRequest.source.units` 中查找支持句，生成真实span内UTF-8半开局部区间与片段SHA。子进程不接收项目路径，也不读取原项目、源文件、expected、凭据或生产Vault。

外层请求身份、来源SHA、provider与actual model来自收到的请求；候选和引用仅使用该请求允许的来源单元。模拟用量固定为input160/output400，`provenance=simulated`，费用为unknown/null/null/not_available。这些数字是fake政策检查数据，不是实际Token统计、真实计费或性能测量。

私有argv入口为 `--fake-child <mode> <绝对模板路径>`，只存在测试二进制中。普通生产CLI不提供fake入口或环境开关。合法模式与认证、限流、能力不足、远端未知、空/截断/超限、超时、凭据回显、重复/未知字段、坏引用和Token超限模式分别验证有界Provider及严格Core处理。

模板可以构成离线契约输入，但不能证明科学正确性、真实模型抽取、真实PDF支持、独立样例质量门槛或PRD正式200原子/98%目标已经达标。
