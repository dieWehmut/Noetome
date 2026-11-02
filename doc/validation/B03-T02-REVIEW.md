# B03-T02 独立审查及修复复核

审查子代理仅分工本项，未推进后续产品任务。源码/预期包在审查前冻结，长度及SHA与原件核对；全量执行由主代理另外完成。不可将“无新发现”解释为一般PDF正确性证明。

## 作者预期与视觉记录

初次14文件/19页审查指出两个重要缺口：非零旋转/review页没有独立字形位置oracle；语义篡改可能先被旧Markdown/摘要拒绝，从而不能证明目标PDF语义错误。已按原始authoring指标补全全部字符框及旋转CropBox/UserUnit预期，真实变异同时更新资源SHA、摘要和Markdown，并要求具体PDF语义错误族。review工具现在要求每个可渲染PNG实际存在且命令成功。

23页限定复核核对14份raw SHA、矩阵/原始操作符、全部PNG，缺口关闭。25页限定复核核对包333,479字节、SHA `1f2cc8ed5c0227c6837663bb74d8a3621e08543935acdccded1456b9fc13bebd`，当前expected SHA `020977dbb0eecb79799dbac3bb89bcec11db9443507a0d90b04ee4a0d8ffb352`。新增错开双栏原始四段共80字符，左右baseline分别728/698、716/686，错开12pt；字体top和独立框一致。纯空格页原始 `(   ) Tj` 三个空格，Helvetica12布局宽度均3.336pt、X边界72→75.336→78.672→82.008，与authoring框一致，但空格无可见墨迹。另12份raw及旧blank/columns归档保持实际SHA。

最终25渲染条目均退出0、图片存在，未发现需要修正的实质问题；[图片/命令记录](assets/B03-T02-authoring-review.json) 与 [当前预期](../../fixtures/pdf-text-v1/expected.json) 分开。此审查没有运行parser或全workspace，也没有断言科学、墨迹或UserUnit物理比例正确。

## 三个P2的实际复现与关闭

首次代码审查包517,917字节、SHA `4873aa24f5c41890ae2451f2752ae187b2a0968a5ea2be636eef6e1bcd1382e6`，base `5e96864e96d37299b840cfa5fc6ea068a4bd280e`、head `09f97a7d632e7dcd2c9e4c61a3787e523fc3bc03`，另附冻结未提交实现。审查用自造真实PDF独立复现三项P2，未发现Critical/P1。旧360 Rust/40 Python全绿不能排除这些问题。

| 原问题 | 实际修复、定向红绿 | 限定复审结论 |
| --- | --- | --- |
| 双栏错开baseline后行内检测遗漏，误parsed | adapter与Core对相邻规范行做保守水平间隔/高度候选检查；加入独立作者第二页、Core失败态和真实CLI红绿 | 关闭；原复现80glyph全部保留、PDF_POSSIBLE_COLUMNS、needs_review/exit3 |
| 仅三个空格被当成有正文，误parsed | 以非空白正文判断未知空白，实际字节/非空wire范围保留；直接adapter红绿与Core/CLI检查 | 关闭；保留`   \n`、PDF_UNKNOWN_BLANK、needs_review |
| ToUnicode把A+space映为一个glyph，adapter整glyph空白判断与Core首尾字符规则不同 | 双方只看前末/后首字符推断空格，多码点glyph必须review；6个CMap子场景先红后绿，真实CLI检查字面A B | 关闭；两原复现均`A B\n`、PDF_MULTICODEPOINT_GLYPH，无PDF_TEXT |

限定修复包377,689字节、SHA `96645c69eaf6bf43b36a25f7595b83070c68f9e9013fa1a074544b6b1c1fac25`。审查精确重放六份CHANGED差异，逐字节匹配冻结当前文件，核对8926-byte payload解压等于源码。复审另实际4次parse+4次inspect，page_count都为1、退出3、报告往返及资源SHA正确、原始/外部哨兵和inspect字节/mtime不变；[原始事实](assets/B03-T02-review-recheck-facts.json) 及4份原输入在assets归档。原三项P2均关闭，修复差异未发现新P1/P2。

## 最终测试握手修复

最终全回归暴露父测试在PID文件刚创建但尚未写完时读到空内容；与parser产品无关。通过create/flush后暂停0.2s令原路径写法稳定红灯，当前helper在pending文件保留同样暂停，写完关闭、flush诊断、rename才发布handshake.pid。[实际红绿](assets/B03-T02-handshake-green.log) 保留真实PID存活检查、取消/5秒超时、PID已退出以及诊断断言。产品源码、限额和断言没有放宽。

仅此修复审查包17,656字节、SHA `c69396f659d778cbb91842ac64f5446f455c2995339c78e5d741b19696774aaf`。复审核对包、嵌入文件字节以及六项生产源码/payload指纹未变，在自己的独立临时目录实际重放取消/超时两项：2通过、0失败、0忽略、5.04秒，见[独立事实](assets/B03-T02-review-handshake-facts.json)。握手竞态闭合、实际进程验证未削弱，未发现新P1/P2。完整workspace由主代理重新执行364项全绿，不能用此前失败的日志替代。

最终全回归和能力限制统一见 [B03-T02验收](B03-T02.md)。相邻行仅是保守候选、多码点字体bbox未独立验收、一般PDF内容完整性不能只靠字符清单证明，这些限制继续保留。
