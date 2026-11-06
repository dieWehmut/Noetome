# 有据过程与流程表达作者样例

本包是公开自造的卡片工作流与演示装置文字，不是教材、真实模型输出或科学基准；固定科学质量 not_run、知识 pending、published=false。作者在产品 parser 和流程 CLI 实现前声明原文、来源语义 gold 与独立表达 expected；不根据产品输出回填预期。作者身份为本轮独立分工 `/root/independent_audit`，仅编写本任务作者材料。

原文是单段多行中文，通过 [GRAMMAR.md](GRAMMAR.md) 的明确语法说明过程、步骤、判据和关系；没有 Mermaid、NotePlan、坐标或布局。每条步骤/关系的 label、description、输入、输出、前提、例外及真/假分支均是来源明示词面。作者用手工声明的语义记录进行机械排印和证据偏移计算，没有调用产品 parser、planner、renderer 或模型。Gold 供来源核对，不是被直接采纳的候选图；实际链必须先登记选定文字，由 SourceSession 重建原来源，再解析原文、完整核对 gold 并保存成功 IR。

`authors.json` 列出16个场景，各指向 `.txt`、`gold-*.json`、`expected-*.json` 和独立渲染 profile。Gold 的证据是完整声明行、不含行末 LF，使用 RequestUnit 内 UTF8 半开字节区间。Expected 只声明过程/节点/关系语义、状态、表示选择、先后约束及完整分母，不规定成图布局；`required_precedence` 只陈述有据先后或有向关系的展示约束，分支两个终点间没有先后断言，促进/抑制不表示被作用节点一定执行。

场景：linear、branch、causes、promotes、inhibits、correlation、missing_branch、cycle、direction_unknown、missing_connection、complex_labels、multiline_labels、compatibility_fallback、unknown_criterion、unparsed_tail、negated_relation。复杂标点案例故意把 s2/r2 的声明放在 s1/r1 之前，确保成图依据关系而非文本排列。causes 只保留“导致”，不得补正号；promotes 与 inhibits 分别保留促进/抑制作用。纯相关、方向未知、缺分支、未知判据、循环和缺关键连接均能准确核对原文，但不得得到成功流程表达。

全部列表字段必填，空数组仅表示选定来源未列出，不表示“无”。unknown_criterion 保留 null 判据以及来源明确的“未知”输出，不默认补判据。unparsed_tail 和 negated_relation 的 gold 仅声明可解析的完整语句；无法表达的尾句/否定语句完整留在原文和 expected.unparsed_lines，review 必须 needs_review、无成功 process IR，flow 不运行。否定关系绝不被改成 causes 或 unknown 肯定关系。

`multiline_labels` 的 JSON 词面明确含 \n/\t，解码后原标签包含实际 LF/TAB，图中不能保真时完整文字回退。`compatibility_fallback` 与 linear 的来源字节和语义 gold 相同，独立选用 unverified_flowchart profile，必须说明能力未核验而回退有序文字；该 profile 不声称 Obsidian 不支持 Mermaid。supported profile 指定 Obsidian 1.6.7 / Mermaid 10.6.1，仅在本任务实际探针和渲染证据对应时可据其选择图；本作者文件本身不是兼容验收。

`frozen-sha256.json` 签署本目录其他文件原字节。驱动必须先读回指纹并将作者输入、gold、expected、profile 冻结到全新测试目录，之后才调用 CLI。源码路径、SQL/资源、失败报告、正常/篡改表达和保护不变量仍须另行实际验收；本包不证明任意自然语言抽取、科学因果正确率或生产发布。
