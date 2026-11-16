# 已授权的模型服务与复用规则

2026-11-12 用户授权的整书连续处理沿用下列服务、模型、DPAPI引用与累计10M额度，替代历史小样例的“仅一次、不批量”调度限制。B16-T03通过[生产持久编译契约](../technical/PRODUCTION_COMPILE_CONTRACT.md)自动拆分全部物理页，每个网络请求仍有独立输出/时间/预算限制且不隐式重试。现有十条历史及33,185已结算Token作为迁移基线保存；当前余额必须读取实际外部权威，不把本段快照当作每轮新额度。原JSON迁移后保持原字节且设为只读，旧JSON写入助手拒绝；新请求统一在外部SQLite预留、结算和恢复，未知消费保持承诺。下文历史单请求规则与原始失败继续保留其当时范围。

2026-11-03 用户为持续开发明确提供以下配置，并要求保存、以后复用。D08 的服务和预算条件已经满足；密钥是本机外部凭据，不能因为缺少密钥值再次索要这些已知设置。

| 项目 | 已授权值 |
| --- | --- |
| 当前接口 | OpenAI Chat Completions；原 Responses 配置及调用历史保留 |
| 服务 | 糖猫云，`https://ai.1343263.xyz/` |
| 当前请求端点 | `https://ai.1343263.xyz/v1/chat/completions` |
| 配置模型 | `deepseek-flash` |
| 累计模型预算 | `10000000` Token；输入、输出及返回的 reasoning 都按实际 usage 对账 |
| 单次验证限制 | 一次有界请求；不自动重试或批量 |
| 凭据环境变量引用 | `NOETOME_TANGMAO_API_KEY` |
| 价格和费用 | 网关有效价格尚未核实，保持未知；使用已授权 Token 控制 |

该服务是用户选择的网关。DeepSeek 官方文档是请求协议与配置模型的能力依据，不能据此推造网关实际路由的内部模型版本、官方费率或用户账单。实际返回 model、usage、HTTP状态、策略与原响应分别记录。

## 本机保存位置

非秘密模型设置位于当前用户 `%LOCALAPPDATA%\Noetome\model-services\tangmao-deepseek.json`，累计使用记录位于同目录的 `tangmao-deepseek.usage.json`。凭据位于 `%LOCALAPPDATA%\Noetome\credentials\tangmao-deepseek.dpapi`，用 Windows CurrentUser DPAPI 保护；仓库只保存工具、协议、公开配置和验证证据。

使用 [set_api_credential.ps1](../../scripts/set_api_credential.ps1) 在本机遮蔽输入一次，或者从用户明确指定的环境变量保存。没有明文 key 参数，不读取默认认证文件、不覆盖已有凭据。已有凭据正常复用，只有确实缺失或认证失败需要用户更新时才要求新的 key；不在聊天、工具输出或仓库存储密钥值。

[invoke_api_credential.ps1](../../scripts/invoke_api_credential.ps1) 只读取选定配置和 DPAPI 引用，在内存解密并通过子 CLI 的环境变量调用既有 `extract api`；不把 key 放进参数、正文、日志或明文临时文件。它不新增模型路由、自动重试或批量行为。

从仓库根目录运行下面的命令可打开本机输入窗口，按 Save 保存一次；如果所选凭据已经存在，直接复用，不再次执行保存命令。需要终端遮蔽输入时去掉 `-Dialog`。

```powershell
powershell.exe -NoProfile -STA -File .\scripts\set_api_credential.ps1 -CredentialFile "$env:LOCALAPPDATA\Noetome\credentials\tangmao-deepseek.dpapi" -ServiceUrl 'https://ai.1343263.xyz/' -Model 'deepseek-flash' -Dialog
```

凭据调用脚本执行选定配置的单请求限额；累计使用记录由本轮执行者在每次真实调用前后核对和结算。不能把手动调用脚本等同于已经自动执行累计预算检查。

## 预算必须累计

每次真实调用前先读取本机累计记录，剩余额度等于已授权上限减已核实使用量和未结清工程预留。请求次数仍限制为一次、输出最多8192 Token，单请求配置总量不得超过剩余额度。发起前写入预留记录；有有效返回 usage 时累计实际 total_tokens，返还未使用预留；没有可核实用量的尝试保留预留与未知状态，不能当0消费或悄悄恢复额度。

保存10000000这个数字不等于每次获得新的10000000。后续任务先复用设置、凭据和剩余额度，在已有额度内正常推进；超过累计额度、新收费范围或改变服务时再按用户新指令处理。

## 真实验收范围

首次使用作者固定的小文本、来源版本/SHA及调用前gold；Core候选通过不代表知识质量已经独立核对。用户已在本机窗口保存所选密钥，真实DPAPI复用0请求预检通过，后续直接复用。首个Responses请求计量551输入/385输出/936总Token，未返回候选JSON；v2第二次计量1491输入/8192输出/9683总Token，JSON截断。精确Chat策略经独立审查、完整497项Rust回归及127份双报告核对后，已保存为同一服务的活动接口，原profile历史保留。第三次Chat计量1583输入/3329输出/4912总Token，返回完整JSON，但atoms缺少严格Schema要求的atom包装，候选失败。第四次v4在新鲜审查及完整498回归后计量1712输入/3071输出/4783总Token，嵌套结构正确，但Atom qualifiers缺少非空测量单位，Core返回EXTRACT_QUALIFIERS。四份原响应及调用前gold保持；累计20314、余额9979686、pending0，费用未知。第四次仍含reasoning_content，thinking={type:disabled}的实际执行效果未获证明。

v5只明确已有单位/条件与atom.qualifiers的逐字对应规则，严格Core契约不变。最终499项Rust回归、API84/fake54/Chat35和128双报告/90请求核对通过；899项新鲜审查通过后第五次实际计量1283输入/2196输出/3479总Token。完整JSON、嵌套、引文及限定均正确，但三个概念和一个原子的revision为0，Core拒绝EXTRACT_KNOWLEDGE；原第五响应保留。第五次结算后累计23793、余额9976207、pending0。第五回包未出现reasoning_content，这个单次观察不构成开关普遍生效或内部模型版本证明。

用户顶部栏要求已在B04U验收、安装并推送；B04R的v6所选Chat正整数revision Schema/提示修复保持原严格验证及旧Responses。native41/普通API7/fake3、Python76、完整Rust502/fmt/check、API84/fake54/Chat37及130双报告/92请求通过；30项新鲜组件审查通过后，第六次单次请求HTTP200/stop、CLI exit0，Core接受1个候选原子及1条证据，1792输入/4512输出/6304总Token已结算。当前累计 **30097**、余额 **9969903**、pending **0**。原响应与保存payload一致，前五响应和调用前来源/gold/SQL不变。第六回包含16715字节reasoning_content，thinking开关效果及底层模型版本仍未证明，费用未知。

严格API候选成功仍为knowledge_status=pending、scientific_quality=not_run、published=false；literal gold数值/完整条件集合的诊断匹配为0，独立知识核对留给B04-T03，不以这次API成功代替科学验收。T02最终整分支469项断言/55指纹审查通过，已通过日志工具complete；提交/推送核对后进入T03。以后直接复用已保存服务、凭据和累计余额，不重新获得10M，不再次弹出密钥窗口。

真实原响应、使用记录和严格候选核对完成后，才按任务卡验收B04-T02并进入B04-T03。假服务、公开synthetic key测试和0请求预检都不得称作真实模型成功；返回别名也不是网关底层模型版本或收费的独立证明。

## 2026年11月6日的实际状态核查

B06V-T02 对同一明确路径实测：服务设置、累计账本和 DPAPI 凭据三文件及对应父目录均不存在；当前 Windows 用户/LocalAppData 与原记录相同，Windows 与 Linux 的指定 key 环境变量均未设置。证据见[当前状态预检](../validation/assets/B06V-T02/recognition/vision-api-preflight/missing-current-state.json)。前文 30,097 / 9,969,903 / pending0 是最后有效结算快照，不能据此声称缺失文件后的当前余额已核实。

服务、别名和累计上限仍沿用用户原授权。已向用户核实文件迁移位置或恢复范围；在实际账本与凭据可用并完成预留核对前，不发送真实模型请求，不重新初始化 10M 或假零使用量。扫描方案的本地失败历史保持原响应，最终视觉客户端能力按[扫描契约](../technical/SCAN_FORMULA_CONTRACT.md)分别验收；官方视觉支持声明不证明糖猫内部路由或精确权重版本。

扫描调用新增独立的[invoke_scan_api_credential.ps1](../../scripts/invoke_scan_api_credential.ps1)，只启动四路径扫描 CLI，复用明确 DPAPI 引用，并锁定实际累计账本。它保守预留本次配置总上限、按有效 usage 结算、未知用量保持未结清；同时保存四阶段公开收据及账本原字节快照，保持中文 stdout 的 UTF-8 字节。旧 `invoke_api_credential.ps1` 仍只用于 `extract api`，没有因此新增累计账本自动控制。

封装的进程和管道共用有界等待，失败时终止本次 Job 内进程并保存 `failed` 收据及未知用量预留。Job 在选定进程启动后分配，保留该窗口局限；进程管理不是文件或网络沙盒。成功四阶段原协议保持，失败阶段单独记录诊断和终止证据。

新脚本已在两个 Windows PowerShell 版本上用人工凭据、人工账本和无 HTTP 的替身子进程验证；见[辅助脚本记录](../validation/assets/B06V-T02/recognition/credential-helper/validation-receipt.json)。这不证明当前真实文件恢复、真实凭据解密或模型调用成功。`set_api_credential.ps1` 只保存缺失 DPAPI 凭据，不创建服务设置或重置累计账本；恢复路径与额度范围仍需依据实际当前记录。

## 2026年11月6日用户授权恢复

用户明确说明：“新增用量我不知道，当没有吧。凭据我也不知道在哪，我重新输APIkey吧”。按这项指示将缺失期间的未知新增用量假设为0，完整恢复最后有效结算的六条记录：累计30,097、pending0、余额9,969,903、原累计上限10,000,000。该值是用户指示下的恢复基线，缺失期间用量和服务端余额没有独立核实；不是用户确认从未新增调用，也不是新一轮10M额度。

恢复来源、原字节指纹、六笔快照与用户假设保存在[独立恢复收据](../validation/assets/B06V-T02/recognition/credential-restoration/recovery-receipt.json)。外部账本、同服务视觉单请求配置与恢复旁证只新建缺失文件，原记录的全部字段和时间保持。恢复说明单独保存，不加入严格账本顶层。

用户已在本机遮蔽窗口重新保存 CurrentUser DPAPI 凭据；[保存结果](../validation/assets/B06V-T02/recognition/credential-restoration/native-key-save-result.json)不含密钥值。随后实际选定扫描CLI的[零请求预检](../validation/assets/B06V-T02/recognition/credential-restoration/zero-request-preflight.json)通过凭据解密并按预期返回SCAN_BUDGET_STOP，network/paid均0，恢复账本SHA未变化。真实识别仍需逐次预留、保存原响应及有效usage结算后验收，零请求预检不代表模型识别成功。

本轮已分别执行清晰、负号缺损、分母下标缺损及清晰的未经验证目标原图回退四次单请求，实际usage为654、776、1,070、588，共新增3,088。完整六笔历史不变，十条记录全部结算后累计 **33,185**、pending **0**、余额 **9,966,815**；[四次原响应与结算](../validation/assets/B06V-T02/recognition/vision-live-final/live-receipt.json)保存原字节和独立锚。清晰重建及原区回退通过，两个缺损均返回明确不确定并由来源未知状态阻断；实际返回别名仍不证明底层模型版本，价格保持未知。外部`tangmao-deepseek.json`本次恢复为严格扫描单请求配置，既有文本profile和历史另行保留；以后按具体入口选择对应配置，复用同一凭据和累计账本。
