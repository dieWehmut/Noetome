# B06V-T02 最终活动 API 新鲜只读审查

当前结论：发现两个重要边界问题，修复与反例 GREEN 尚未在本次审查验证，不能把本记录标为整包通过。root 已收到具体反例并安排修复；本审查未修改产品、脚本、测试或账本，也未提交、推送或读取真实凭据。实际范围与指纹见 `receipt.json`。

## 必须处理的发现

1. **SCAN-CASEFOLD-COLLISION：大小写等价附件引用未拒绝。** 原 `scan_input.rs` 使用精确字符串 `names.contains(file)`，共享 Held 精确库存与身份检查也不拒绝两个独立文件的大小写等价名称。独立 Python `selected_source` 同样只检查精确名称。在新的公开作者副本上，将整页改名 `page.PNG`、裁片改名 `page.png`，只更新相应引用；冻结作者与 expected 未改。固定 CLI 实际退出 3、创建 blocked Vault、报告来源与区域 passed，唯一问题为预算停止，而契约要求在读取输入阶段退出 2、不创建输出。独立来源读取也接受该副本。`casefold-red/receipt.json` 保存实际单命令、旧源码指纹和原输出；本次额外一次 CLI 为明确零请求配置，网络和模型均为 0。应在 Core 与独立读取器按规范化名称拒绝碰撞，包括 `case.json` 的等价引用，并验证普通大小写文件名仍可使用。

2. **SCAN-HELPER-NESTED-ECHO：辅助脚本漏检嵌套 Unicode 回显。** 原 `Hide-Credential` 的单层正则处理普通文本和一次 `\uXXXX`，但双反斜杠 JSON 转义后的公开测试 key 保持原 266 字节不变；脚本会将可恢复的内容保存为 stdout 副本，并标 `omission_reason=null`。实际 Windows PowerShell 5.1 与 7 各复现普通/单层通过、嵌套失败，probe exit1。原 helper、精确 probe 与双版本 stdout/stderr 在 `budget-subreview.echo-red.json` 和相关文件中保存。Core 对模型响应已有递归回显保护，本问题位于 helper 独立输出/公开原件的最后一道保护。应使用有界递归检测或发现任一转义层回显时整段省略，并覆盖混合转义、深度上界与允许的 ASCII 引号/反斜杠字符；深度耗尽必须保守处理。

## 已复核的其他边界

只读重读 `scan_api/input/formula/model/render`、共享 extraction 的两项可见性与扫描 owner/清理增量，以及实际 HTTP transport。请求生成仅接收配置与原裁片字节，固定 system 任务及单个 user 内联 PNG 不携带来源公式、expected、生成源码或建议答案。Core 对整页与区域解码成 RGBA 后逐行核对；严格 Schema、版本、nullable 与未知字段/重复键拒绝存在。模型 unknown、来源关键缺损、缺定义、未知语法和数学不一致都阻断；只定义单操作数脚标及原分式兼容规则，没有移除上横线或替换变量的宽松处理。

HTTP 的单次原响应、模型/finish/usage 与识别结果分开处理。有效 usage 在后续数学失败时仍保留；所有已开始的 transport 操作都通过 receipt 返回实际 attempts 与 received bytes，只有发起前的构建/守卫错误直接返回 Err。live 已发起但未验证 metadata 的模型/收费次数保持未知；人工数字环回明确为真实模型 0。原件、请求与成功对象有贯穿运行的身份/字节/祖先和库存检查，失败只清理可确认本次且未被人工修改的成功对象。

另独立重核 33 份作者原字节及三个 PNG 页/裁片关系，查看清晰整页与两个关键缺损裁片：来源为明确 CC0 自造工程页，负号和分母下标缺损区域确实遮住关键内容，不是科学文献分母。通过只读校验六份已有输出：原生重建、原图回退、无目标阻断、符号未知、下标未知及额外横线错误识别。实际笔记中的定义/单位邻接、全部前提/范围/例外、原识别文本、附件与 UTF-8 映射均核对；见 `saved-output-readback.json`。此抽查没有重新执行 CLI/HTTP，因此新增 transport/ordinary invocation 验证保持未运行。

预算子审查重新读取已有 g-run 的 20 次 synthetic helper invocation、80 份阶段收据、80 份账本快照和 200 份文件指纹。历史保留、保守总量 cap、预留、launch 原字节、用量结算及 UTF-8 原件一致；独立读取器使用明确原件与清单的外部 SHA anchor，不依赖手写的当前余额。除上述回显问题外，未发现该部分必修算术或绑定缺陷。`budget-subreview.originals-readback.json` 和 `budget-subreview.md` 记录本分工的实际范围。

文档精度建议：PNG 的 32 MiB 是每次 decoder 工作/输出缓冲的限制，不能解读为整个进程或同时持有两份 RGBA 时的 RSS 上限。二维轴与附件限制仍提供有界资源范围，本次没有性能/RSS 实测。

## 仍未验收与后续

本审查没有执行完整 Rust/Python 套件、真实模型 HTTP、当前 DPAPI 解密/账本核对、干净 Obsidian 或后续兼容矩阵；正在运行的套件归 root 管理，没有从旧版本结果推定新版本全部通过。两个发现关闭需要修后源码指纹、对应实际反例 GREEN 和必要回归。

当前实际服务设置、累计账本和凭据文件缺失，当前 accounted/pending/remaining 保持未知，历史 30,097 不能代替当前余额。官方图像协议只支持客户端接口依据，不证明糖猫权重版本或真实视觉路由。本项仍需真实清晰重建、关键未知、原响应及累计用量结算和实际 Obsidian 对照；所有 synthetic/零请求结果均不能替代这些门槛。知识为 pending、科学质量 not_run、未发布，不启动 B06V-T03。

## 回显发现的后续范围补充

在 helper 修复交叉讨论中发现：允许的 ASCII key 可以自身含字面 `\n` 或 `\uXXXX`。如果响应将 key 的其他部分编码得更深，全局迭代解码会先改变 key 的字面序列，后面才恢复其余字符，某一轮可能始终没有完整原 key。这也影响本轮复用的旧 Core `credential_echo`；前文“Core 已保护”只适用于最初的普通字母数字测试 key，不适用于全部允许的 ASCII 字符。

`probe_mixed_key_core.py` 用公开 fake key `noetome-test-\n-abcdef` 在明确数字环回人工服务中实际发起一次普通扫描 CLI。模型字段保存字面前缀和四层 Unicode 后缀；固定旧 CLI 实际退出 0、preview_ready、issues 为空，完整可解码原响应和 model 字段被保存。`mixed-key-core-red/receipt.json` 与 server/request/stdout/stderr 原件记录此例。本次新增一条人工 CLI/POST，真实 key、模型和费用仍为 0；冻结作者 33 文件未修改。

因此 SCAN-HELPER-NESTED-ECHO 的修复范围扩展到共享 Core 凭据回显检测，不能仅修 helper。应同时检测原 key 与同一有界解码规则下的归一化 key/变体，或使用保守失败策略；归一化耗尽、空值或极短目标不能让原文漏出。需要字面反斜杠/引号、混合深度和轮数上界的实际回归。此补充保留前一阶段观察与错误假设，不覆盖原 RED 或原审查收据。
