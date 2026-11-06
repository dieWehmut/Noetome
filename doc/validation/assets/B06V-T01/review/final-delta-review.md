# B06V-T01 颜色问题最终增量复核

结论：先前fresh review的已知黑色填充导致关键SVG文字不可见问题已关闭。本轮只复核这一个问题的实现和验收增量；未发现新的可复现实质缺陷。旧fresh-review.md及contrast-probe/原始PNG、手工声明、CLI RED、receipt、源码指纹和旧SVG均逐文件核对未变，本文件单独追加最终GREEN，不改写历史。

## 变更核对

Rust 790e13f只在有限SVG的布局门槛中检查已声明图例色。RGB任一通道小于192即保守回退original_image，并说明IMAGE_COLOR_CONTRAST；所有通道至少192才继续原有布局核对。来源颜色、标签、单位均保留，合法knowncolor不被改成unknown。输入本来未知、原图不可辨或几何矛盾仍先阻断；曲线/观测与目标能力回退继续遵循原决策。

独立校验器00d6a36在classify与verify_svg都检查该范围，实际SVG校验不能只验证图元与同步指纹后一并接受不可读颜色。新增测试覆盖每通道191及192边界、knowncolor与unknown/曲线分别决策、正文及原件保留，以及实际SVG、来源声明、计划、双报告、映射与SHA同步篡改后仍因IMAGE_COLOR_CONTRAST拒绝。

本轮实际git show比对上述三个Rust文件及Python校验器/测试与对应commit逐字节一致，记录于final-delta/commit-bindings.json。标准已明确Schema仍接受任意#rrggbb，有限固定黑字仅支持保守亮色范围；此工程门槛不被表述为科学或自动识别正确率。

## 本轮实际验收

同一原件重新调用固定单link普通CLI：

```
/tmp/noetome-b06v-t01-root-final-20261106a/noetome knowledge image /home/lyinn/project/Noetome/doc/validation/assets/B06V-T01/review/contrast-probe/source /home/lyinn/project/Noetome/doc/validation/assets/B06V-T01/review/contrast-probe/profile.json /home/lyinn/project/Noetome/doc/validation/assets/B06V-T01/review/final-delta/new-Vault
```

- 调用前冻结本轮expected与保护指纹。CLI运行前后SHA均为3fe5ee193f9578e401c6a4602c7238f49d9923e68dea35d1637bdfc996428cbf，link count=1；未Cargo rebuild。
- 实际退出0，decision为original_image，fallback_reasons=[IMAGE_COLOR_CONTRAST]，issue_codes=[]；新Vault没有redrawn.svg，原PNG被本地嵌入并有原附件链接。
- 本轮从实际PNG/手工声明重新独立分类，完整verify_note通过标题、说明、元素标签/单位/图例身份、图例颜色/含义和所有源字段字节映射；黑色#000000、Payload、mg保持原声明。
- 三来源及profile副本逐字节一致；全部inputs、artifacts、plan和渲染文件SHA/长度对账，实际库存九件；CLI stdout和保存JSON一致，双报告payload和整段诚实可见报告壳一致。
- 原RED的黑字黑底SVG在新独立verify_svg下真实拒绝，具体原因actual SVG violates independent source layout/color safety: IMAGE_COLOR_CONTRAST。旧RED文件及源码保护指纹运行前后一致。

针对变更实跑两个已有必要测试：

```
/tmp/noetome-b05-t03-python/bin/python -m unittest test_verify_image.ImageAuthorTests.test_known_dark_colors_keep_original_without_reclassifying_labels test_verify_image.ImageCliReaderTests.test_actual_dark_svg_is_rejected_after_source_plan_and_reports_resynchronize
```

运行cwd=/home/lyinn/project/Noetome/tests，subprocess环境NOETOME_TEST_BINARY选择上述固定CLI。结果2 tests、0失败、0跳过；reader setup实际另调用一次普通CLI取得合法baseline，再执行同步dark SVG篡改拒绝。本轮显式samecase CLI与该测试baseline共2次CLI，不新增作者场景或扩大科学样本分母。Python解释器复用既有绝对路径，没有安装依赖。

## 证据和范围

final-delta/before.json保留调用前expected、源码/原件/旧历史保护pin；cli.receipt.json及stdout/stderr保留实际samecase运行和核对；delta-tests.receipt.json及stdout/stderr保留两个实际测试，commit-bindings.json联结代码版本；final-delta-pins.json登记本轮产物SHA。

本轮未编辑产品、脚本、测试、标准、旧审查或账本，未commit/push，不推进B06V-T02或T03。本次关闭的是颜色问题的fresh review门槛；没有重跑完整Rust/Python、平台保护或Obsidian。Windows完整同源回归与最终原生图像验收仍由父代理/指定执行者收束，不能从本次颜色GREEN直接宣布整任务complete。
