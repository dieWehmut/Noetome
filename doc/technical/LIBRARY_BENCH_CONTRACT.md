# 大库规模实测合同（library-bench-v1）

对应需求：**NFR-11 大库管理**（3000 篇笔记、其中 200 篇人工修改，可扫描影响、分页展示差异、不因全量渲染冻结界面）
与 **NFR-12 交互响应**（本地常规操作 P95 在 2 秒内反馈，长操作立即给状态）。
相关验收：本轮只把可测量的部分标为 `partially_measured`，把测不了的部分**明确写成未验证**，不用百分比掩盖。

## 1 边界

- 语料是**合成的**：笔记正文、主题、概念、人工改动全部由参数生成，`scientific_status: not_run`。它不证明真实笔记库的语言质量，也不证明真实 PDF 抽取或真实人工编辑的表现。
- 只读：不写项目、不写 Vault、不联网、不调用模型。唯一写入的是 `--output` 指定的输出根。
- 本轮**没有界面**。NFR-11 的「不因全量渲染冻结界面」是本合同**没有**验证的部分，报告与 `structure.json` 的 `coverage` 必须把它写成缺口。
- 暂停语义（NFR-13）不在本轮：属 B13-T03。

## 2 参数

| 参数 | 范围 | 说明 |
| --- | --- | --- |
| `--notes` | 1 ..= 20000 | 合成笔记数 |
| `--edits` | 0 ..= min(notes, 10000) | 人工改动数 |
| `--samples` | 1 ..= 200 | 每个阶段保留的样本数 |
| `--warmup` | 0 ..= 50 | 丢弃的预热轮数 |

越界报 `BENCH_ARGUMENT`（exit 2）。解析失败（例如 `abc`）同样报 `BENCH_ARGUMENT`，文案形如「笔记数必须是非负整数：收到「abc」。」。

## 3 合成库的确定形状

- **扇出由笔记数推导**，不是写死常量：主题数 `max(notes/10, 1)`、概念数 `max(notes*2, 3)`。这样小夹具与大样本共用一条公式，`counts.topics/concepts` 才能被独立复算。
- 主题名 `topic-{index % topics:03}`、概念名 `concept-{index % concepts:04}`；每篇挂三个概念 `[i, i+7919, i+104729] % concepts`。
- 正文固定四段：`# 第 {i} 篇`、`## 定位`（两个概念的讨论）、`## 反应`、以及改动篇附加的 `## 人工改动`。
- **改动下标**：游标从 `3 % notes` 起、每步 `+7` 对笔记数取模、去重后取前 `edits` 个。因此 300/20 落在 3、10、17、…、136。
- 同一参数必得同一字节；不同参数必须改变 `corpus_sha256`。

## 4 四个阶段

| 阶段 | 做什么 | 为什么是这一段 |
| --- | --- | --- |
| `scan` | 逐篇跑 `crate::text::analyze`，取规范化摘要与片段数 | 大库更新的第一道成本就是全量文本分析 |
| `detect` | 与冻结基线摘要逐条比对，得出改动集合 | 纯比较，不做解析，单独计量才能真正归因 |
| `plan` | 汇总与改动笔记共享概念或主题的笔记 | 影响范围规划是「扫描影响」的实际产物 |
| `page` | 渲染变更清单第一页（表头 + 分隔行 + 至多 100 条） | 分页展示差异的最小可测单位 |

影响范围的口径：**共享主题或概念、且自身未改动**的笔记。改动笔记自己不进入影响范围——它已经被改动集合表达了。

## 5 测量协议

- 每个阶段重复 `samples + warmup` 轮；前 `warmup` 轮丢弃。
- **计时只包住阶段本身**：进程启动、参数解析、语料生成都不计入。`protocol.process_start_included` 固定 `false`，这条必须写进产物，否则读者无法判断数字的含义。
- 分位数用 **nearest_rank**：样本升序排列后取第 `ceil(P/100 × n)` 个（最小为第 1 个）。`p95`、`median`、`min`、`max` 全部由同一批样本直接算出。
- `median` 取 `sorted[len/2]`（偶数个取偏后一位），与 Rust 侧 `median()` 逐字一致。
- 判定：`p95 <= threshold_millis` 记 `met`，否则 `not_met`。门槛 2000 ms 是 **NFR-12 的常量**，不是读数。

## 6 两份产物：确定的一半与不确定的一半

| 文件 | 是否由参数唯一决定 | 说明 |
| --- | --- | --- |
| `structure.json` | 是 | 14 字段：policy / schema_version / case_id / spec / counts(9 项) / corpus_sha256 / baseline_sha256 / changed_notes / first_page_sha256 / protocol / threshold_millis / coverage / model_calls / network_requests |
| `manifest.json` | 是 | 9 字段，含 `structure_sha256` 与 `structure_byte_length`，**不含 `timings_sha256`** |
| `report.md` | 是 | 七节，**不写任何耗时读数** |
| `timings.json` | **否** | 8 字段：policy / case_id / protocol / stages(4) / threshold_millis / verdicts(4) / memory / note |

- 输出清单**刻意不摘要 `timings.json`**：一旦摘要它，"同一参数两次运行字节相同" 就永远不成立。
- `report.md` 唯一允许出现的毫秒数是常量门槛 `2000`。任何一次运行的读数写进去，同参数两次运行就不再相同——这条由审计器逐数字核对，不是靠措辞自觉。
- `timings.json` 的 `memory` 读 `/proc/self/status` 的 `VmRSS`/`VmHWM`，是**整个进程**的常驻集与峰值（含已建好的语料），不是单阶段分配量；平台读不到时 `peak_rss_bytes` 写 `null`。

## 7 覆盖率必须诚实

`structure.coverage` 是两个条目的数组，`requirement` 恰为 `NFR-11`、`NFR-12`，`status` 恒为 `partially_measured`：

- **NFR-11** 的 `detail` 按本次参数生成（写死「3000 篇」会在跑小样本时变成谎话）：它说明合成库确实建到了这个规模、四个阶段都真实跑过，然后指出「不因全量渲染冻结界面」属于 GUI 行为、本 harness 没有界面。
- **NFR-12** 的 `detail` 指出只测了四个阶段的 P95，项目页、暂停命令、搜索与打开问题的端到端反馈没有界面可测，暂停语义属于 B13-T03。

不许把 `partially_measured` 改成 `met`：没有界面就没有测到界面。

## 8 输出根

- 恰四个文件：`structure.json`、`timings.json`、`report.md`、`manifest.json`。多一个少一个都算失败。
- 输出根必须**尚不存在**；父目录必须存在且可访问。父目录缺失报 `BENCH_IO`（「无法访问输出根的父目录 …」），已存在报 `BENCH_ARGUMENT`。
- 写盘用 `create_new` + `sync_all` + 读回逐字节比对；任一文件写失败则删掉整个输出根，不留半个结果。

## 9 CLI 动词与错误码

```text
noetome bench measure --notes <N> --edits <N> --samples <N> --warmup <N> --output <输出根>
noetome bench --help
```

| 码 | 退出码 | 触发 |
| --- | --- | --- |
| `BENCH_ARGUMENT` | 2 | 参数越界、数字无法解析、输出根已存在 |
| `BENCH_IO` | 3 | 父目录不可访问、写盘失败 |
| `BENCH_INTEGRITY` | 3 | 检测到的改动数与声明不符、语料不是合法 UTF-8、写回读不一致 |
| `CLI_INVALID_ARGUMENT` | 2 | 缺少必填 flag、未知动词、多余参数 |

## 10 独立审计器

`scripts/verify_library_bench.py` 从不 import Rust 产品、也不运行 CLI 来判定任何东西：

```sh
python3 scripts/verify_library_bench.py <输出根> --package fixtures/library-bench-v1
python3 scripts/verify_library_bench.py --package fixtures/library-bench-v1
python3 scripts/verify_library_bench.py --freeze --package fixtures/library-bench-v1
```

它独立复刻生成器、片段模型、三个摘要、nearest_rank 与判定规则，核对：布局、字段集合、规范编码、摘要一致、改动清单、协议、覆盖率措辞、分位数自洽、输出清单不含时间戳、报告只含常量门槛、先验与实测一致、冻结成员未被篡改。任一项不符即退出码 1。

## 11 本轮明确不做

1. 不做千笔记/万原子的**规划**规模实测——`plan_model` 的 4/8/64/256 上限没有放宽，`scale` 动词已把这条如实标为未达标。
2. 不测界面：没有 GUI，也就没有「不因全量渲染冻结界面」的证据。
3. 不测真实 PDF 抽取与真实人工编辑：语料是合成的。
4. 不测并发、多进程或后台任务争用。
5. 不做 24 小时 soak（属 B13-T03）与暂停语义（NFR-13）。
6. 不把耗时读数写进确定性产物，也不做跨机器的耗时比较。
7. 不引入任何第三方基准框架：四阶段计时用 `std::time::Instant`，分位数自己算。
