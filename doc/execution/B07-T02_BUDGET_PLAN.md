# B07-T02 持久预算预留下的有界重试与隔离

当前只推进本项，T03保持todo。依据[B07执行卡](B07_STEPS.md)、[FR-36/38](../requirements/PRD.md)、[预算契约](../technical/REQUEST_BUDGET_CONTRACT.md)，真实状态由[账本](../../dev-log/tasks.json)生成。

## 范围与已有依据

依赖B07-T01已真实完成并推送，基线commit为1612f200c847b94bb055c09eaa1d0651f42650d5，本轮已重新核对远端HEAD。复用旧单请求Responses适配器、来源与候选严格校验；只允许数字环回synthetic服务与人工凭据。全局预算使用新的明确synthetic authority，已有真实累计额度、DPAPI凭据和生产Vault不读写。

不新增模型SDK、真实网关重试、GUI、OCR、知识图谱或生产发布。未知费用保持null，Token预留不承诺服务商账单绝对上限。参数/合同和实现骨架不作为验收证据。

## 1 先验作者与RED

冻结新的run-api-budget-v1作者包：原source/gold/payload字节、旧来源pins、固定响应/故障模板、有限策略、14个核心case和独立预算expected。旧Core05在新测试源项目以max_requests=0完成两次原API正文预检：5238B正文、6262输入预留、2048输出、R8310；预算近限固定9009，合法用量700后仍无第二个R空间。实际HTTP/模型/付费0；首次预检包装器解析project init文字为JSON的失败原件保留，不能把字面source_unchanged字段当历史前后库存证明。

原件和expected先单独提交，再用普通旧CLI保存缺少新入口的真实RED。Source/expected不从新调度报告产生；UUID等动态槽位必须精确列出，其他正文逐字核对。

## 2 实现与分工

Root负责原API的crate-private最小hooks、有限调度/CLI、输入及输出保护和标准同步。一个Rust分工只写request_budget_model/store的新两模块，metadata/events完整链重建、原子预留、单次发送许可、未知和幂等结算；一个作者/验证分工维护新fixture与独立Python预算检查器；第三分工只读核对原件、协议和实际Windows/Linux验收。只一个in_progress，不推进后续产品阶段。

事务外并发调用原API，每个新attempt重新预留；HTTP客户端本身仍禁retry。Prepared原件先固定，Reserved到MaySend事务取得后重查预算、终态、期限和输入。已持久真实响应与结算恢复不再次HTTP；没有可信最终回执时保留R。单元失败和资金使用分别保存，最终报告逐字段说明成功、隔离、预算停止或未知。

## 3 实际行为与收束

先执行独立预算/原件unit检查，再运行真实两并发单元及跨Run近限、429/503/timeout/断连/结构失败/同SHA两个请求、opening和既有消费、新Run/变更拒绝。实际测量服务器接收、退避、网络和绝对时限；调用前失败与未知完成分开，所有失败原件保留。

在预留提交后、MaySend后/服务器实际接收后、response+transport共同sync后、结算提交后、retry_at保存与等待中分别实际终止own PID并wait，再用普通CLI恢复。逐条重建原确认用量/未决R、身份和原文件字节；第二次恢复不得再结算或重发。

最终冻结同源CLI/真实test driver与源码清单，Windows运行fmt/check/test/build和完整Python，Linux完整Python及本项行为；完整Linux Rust依既有Windows-only宿主边界准确标未运行。确认两个PowerShell引擎的旧凭据测试均被发现。独立审查、devlog complete/check、diff/Git原字节回读通过后分步提交推送并核对远端。尚未运行的测试、兼容或性能不记通过。
