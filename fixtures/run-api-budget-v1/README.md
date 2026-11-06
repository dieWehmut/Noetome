# 持久 API 预算与有界重试作者包

本包在 B07-T02 产品行为实现前声明故障、策略和预算 expected。只用于新的测试项目和数字环回 `127.0.0.1` Responses 服务；凭据为公开人工值，真实模型及付费调用为 0，价格保持 `null`。不访问旧累计授权、生产账本、私人资料或生产 Vault。

`source/source.txt`、`source/gold.json`、`source/candidate-template.json` 分别逐字节复制旧 `extraction-gateway-v1` 的 source、expected、response，来源 SHA 见 `source-pins.json`。140 字节文字是原作者工程数据；两独立单元可以各选同一原始 span0，不代表两份真实科学页面。quote、数值、单位和条件来自旧作者 gold，不能从新输出推导。成功只表示严格候选绑定/结构通过，候选仍 pending、unverified、unpublished，科学质量 `not_run`。

`cases.json` 冻结 14 核心场景：双单元成功；429/503 后成功；次数耗尽隔离；已知用量的候选失败隔离；相同响应 SHA 的不同请求分别结算；Run 请求/Token 两近限；共享全局请求/Token 两近限；前次支出与非零人工 opening 不重置；未知远端完成的超时/断连/截断/真实 OS kill 变体；退避检查点 kill 恢复；耗时截止 kill 恢复；retry Token 到限隔离。四个 near-limit 场景需要两个实际并发 worker，首响应由服务端持有，胜者身份无预设。全局场景使用两个实际 Run 共享新 authority；顺序支出场景则先真实结算再新建 Run。额外 opening 和未知完成变体有各自先验字段，不能把它们的流量合并成虚构实际测试计数。

`policies/authority-template.json`、`authority-opening-template.json` 和 `run-template.json` 使用约定配置字段；Run 模板绝对路径/UUID是明确占位符，运行前只能按 `runtime-binding.json` 从普通 CLI 新建的选定项目取得真实 project/import/API-config。API 单请求模板只允许 local_http_test/openai_responses。`request-binding-windows.json` 列出六个 36 字节真实 UUID 替换窗口，其他 body 字段和原文不变，最终完整请求 UTF-8 字节数必须仍是 5238。

旧固定 Core05 ELF `e7194fa4…c8512` 在新测试根实际执行了 4 条普通 CLI：init、import、两个 `max_requests=0` API 预检。后两条保存原 request-body，实际 exit3/EXTRACT_BUDGET_STOP、network_attempts=0；原 stdout/stderr/回执及报告保存在 `originals/`。独立按原字节计算输入预留 `5238 + 1024 = 6262`，完整预留 `R = 6262 + 2048 = 8310`。近限 cap 固定 `R + 699 = 9009`；有效完整响应已知用量 700 后剩余 `8309 = R - 1`，不能再准入完整 R。该算式是旧单请求 body 与保守预留规划证据，不是新预算实现验收。

原 preflight receipt 的 `source_author_files_unchanged:true` 是脚本字面赋值，缺少当时独立作者/项目库存，不能作为历史全过程 guard 证明。本包只认可当前三份 raw copy 与旧作者 source pins 逐字节相等、原 4 条 CLI 和零 HTTP；新的冻结过程另在 `.artifacts/b07-t02-author-freeze-20261107/` 保存实际原作者及 copy 的 before/after SHA、len、身份、mtime 核对。最初 preflight wrapper 在 init 后尝试 JSON 解码普通文本失败，原 1 条 init 和错误原件仍留在 `.artifacts/b07-t02-author-preflight-20261107/`，不计为产品故障或删除。旧 preflight 原件保持原字节。

服务端使用固定响应模板。`fixed-200-invalid-candidate.bin` 的完整 wire 有可信 input100/output600/total700，但候选文本 `{}` 不合法；两个不同请求返回同一原字节 SHA，均必须各扣 700 且隔离失败，不能按 SHA 合并或放宽候选绑定。成功模板只把实际 request/source 身份绑定进旧作者候选，不增删作者内容。HTTP 429/503、超时、断连、截断缺可信 usage，原 attempt 完整 R 保留；只有五类允许普通有限重试，每次新 attempt/request_id、新预留，失败 wire/candidate/output/auth/source 等隔离而不重试。已知 provider 超量必须保存真实数值并停止新准入，不能裁成上限或写 0。

真实进程中断独立归为 `REQUEST_INTERRUPTED`，只在显式普通 resume、旧 worker 已实际失去发送能力且 Source/Owner/compiler/原件重验后允许继续。没有可信 final receipt 的旧 MaySend 仍 Unknown 并保留 R；新 attempt 仍受次数、retry Token、Run/global 和持久截止时间限制，绝不重发旧 key。Prepared 逻辑编号不倒退；仅从未 MaySend 的协议证据允许 NotSent 释放 token/request slot，记录仍保留。一次 attempt 重启/重复交付只结算一次；不同 attempt 同 SHA 仍分别计量。

退避默认 100/200ms 有界；kill 退避案独立用 base1000/max2000ms，实际服务 `response_before_write` 到后继 `request_received` 给出延迟下界，并核对持久 retry_not_before。耗时案 unit1500ms、实际等待1700ms，Run10000ms仍容许独立 B 单元。OS kill 必须是真实平台终止和 wait 回执；不能以故障 flag 或伪造状态替代。

`expected.json` 与 cases/policy 是先验判据，未从新产品 DTO/report/SQL 镜像生成。`frozen-sha256.json` 按每个原文件 SHA/字节数冻结本包，排除自身；冻结锚为该文件完整 SHA。新增实现、独立 HTTP 时序/预算/每提交窗口 kill 与完整回归尚未运行，此包不宣称这些门槛通过，也不证明 live 网关恢复、Retry-After、供应商幂等键、真实账单或科学正确率。
