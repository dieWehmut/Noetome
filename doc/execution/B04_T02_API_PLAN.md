# B04-T02 唯一 Responses 服务的执行计划

2026-11-02 用户补齐目标与范围：`https://ai.1343263.xyz/` 的 Responses 接口、`deepseek-flash`、单次总预算10,000,000 Token，并明确允许外发冻结的1208字节虚构纸灯箱R7。当前只推进同一T02；T03仍等待实际验收。旧阻塞、离线验证及提交均保留历史。密钥尚未输入。

## 固定契约

- 目标固定为 `https://ai.1343263.xyz/v1/responses`，不自动换接口、模型或服务。用户提供的模型名按`deepseek-flash`处理；网站自称糖猫云，模型别名映射及实际兼容性以本次返回事实记录。
- 根据实际读取的[Responses create](https://developers.openai.com/api/reference/resources/responses/methods/create)及[Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)，使用`text.format=json_schema/strict`、完整文本输入、输出1024 Token、30秒、一次POST、无重试；禁工具、检索、流、背景和服务端存储，截断策略disabled。官方契约不替代所选服务实测。
- 继续使用T01严格领域请求/Outcome/CandidateIr。完整units包含Formatting；最终证据只许真实非Formatting来源单位。模型返回紧凑的unit ordinal、完整连续原文statement及qualifiers，适配器唯一匹配并计算UTF8证据区间与SHA、映射真实span UUID；不把独立expected提供给模型，不人为缩成几个原子以求通过。
- 调用前核对完整来源、配置、授权和预算的实际字节/身份/SHA。Token预算用明确保守客户端预留及实际报告用量分别记录，不承诺未知服务计费或将未知费用写成零。请求文本上限8KiB，元数据/HTTP body有64KiB独立上限；原样文本只发送一次。
- 密钥引用固定`env:NOETOME_AI134_KEY`；本地PowerShell7/WinForms隐藏输入，仅将值放入本次Core子进程环境。Core准入后读取，provider子进程仍只SystemRoot和既有三管道/Job，凭据只stdin帧。关闭后需重新输入，不注册系统变量，不保存密钥或其派生指纹。
- 新API owner/report与旧v1候选报告分离。保存无认证头HTTP body、严格安全原始响应、canonical Outcome、实际模型/用量/HTTP发送事实及候选；report最后发布。中断现场不自动重试，未知远端状态保持unknown，不以本地PID推定发送。

## 实现和验收顺序

1. Python标准库唯一适配器：`sidecars/responses_api/adapter.py`与离线测试。先真实行为红，再验证紧凑模型JSON、strict/duplicate/unknown/UTF8、实际字段、凭据抑制、TLS单POST/no redirect/no retry、超限/超时/失败。提供纯离线body/response模式用于独立协议核对；Core重载原生重建，不启动报告所称程序或其他子进程。
2. Core与持久边界：`model_api.rs`负责严格config/authorization/report、源/预算准入、调用与canonical验证；`api_store.rs`只负责新根、库存、nlink/native守卫、原子最后report及只读保护。CLI增加`api preflight/extract/inspect`，旧入口和Schema保持原义。
3. 本地隐藏输入辅助：`scripts/invoke_api_with_key.ps1`先校验/锁定实际CLI PE和SHA、检查独占新输出，mask输入并禁重复请求；所有stdout/stderr被排空抑制，状态文件只固定阶段/PID/退出码。使用合成子进程验证后才由Root打开真实窗口。
4. 独立审查及全离线检查：Rust fmt/check/test workspace、Python unittest、devlog check、git diff check；保留实际红/绿、源和外部哨兵。分步提交，所有本轮适用检查通过后推送。
5. 准备真实R7新项目、显式config和授权，实际CLI全链预检。打开本地密钥窗口，用户只需输入密钥；最多一次生成后核对安全响应、实际模型/用量、unverified候选、原项目/原件/哨兵、真正只读重载零API。实际冒烟未完成不得complete T02。

科学正确率、必要原子遗漏的独立核对、PDF模型证据、生产Vault、批量与长任务仍按后续任务分别验收。不能用本地模拟报告或合法JSON代替真实调用和科学质量。
