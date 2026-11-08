# scale-harness-v1（合成规模输入）

这是**合成语料**的规模实测夹具。页面由参数生成，不含任何真实教材原文，
也不证明真实教材的抽取质量、科学正确性或规划能力已经达到千笔记规模。

## 成员

| 文件 | 作用 |
| --- | --- |
| `spec.json` | 参数声明：页数 8、每页标题 2、每页段落 2；policy `scale-harness-v1` |
| `expected.json` | 实现前手写的先验：counts、逐页记录、上限、`nfr10_status = not_met`、确定性与边界声明、7 条推导 |
| `cases.json` | 用例表：两次同参数运行、五个拒绝用例、一个 help 用例 |
| `frozen-sha256.json` | 冻结清单，由 `--freeze` 生成；三个成员各自的摘要与字节数 |

## 先验怎么来的

1. 每页正文由「每页标题数」个标题块与「每页标题数 × 每页段落数」个段落块组成，
   标题块形如 `## i.h 概念 的第 h 节` 后跟一个空行，段落块是一行散文后跟一个空行。
2. 分段规则（`crates/noetome-cli/src/text.rs` 的 `segment`）把连续空行并成一个 formatting 片段，
   所以每页片段数 = 2×(1 标题 + 1 formatting) + 2×2×(1 段落 + 1 formatting) = 12，八页合计 96。
3. 字节数 4708 由生成器模板逐字拼接得到；模板改一个字，字节数与每页摘要都会变。
4. 概念出现次数按 8 个概念名在正文里的字面出现计数（标题与段落都会命中），合计 80。
5. 对齐警告为 0：每页标题都带编号，层级不跳级。
6. 上限四项直接取自规划动词的声明上限（topics=4、goals=8、concepts=64、atoms=256），
   `nfr10_status` 固定为 `not_met`，因为本轮没有做千笔记、万原子规划。
7. 确定性只覆盖 `structure.json`、`manifest.json` 与 `report.md`；`timings.json` 的毫秒数与峰值内存每次运行都不同。

## 复现

```bash
./target/debug/noetome scale measure --pages 8 --headings 2 --paragraphs 2 --output /tmp/scale-a
./target/debug/noetome scale measure --pages 8 --headings 2 --paragraphs 2 --output /tmp/scale-b
python3 scripts/verify_scale_harness.py /tmp/scale-a --package fixtures/scale-harness-v1
python3 scripts/verify_scale_harness.py --package fixtures/scale-harness-v1 --freeze
```

## 边界

- `scientific_status: not_run`：没有做任何科学正确性判断。
- `synthetic: true`：没有真实教材内容，也没有真实 PDF 抽取。
- 不联网、不调用模型、不写项目与 Vault。
- 千页规模只证明文本分析与章节对齐可用，不证明规划已经达标。
