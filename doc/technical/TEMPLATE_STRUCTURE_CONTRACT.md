# 主题阅读目标与结构模板合同

对应需求：FR-15（教材式自适应规划）、FR-16（同类结构一致与模板是结构规范）、AC-17（同一概念仅一处完整解释）。本文件冻结 `template adopt|build|list|status` 的可观察行为，供独立审计器复算。

## 1. 边界

- 只读：读一份结构声明（JSON），产出规划与报告，不改写任何来源文件。
- 不联网、不调用模型：报告里 `model_calls` 与 `network_requests` 恒为 0。
- 不写项目与 Vault：冻结根只有 `control/`，输出根只有派生文件。
- 不判断内容质量与科学正确性：本阶段只回答"哪个目标放哪些原子、章节怎么排、概念在哪里完整解释"。
- 规划有缺口时仍然写出完整报告，只把退出码置为非零——缺口必须逐条可见，而不是被藏进拒绝里。

## 2. 结构声明

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `policy` | 字符串 | 固定 `structure-template-v1` |
| `schema_version` | 整数 | 固定 `1` |
| `producer` | 字符串 | 固定 `fixture_author`（样例声明不得冒充真实作者） |
| `case_id` | 标识符 | 本次规划的编号 |
| `description` | 字符串 | 一句话说明这份声明在验证什么 |
| `folder_root` | 相对路径 | 规划输出的顶层目录，例如 `代谢` |
| `templates` | 数组 | 1..=4 个模板，每个含 `template_id`/`title`/`sections`/`navigation_depth` |
| `topics` | 数组 | 1..=4 个主题，每个含 `topic_id`/`title`/`folder`/`template_id`/`level`/`goals` |
| `concepts` | 数组 | 1..=64 个概念，`concept_id` + `canonical` |
| `atoms` | 数组 | 1..=256 个必要原子，`atom_id`/`concept_id`/`goal_id`/`section`/`role` |
| `limits` | 对象 | `max_templates`/`max_topics`/`max_goals`/`max_concepts`/`max_atoms` |

阅读目标的 `kind` ∈ `overview`/`reaction`/`regulation`/`energy`/`comparison`，中文标签依次是总览、反应、调控、能量、比较。原子的 `role` ∈ `explanation`（完整解释）/`reference`（引用）/`embed`（同源嵌入）。

## 3. 冻结根布局

顶层只允许 `control/`；`control/` 恰好三项：

```
control/inputs.json     # 规范编码、无尾换行
control/manifest.json   # 规范编码、无尾换行
control/project.json    # 结构声明的原始字节（保留缩进与尾换行）
```

`manifest.json` 13 个字段：`policy`/`producer`(`noetome-template`)/`schema_version`/`case_id`/`root`/`root_identity`/`parent_identity`/`control_identity`/`manifest_identity`/`inputs_identity`/`project_sha256`/`inputs_sha256`/`created_at`。

`adopt` 先把规则预演一遍（空章节、章节乱序、模板外缺理由都在建根前报出），全部通过才建根；建根过程中失败且根身份未变则删掉刚建的目录，不留半成品。

## 4. 模板是结构规范，不是填空

- 不适用的章节**可以省略**，省略逐条记 `section_omitted` 偏差，且 subject 写成 `goal_id#section`（省略合法但必须可见）。
- 声明了章节却没有原子 → `TEMPLATE_EMPTY_SECTION`（"不适用的章节应当省略，而不是留一个空标题凑齐模板"）。
- 声明模板里没有的章节 → 必须给 `special_reason`，否则 `TEMPLATE_UNREASONED`；给了记 `special_section` 偏差。
- 目标的章节相对顺序必须与模板一致（省略可以，换序不行）→ 否则 `TEMPLATE_SECTION_ORDER`。

## 5. 命名与导航深度

- 标题必须等于 `"{主题标题}：{kind 中文标签}"`；不等必须给 `title_reason`，否则 `TEMPLATE_UNREASONED`，给了记 `title_deviation`。
- `navigation_depth` 必须等于模板的 `navigation_depth`；不等必须给 `depth_reason`，否则 `TEMPLATE_UNREASONED`，给了记 `depth_deviation`。

## 6. 同类同层一致性

一致性判据：按 `(template_id, level, kind)` 分组比较各目标"存活章节"的序列。

- 序列不同但各目标都有 `special_reason` → 记偏差，不阻断。
- 序列不同且缺理由 → 记 `inconsistent_structure` 缺口（不抛错，进 findings）。

这样"差异必须声明理由"是可复算的：审计器按同一分组规则重算，不依赖任何百分比。

## 7. 唯一解释位置

- 每个概念 `role == explanation` 的原子数必须恰为 1：0 → `concept_unexplained`，>1 → `duplicate_explanation`（列出每一处笔记路径）。
- 恰有 1 处时，其余目标的同概念原子只能写 `reference` 或 `embed`，并在 `references` 里指向**解释所在的那份笔记**。
- 解释笔记内部出现同一概念的 `reference`/`embed` → `TEMPLATE_ARGUMENT`（"同一份笔记里引用自己会把链接变成噪音"）。

## 8. 输出与字节确定性

输出根恰好五个文件：`plan.json`、`report.json`、`report.md`、`manifest.json`（无 `created_at`，多一个 `verdict`）、以及输出清单自身。同一冻结输入的两次构建必须逐字节相同；`report.md` 六节：规划规模 / 阅读目标与结构 / 唯一解释位置 / 偏差与理由 / 缺口 / 边界。

## 9. 动词与错误码

| 动词 | 作用 |
| --- | --- |
| `template adopt <声明> <结构根>` | 冻结声明与输入摘要，预演规则后建根 |
| `template build <结构根> <输出根>` | 重新规划并写出派生文件与报告 |
| `template list <结构根>` | 机器可读的规划摘要 |
| `template status <结构根>` | 人类可读的重算结论 |

| 错误码 | 退出码 | 触发 |
| --- | --- | --- |
| `TEMPLATE_ARGUMENT` | 2 | 参数或声明字段非法（含解释笔记内自指） |
| `TEMPLATE_INTEGRITY` | 3 | 冻结根被改动或布局不符 |
| `TEMPLATE_UNKNOWN_ROOT` | 3 | 陌生目录，保持只读 |
| `TEMPLATE_SECTION_ORDER` | 3 | 章节顺序与模板不一致 |
| `TEMPLATE_EMPTY_SECTION` | 3 | 声明章节没有原子 |
| `TEMPLATE_UNREASONED` | 3 | 特殊结构、标题或深度偏离缺理由 |
| `TEMPLATE_INCONSISTENT` | 3 | 同类同层结构不一致（build 后判定） |
| `TEMPLATE_UNEXPLAINED` | 3 | 有概念没有完整解释（build 后判定） |
| `TEMPLATE_DUPLICATE_EXPLANATION` | 3 | 同一概念多处完整解释（build 后判定） |

## 10. 报告单列什么

`report.json` 的 `counts` 16 项覆盖模板、主题、目标、笔记、概念、原子、解释、引用、嵌入、章节、特殊章节、省略章节、偏差、缺口、模型调用、网络请求；`deviations` 与 `findings` 各自是数组，逐条含 `kind`/`subject`/`detail`。报告不把"规划成功"当成"内容正确"：结论行只声明结构与位置。

## 11. 本轮明确不做

1. 不生成正文，不写 Vault。
2. 不做语义相似度判断（章节、概念、标题都按字面规则比对）。
3. 不支持跨主题搬移原子（跨主题只能建立链接）。
4. 不处理用户锁定位置（留给下一张卡）。
5. 不做增量重规划与影响面传播（留给下一张卡）。
6. 不支持 PDF/DOCX 直入。
7. 不对真实教材内容作科学正确性判断。
