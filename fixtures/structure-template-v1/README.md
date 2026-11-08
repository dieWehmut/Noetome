# structure-template-v1

同一主题按阅读目标分成多份笔记、同类同层主题共用结构模板的冻结包。

## 成员

| 成员 | 用途 |
| --- | --- |
| `project.json` | 基线结构声明：1 个模板、2 个主题（同为 level 1）、6 个阅读目标、4 个概念、10 个原子 |
| `project-empty-section.json` | 变体：主目标声明「调控」章节但没有任何原子 → `TEMPLATE_EMPTY_SECTION` |
| `project-section-order.json` | 变体：主目标把「反应」写在「定位」前面 → `TEMPLATE_SECTION_ORDER` |
| `project-special.json` | 变体：主目标多声明模板外章节「补充」且没有 `special_reason` → `TEMPLATE_UNREASONED` |
| `project-title.json` | 变体：能量笔记标题偏离「主题：类型」命名且没有 `title_reason` → `TEMPLATE_UNREASONED` |
| `project-inconsistent.json` | 变体：糖异生总览的章节序列与糖酵解总览不同 → 缺口 `inconsistent_structure` |
| `project-duplicate.json` | 变体：糖酵解在两个目标里各有解释原子 → 缺口 `duplicate_explanation` |
| `expected.json` | 先验判据（计数、每个目标的路径/章节/原子/引用、唯一解释位置、偏差与缺口） |
| `cases.json` | 序列与变体清单 |
| `frozen-sha256.json` | 成员摘要清单（由审计器 `--freeze` 维护） |

## 先验怎么来的

1. 6 份笔记 = 2 个主题 × 3 个阅读目标；路径为 `代谢/<主题目录>/NN-<goal_id>.md`，`NN` 按主题内目标声明序从 01 起。
2. 8 个章节 = 各目标声明章节数之和 2+1+1+2+1+1。
3. 4 处主解释 = 4 个概念各恰有一个 `role=explanation` 原子；其余 5 个 `role=reference` 与 1 个 `role=embed` 在别的笔记里指向解释位置，不复制正文。
4. 10 处省略 = 每个目标相对模板三章未声明的章节数之和 (3-2)+(3-1)+(3-1)+(3-2)+(3-1)+(3-1)；省略是合法的，但必须作为偏差可见。
5. 同类一致 = `overview` 组的两个目标章节序列都是 `[定位, 反应]`，`energy` 组都是 `[反应]`，因此没有 `inconsistent_structure` 缺口。
6. 四个拒绝/缺口变体各只破坏一条规则：空章节、章节乱序、模板外缺理由（两条变体分别打 special_reason 与 title_reason）、同类形状不一致、重复解释。

## 冻结与复现

```bash
PYTHONUTF8=1 python3 scripts/verify_template_structure.py --package fixtures/structure-template-v1 --freeze
./target/debug/noetome template adopt fixtures/structure-template-v1/project.json /tmp/tpl/root
./target/debug/noetome template build /tmp/tpl/root /tmp/tpl/out
PYTHONUTF8=1 python3 scripts/verify_template_structure.py /tmp/tpl/root --output /tmp/tpl/out --package fixtures/structure-template-v1
```

## 边界

`scientific_status: not_run`。包内全部为自造声明，不含真实教材原文；本阶段只做结构规划，不生成知识正文、不写项目与 Vault、不调用模型、不联网，也不判断内容质量与科学正确性。
