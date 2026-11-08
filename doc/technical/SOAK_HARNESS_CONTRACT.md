# 长时运行与暂停闸门的合同

本文件冻结 soak harness 的口径：它重复执行一个只读工作单元，逐轮记录耗时、内存与
错误码，并用**可观测计数**证明「闸门关闭后不再调度」。对应需求 FR-35（暂停、继续、
崩溃恢复）、NFR-09（连续 24 小时基准运行）、NFR-13（暂停或退出 2 秒内停止调度新
请求）与 AC-39。

## 1 边界

- 只读：工作单元对一页合成文本跑 `crate::text::analyze`，不写项目、不写 Vault、
  不联网、不调用模型（`model_calls` 与 `network_requests` 恒为 0）。
- 合成负载：页面由参数确定生成，不含任何教材内容；`scientific_status = not_run`。
- **不代行 24 小时结论**：harness 按轮次运行，计划时长下限是
  `rounds × interval_ms`，与 NFR-09 的 86400000 毫秒是两件事。未跑满即
  `not_met`，任何短时结果都不能被写成达标。
- 真正的「应用暂停命令」还没有界面，所以 NFR-13 只标 `partially_measured`：
  闸门逻辑由计数证明，但界面与在途请求展示不在本轮范围内。

## 2 参数

| 参数 | 范围 | 说明 |
| --- | --- | --- |
| `--rounds` | 1 ..= 100000 | 总轮次数；每一轮是一个观测点 |
| `--interval-ms` | 0 ..= 1000 | 轮间隔；同时是计划时长的乘数 |
| `--pause-after-ms` | 0 ..= 600000 或省略 | 暂停时点；省略即全程不暂停 |
| `--max-rss-growth-bytes` | 0 ..= 1073741824 | 允许的 RSS 增长上限；超出即判无界增长 |

`--pause-after-ms` 用独立的 `pause_seen` 标记区分「没给」与「给了 0」：给了 0 表示
第一轮就关门，这与不暂停是两个不同的样例，报告与摘要必须能区分。

## 3 闸门语义

- 每轮开始时取一次 `elapsed = started.elapsed().as_millis()`。
- **闸门只在一处判断**：`paused = pause_after_ms.is_some_and(|after| elapsed >= after)`。
- 一旦为真就永远为真（不存在「一会儿开一会儿关」的抖动）；为真时该轮记
  `state = refused`，**跳过工作单元**。
- `effective_round` 与 `latency_millis = elapsed - after` 只在第一次关门时记录。
- 关门之后若仍有轮次被调度，`scheduled_after_pause` 自增——这是实现错误的直接证据，
  审计器对这个计数要求恒为 0。

这样设计的理由：暂停的可验证性在于「没有新请求被发出」，而这一点只能由计数证明。
如果只在日志里写「已暂停」，读者无法判断闸门是否真的拦住后面的轮次。

## 4 产物

| 文件 | 确定性 | 内容 |
| --- | --- | --- |
| `structure.json` | 只由参数决定，重复运行必须字节相同 | `spec`、`enforcement`（含 2000/86400000 两个规则常量与计划时长下限）、`coverage`、`reproduce`、零调用 |
| `manifest.json` | 同上 | 参数、`structure_sha256`、`structure_byte_length`、`coverage`；**不得带 `summary`、`pause` 或 `timings_sha256`** |
| `report.md` | 同上 | 参数、工作单元、闸门口径、覆盖范围、边界、复现命令；只允许出现规则常量与参数里的毫秒数 |
| `timings.json` | 本次运行的读数，明确不参与确定性比较 | `rounds[]`（逐轮 `state`/`elapsed_millis`/`work_millis`/`total_millis`/`rss_bytes`/`error_code`）、`summary`（13 项）、`pause`（7 项） |

清单里不能放 `summary` 与 `pause` 是踩过的坑：这两个结构装着毫秒与 RSS 读数，
放进清单会让同一参数两次运行的 `manifest.json` 不同，直接违反 §4 第一列的承诺。

## 5 覆盖率必须诚实

`structure.json.coverage` 恰好两条，顺序固定：

| requirement | status | 判据 |
| --- | --- | --- |
| NFR-09 | `not_met` | 计划时长下限 < 86400000 毫秒（本 harness 永远如此，除非真的跑满 24 小时） |
| NFR-13 | `partially_measured` | 闸门由计数证明；界面与在途请求展示未实现 |

状态是**由参数推导**的，不是写死的字符串：`detail` 里会写出本轮的计划时长下限。
把任何一条改成 `met` 都是把未验证的结论当成通过，集成测试与审计器都会判失败。

## 6 读数自洽

审计器与集成测试都要求：

- `scheduled + refused == rounds`，`units == scheduled`，`errors == 0`；
- 每轮 `total_millis >= work_millis`，轮次编号从 1 连续递增；
- `peak_rss_bytes >= max(first, last)`，`rss_growth_bytes == last - first`；
- 没有轮次被调度时，四个内存读数必须全部为 `null`（不可得就是不可得，不能补 0）；
- 有轮次被调度时四个读数必须齐全。

## 7 输出根

- 恰四个文件：`manifest.json`、`report.md`、`structure.json`、`timings.json`。
- 输出根必须尚不存在（`SOAK_ARGUMENT`），父目录必须存在（否则 `SOAK_IO`）。
- 每个文件用 `create_new` + `write_all` + `sync_all` 写，再读回逐字节比对。
- 写盘中途失败会删掉输出根，不留半成品。

## 8 命令行与错误码

| 动词 | 用法 |
| --- | --- |
| `soak run` | `--rounds N --interval-ms N [--pause-after-ms N] --max-rss-growth-bytes N --output <根>` |
| `soak --help` | 打印 `SOAK_HELP` |

| 错误码 | 退出码 | 触发 |
| --- | --- | --- |
| `SOAK_ARGUMENT` | 2 | 参数越界、数字不可解析、输出根已存在 |
| `SOAK_IO` | 3 | 父目录不可访问、写盘失败 |
| `SOAK_INTEGRITY` | 3 | 编码/回读不一致，或内存增长超过声明上限（无界增长） |
| `SOAK_WORK` | 3 | 工作单元失败；文案必须带轮次编号与复现命令 |
| `CLI_INVALID_ARGUMENT` | 2 | 动词或 flag 结构不对（缺少必填 flag、未知动词） |

`SOAK_WORK` 的文案里必须带复现命令，这是「崩溃时留下可复现命令」这条验收的直接
落点：失败信息本身就够重跑一次。

## 9 独立审计器

`scripts/verify_soak_harness.py` 从不 import 产品、不跑 CLI 判定。三种用法：

- `<输出根>`：布局、结构、计时、清单、报告；
- `--package <冻结包>`：声明、先验、执行序列、冻结清单；
- `<输出根> --package <冻结包>`：额外核对「这个输出根是不是用包里的声明跑出来的」。

它独立复算 `enforcement` 与 `coverage` 全文、逐轮闸门状态、`scheduled_after_pause`、
读数自洽与报告里出现的每个毫秒数。

## 10 报告与摘要的分工

摘要是给人看的：它必须写出计数、暂停窗口与 `NFR-09 not_met`。
报告是给复核看的：它必须写出参数、工作单元、闸门口径、覆盖率全文、边界与复现命令，
并且**不复制任何本次运行的读数**——读数只在 `timings.json` 里。

## 11 本轮明确不做

1. 不跑真实 24 小时（NFR-09 因此为 `not_met`）。
2. 不做暂停命令的界面（NFR-13 因此为 `partially_measured`）。
3. 不做崩溃恢复（FR-35 的恢复部分属于后续任务）。
4. 不做并发与多进程 soak。
5. 不测真实项目负载或真实 PDF 解析。
6. 不把耗时读数跨机器比较。
7. 不判断内容质量与科学正确性。
