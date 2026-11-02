# B04-T02 唯一 Responses 入口的离线检查点

本记录是T02第二个本地检查点，**不代表真实API验收完成**。用户已指定 `ai.1343263.xyz/v1/responses`、`deepseek-flash`、单次10,000,000 Token总预算，并明确允许发送1208字节虚构纸灯箱R7。真实密钥只通过本机masked窗口输入；本检查点没有读取真实密钥或发送实际生成请求。原[缺配置时的本地记录](B04-T02-local.md)和失败历史保留。

交付[单次Responses协议](../technical/RESPONSES_API_PROTOCOL.md)、Python stdlib唯一adapter、严格Core配置/授权/预算协调器、API owner/report schema2、原生只读reader及本地masked辅助。新增 `api preflight/extract/inspect` CLI，旧项目Schema3、候选v1、fixture与Compiler保持原义。模型候选始终unverified。

## 实际检查

| 检查 | 实际结果 |
| --- | --- |
| Rust全workspace，锁定离线依赖 | 513通过，0失败，包括custom进程harness；[完整日志](B04-T02-api-evidence/logs/B04-T02-rust-final.log) |
| Python全套 | 58通过，0失败，31.621秒；[完整日志](B04-T02-api-evidence/logs/B04-T02-python-final.log) |
| Rust API Core / API store | 23 / 14项均在全workspace中通过 |
| Responses adapter / 本地key辅助 / 日志工具 | 28 / 18 / 12项均在全Python中通过 |
| cargo fmt/check/build | 实际退出0；fmt无输出，保留0字节原始日志 |
| devlog check / git diff --check | 通过；日志事实源仍不将T02标done |
| 独立回显审查 | Python21个变体符合预期、隔离当前Rust扫描实现8项通过 |
| 原生解码与Python交叉核对 | 六个合法JSON场景结果相同，未将隔离探针当整个workspace或duplicate-key独立证明 |
| 完整R7命令预检 | 新项目初始化、来源导入、原件移动后预检通过；1208B/SHA匹配、key_loaded=false、no_api_calls=true、预算预留66560 |

选定Python3.12.14，测试以 `NOETOME_TEST_API_PYTHON`、PDF的显式运行时环境变量设置实际绝对路径；仓库测试不写死开发机API Python路径。Cargo使用真实共享target构建树，工作树旧target不作为测试结果。

## 已修复的实际失败与边界

凭据原始扫描、严格重复键及非有限数检查先取得真实行为红，再取得绿。独立审阅进一步复现quoted scalar、多层Unicode转义、codefence前缀及未闭合内层token会进入raw；producer与Core均补有限逐层escape片段扫描。普通引号、Windows路径和非法literal escape仍可使用。原始失败结果和红绿日志均保存。

Core先用stub取得行为红；持久目录元数据句柄曾不能阻止真实改名，改用实际READ目录原句柄与祖先守卫。后续负例修复raw缺失时伪已知模型/用量、失败分类改写、completed message/content形状分类不一致、not_called与已收到raw矛盾，以及input本身超预算但total=null仍生成候选。用量未知的字段保持null，已知计数下界超额会阻止候选成功，不伪造provider total。

本地辅助的真实Windows共享读取曾阻止状态原子替换，旧代码误更新身份/内存finished，导致磁盘停在running；现在只有实际child退出、两管道排空且终态发布成功后才finished。忙碌只重试本地状态，不重启Core/API。持续发布失败保持固定错误。

本次第一次启动直接使用Cargo的两个硬链接CLI，被单链接保护拒绝，固定状态preflight_failed、Core PID=null、实际API请求0；旧现场保留。随后独立完整 `pwsh -STA -File` 合成入口验证：共享CLI被拒绝、独立副本展示真实masked窗口、等待状态后受控关闭退出2/cancelled、无Core/API。可见测试只单独显式运行，不进入默认套件；[原生证据](B04-T02-api-evidence/review/key-dialog-validation/standalone-ui-proof.json)。实际用户窗口使用最终构建的独立单链接副本，核对原构建SHA后再运行。

第二个实际窗口因 `Start-Process -WindowStyle Hidden` 的首个显示提示而原生不可见，Core未启动；Root终止该未开始请求的宿主，原状态保留。补修仅在自身Form的Shown事件核对当前进程拥有HWND并正常显示，控制台仍隐藏。同一生产启动方式的独立测试取得可见性红→绿；当前第三个实际窗口也已只读核对Form可见、ES_PASSWORD开启、console隐藏，无密码框内容读取，[当前窗口证据](B04-T02-api-evidence/review/key-dialog-validation/root-current-window-proof.json)。

随后原生取消路径150次中第132次复现Win32替换错误1175，并被旧wrapper混为ownership丢失。依据实际读取的[Microsoft ReplaceFileW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)契约（2026-11-02，62061字节，SHA256 `22b1cb56650976a854674cfb2eb7161be7407db6a716d2db0cbafb1bd2d5559c`），仅在1175且原状态及临时文件原生身份仍匹配时返回本地busy。受控保留身份/外来status/外来temp分别红绿验证；1176/1177及清理异常仍失败关闭，不重启请求。复跑150次无失败但未碰到1175，不把它当触发该分支的证明。最终全58及显式隐藏宿主窗口检查通过，原失败18项日志和150次诊断保存。

## 证据与未完成范围

[机器记录](B04-T02-api-local.json)保存本检查点的源、配置/授权/代码工作区指纹及79份原始证据文件，合计574697字节；[归档目录](B04-T02-api-evidence)只包含明确虚构材料、合成探针和脱敏本地事实，不含可执行程序、真实key或完整官方网页。官方文档的实际URL/日期/SHA见协议。本检查点限定离线及输入前的Core状态，不将随后用户可能启动的实际调用算作离线证明。

extract现场比较HTTP body原字节，reader按固定字段、schema、指令及完整units的语义核对，所有保存指纹仍绑定保存原字节。原配置/授权/Python/adapter路径只是provenance，不被reader打开或执行；不要求原文件继续存在。不宣称服务端签名或能够检测所有外部锚点连同审计一起重写。

T02仍须用户在本机窗口输入密钥后完成最多一次实际生成、取得严格合法且可重读的候选，并记录实际服务模型/用量、安全raw/canonical审计、原项目/源/哨兵不变及无key环境重载零API。科学正确性、冻结必要原子差异、PDF模型引用、批量/长任务和生产Vault尚未验收，T03未启动。
