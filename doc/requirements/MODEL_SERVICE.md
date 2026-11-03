# 已授权的模型服务与复用规则

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

v5只明确已有单位/条件与atom.qualifiers的逐字对应规则，严格Core契约不变。最终499项Rust回归、API84/fake54/Chat35和128双报告/90请求核对通过；899项新鲜审查通过后第五次实际计量1283输入/2196输出/3479总Token。完整JSON、嵌套、引文及限定均正确，但三个概念和一个原子的revision为0，Core拒绝EXTRACT_KNOWLEDGE；原第五响应保留。当前累计23793、余额9976207、pending0。第五回包未出现reasoning_content，这个单次观察不构成开关普遍生效或内部模型版本证明。

用户顶部栏要求已在B04U验收、安装并推送；B04R继续v6所选Chat正整数revision Schema/提示修复，原严格验证及旧Responses保持。相关RED/GREEN、新鲜审查和一次预算内验证仍按真实结果完成。直接复用已保存服务、凭据和累计余额，不重新获得10M，不再次弹出密钥窗口。

真实原响应、使用记录和严格候选核对完成后，才按任务卡验收B04-T02并进入B04-T03。假服务、公开synthetic key测试和0请求预检都不得称作真实模型成功；返回别名也不是网关底层模型版本或收费的独立证明。
