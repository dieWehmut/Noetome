# B03-T02 Python adapter 验证证据

本记录只描述实际 adapter 定向检查；Host/CLI、独立Poppler回看及全仓库检查由Root和作者分工负责。没有把这些尚未由本代理执行的检查报为通过。

- 请求行为：可运行stub先跑11测试真红；最终11测试、13次SystemRoot-only实际子进程调用全绿。
- 真实fixture：作者先冻结15PDF的SHA/页数/文字/独立坐标窗口；实际15类矩阵及5限额调用全绿，另有加密精确错误分类红绿。最终4测试、21次实际子进程调用全绿。
- 生命周期/字形：PDF.close重枚举与U+001C空白差异两项synthetic对象测试均先红后绿；它们没有替代真实PDF矩阵。
- 锁定：9个协议runtime版本字段、8包requirements闭包及27份完整license SHA/长度重新核对；无下载、安装、全局修改或pyc。

| fixture | 实际状态 | 已确认页 | 页状态 | glyph数 | region数 | 资源字节 | 单次墙时秒 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| N01 single-column.pdf | complete | 1 | parsed | 81 | 4 | 12954 | 0.230512 |
| N02 multi-page-single-column.pdf | complete | 3 | parsed,parsed,parsed | 273 | 12 | 43526 | 0.202872 |
| E01 scan-only.pdf | partial | 1 | needs_review | 0 | 0 | 433 | 0.202791 |
| E02 blank.pdf | partial | 1 | needs_review | 0 | 0 | 430 | 0.216269 |
| E03 damaged.pdf | failed | None |  | 0 | 0 | 0 | 0.245143 |
| E04 encrypted.pdf | failed | None |  | 0 | 0 | 0 | 0.162292 |
| C01 table.pdf | partial | 1 | needs_review | 78 | 5 | 11667 | 0.157454 |
| C02 formula.pdf | partial | 1 | needs_review | 55 | 4 | 9025 | 0.152511 |
| C03 image.pdf | partial | 1 | needs_review | 40 | 2 | 6610 | 0.148961 |
| C04 two-columns.pdf | partial | 1 | needs_review | 82 | 4 | 12645 | 0.154511 |
| P01 rotation-upright.pdf | complete | 4 | parsed,parsed,parsed,parsed | 143 | 8 | 23216 | 0.174867 |
| P02 crop-origin.pdf | complete | 2 | parsed,parsed | 62 | 4 | 10206 | 0.198521 |
| P03 rotation-vertical.pdf | partial | 1 | needs_review | 43 | 24 | 6866 | 0.194079 |
| P04 userunit-two.pdf | complete | 1 | parsed | 13 | 1 | 2363 | 0.195583 |
| F01 mixed-page-failure.pdf | partial | 3 | parsed,failed,parsed | 85 | 4 | 14021 | 0.241662 |

这些是当前小型fixture的单次进程墙时与真实资源字节，不是PDF吞吐/RSS或规模性能验收。扫描/空白/复杂页保留needs_review，损坏/加密无假页数；F01完整三页及两个正常页正文均保留。

机器证据见adapter-verification.json及adapter-{red,final-green}-processes.json、adapter-fixture-{first,encryption-red,final-green}.json、adapter-safety-{red,final-green}.json。没有改fixture expected来迁就解析器。
