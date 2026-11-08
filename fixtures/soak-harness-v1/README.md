# soak-harness-v1

冻结的 soak 声明与实现前手写的期望。这一份是**合成负载**：每轮的工作单元只是对
一页合成文本跑一次只读分析，不含任何教材内容，也不代表真实项目负载。它证明的是
「闸门关闭后不再调度」与「短时运行没有出现无界增长」，**不证明 NFR-09 达标**——
NFR-09 要求连续 24 小时基准运行，本 harness 按轮次运行，计划时长下限写在
`structure.json` 的 `enforcement.planned_floor_millis` 里。

## 成员

| 成员 | 说明 |
| --- | --- |
| `spec.json` | 冻结声明：`spec`（8 轮、轮间隔 0、不暂停、内存增长上限 1 GiB）与 `paused_spec`（同样 8 轮，但暂停时点为 0 毫秒）。 |
| `expected.json` | 实现前手写的先验：`enforcement`、两条 `coverage`、`no_pause` 与 `paused` 两组计数、五条拒绝、`determinism`、`boundaries` 与 `derivation`。 |
| `cases.json` | 执行序列（不暂停 / 再跑一次到另一输出根 / 暂停 / help）与五个拒绝变体。 |
| `frozen-sha256.json` | 冻结清单，由 `scripts/verify_soak_harness.py --freeze --package` 生成。 |

## 先验怎么来的

1. `enforcement.pause_bound_millis` 是 NFR-13 的时限常量 2000 毫秒，`required_millis`
   是 NFR-09 的 86400000 毫秒；两者都不是读数。
2. `planned_floor_millis = rounds × interval_ms`；两份样例的轮间隔都是 0，所以都是 0。
   它只是**计划时长下限**，不是本次运行的真实时长。
3. 不暂停：8 轮全部被调度，`scheduled 8 / refused 0 / units 8 / errors 0`，
   `pause.after_ms`、`effective_round`、`within_bound` 全是 `null`，
   `scheduled_after_pause` 是 0。
4. 暂停时点 0：第一轮开始时 `elapsed >= 0` 立刻成立，闸门在第 1 轮关闭，
   于是 `scheduled 0 / refused 8 / units 0 / errors 0`，`effective_round 1`，
   `within_bound true`（延迟 0 毫秒远小于 2000 毫秒），`scheduled_after_pause 0`。
5. 覆盖率必须诚实：NFR-09 恒为 `not_met`（没跑满 24 小时），NFR-13 恒为
   `partially_measured`（闸门逻辑由计数证明，但真正的暂停命令还没有界面）。
   把任何一条改成 `met` 都是把未验证的结论当成通过。
6. 确定性的可比较集合是 `structure.json`、`manifest.json`、`report.md`；
   `timings.json` 单列为不可比较，因为它装着本次运行的耗时与内存读数。
7. 输出清单只能记参数与规则结论：它**不能**带 `summary` 或 `pause`，
   那两个结构里有毫秒与 RSS 读数，放进来清单就会随机器快慢变化。
8. 五个拒绝变体各自只破坏一个约束：轮次 0、轮间隔 1001 毫秒、轮次写成 `abc`、
   复用已存在的输出根、漏掉 `--max-rss-growth-bytes`。

## 冻结与复现

```bash
python3 scripts/verify_soak_harness.py --freeze --package fixtures/soak-harness-v1
python3 scripts/verify_soak_harness.py --package fixtures/soak-harness-v1
./target/debug/noetome soak run --rounds 8 --interval-ms 0 --max-rss-growth-bytes 1073741824 --output /tmp/soak-a
python3 scripts/verify_soak_harness.py /tmp/soak-a --package fixtures/soak-harness-v1
```

## 边界

- `scientific_status: not_run`、`synthetic: true`：合成负载，与教材内容无关。
- 不写项目、不写 Vault、不联网、不调用模型（`model_calls` 与 `network_requests` 恒为 0）。
- 短时运行只能说明「这段窗口里没有出现无界增长」，不能外推成 24 小时结论。
