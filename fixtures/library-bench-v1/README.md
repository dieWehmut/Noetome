# library-bench-v1 冻结夹具

300 篇笔记、20 处人工改动的**合成**笔记库。它用来核对四件事：阶段产物是不是由参数决定的、改动检测与影响范围算得对不对、变更清单分页对不对、P95 口径写没写清楚。

**它不证明真实笔记库的表现。** 笔记由参数生成，人工改动也是生成的；没有真实 PDF 抽取，没有真实人工编辑，也没有界面。「不因全量渲染冻结界面」这一条在 `NFR-11` 的覆盖率里明确标为没有验证。

## 成员

| 文件 | 作用 |
| --- | --- |
| `spec.json` | 参数声明：`notes=300`、`edits=20`、`samples=3`、`warmup=0`、门槛 2000 ms、边界声明 |
| `expected.json` | 实现前手写的先验：计数、三个摘要、改动清单、协议、覆盖率、确定性划分、7 条推导 |
| `cases.json` | 复现序列（两次同参数运行）与五个拒绝变体 |
| `frozen-sha256.json` | 成员摘要，由 `--freeze` 生成 |

## 先验怎么来的

先验是按生成规则手推的，不是从产品输出回填的：`scripts/verify_library_bench.py` 里有同一套规则的独立实现（Python），它与 Rust 侧互不引用。

1. **改动下标**：游标从 `3 % notes` 起、每步 `+7` 对笔记数取模、去重后取前 `edits` 个。300/20 时落在 3、10、17、…、136。
2. **扇出由笔记数推导**：主题数 `max(notes/10, 1) = 30`，概念数 `max(notes*2, 3) = 600`。写死常量会让小样本的参数与大样本脱节，也让 `counts` 无法被独立复算。
3. **每篇片段数**：未改动篇 10 个片段、改动篇 14 个片段（多出的「人工改动」小节 = 标题 + 空行 + 正文 + 空行，共 4 个片段）。300 篇里 20 篇改动，`280×10 + 20×14 = 3080`。
4. **改动检测**：按规范化摘要比对。改动篇的摘要与基线不同，恰好 20 处；没改的篇必须相同，这是「不误报」的判据。
5. **影响范围**：与改动笔记共享主题或概念、且自身未改动的笔记。300 篇里 190 篇。
6. **分页**：每页 100 条，20 处改动只有 1 页；第一页是表头 + 分隔行 + 20 条记录。
7. **三个摘要**：`corpus_sha256`（`path\0body\0` 顺序拼接）、`baseline_sha256`（每行摘要 + `\n`）、`first_page_sha256`（第一页正文的字节摘要），都是对拼接顺序与分隔符的直接复算。

## 冻结与复现

```sh
python3 scripts/verify_library_bench.py --freeze --package fixtures/library-bench-v1
python3 scripts/verify_library_bench.py --package fixtures/library-bench-v1
noetome bench measure --notes 300 --edits 20 --samples 3 --warmup 0 --output /tmp/bench-a
python3 scripts/verify_library_bench.py /tmp/bench-a --package fixtures/library-bench-v1
```

同一参数两次运行：`structure.json`、`manifest.json`、`report.md` 必须逐字节相同；`timings.json` 必然不同，它不参与确定性比较，输出清单里也没有它的摘要。

## 边界

- `scientific_status: not_run`：这是规模实测，不是科学正确性验证。
- `synthetic: true`：语料是合成的。
- 不联网、不调用模型、不写项目与 Vault。
