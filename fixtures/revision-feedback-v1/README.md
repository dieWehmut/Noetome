# revision-feedback-v1：写作反馈缺口回到修订与依赖重编译作者包

本包是一份**独立编写、可公开的**小样本，用来验证「写作反馈发现知识缺口后回到修订流程、
修订生成新 IR 版本并重编译所有依赖表示」这一组工程契约是否真的成立。它不访问网络、
不调用任何模型、不写知识库、也不碰生产 Vault。

## 用途

对应需求 FR-14、FR-28、FR-35 与验收场景 AC-34。本包要回答四件事：

1. **缺口与研究候选是不同实体**：写作、编表、绘图时发现的知识缺口必须带来源与影响范围登记成缺口，
   回到修订流程；它不能退化成一条「以后再研究」的候选，也不能被自动当成已解决。
2. **依赖重编译**：修订经核验后生成新的 IR 版本，所有声明依赖该原子的表示都要重编译；
   未受影响的表示必须**字节完全不变**，不允许顺手重排。
3. **无新证据不循环推翻**：把值改回历史上出现过的旧值却没有带来任何新来源时必须拒绝；
   同一缺口反复修订要记录版本、原因与结果，超过可配置上限就升级为人工判断并停止自动修订。
4. **无来源补充不发布为结论**：没有来源支撑的补充允许写进 IR，但必须标为待核验，
   渲染里带 `【待核验】` 标记、状态列写 `pending_verification`，不得显示成已核验结论。

## 成员

| 成员 | 作用 |
| --- | --- |
| `author.json` | 作者清单：冻结语料声明、修订循环上限（`max_revisions = 2`）、5 个知识原子、3 种表示。 |
| `corpus-manifest.json` | 冻结语料清单：路径、SHA256、字节数。 |
| `corpus/*.md` | 3 个自造教材样例章节，带 `##` 小节标题，缺口来源用 `文件#锚点` 指向它们。 |
| `gaps/*.json` | 3 个缺口输入：缺单位口径、某行缺来源、定位写得过粗。 |
| `revisions/*.json` | 8 个修订输入：正常修订、改回旧值、带新来源的重入、空修订、无来源补充。 |
| `cases.json` | 冻结的操作序列与**先验预期**（IR 版本链、缺口状态迁移、重编译范围、拒绝序列）。 |
| `frozen-sha256.json` | 包内全部成员的字节指纹，由 `scripts/verify_revision_feedback.py --freeze` 生成。 |

## 先验预期怎么来的

这些数字是**先于实现**按契约推出来的，不是从产品输出回填的：

- **表示与依赖**：`rep-energy`（表格）引用 `a-glycolysis-net`、`a-glycolysis-substrate`、
  `a-gluconeo-bypass`、`a-pdc-nadh`；`rep-narrative`（正文段落）引用 `a-gluconeo-bypass`、
  `a-gluconeo-carboxylase`；`rep-trace`（追溯关系）引用四个原子。
- **IR v1**：`adopt` 冻结语料与作者清单后渲染初版并生成 IR v1；此时 4 个原子有来源（已核验），
  `a-pdc-nadh` 没有来源 → 初始核验计数 `verified = 4`、`pending_verification = 1`。
- **`rev-net-1`** 改 `a-glycolysis-net` 的值与单位：引用该原子的表示是 `rep-energy` 与 `rep-trace`，
  故 `recompiled = [rep-energy, rep-trace]`；`rep-narrative` 不引用它，`unchanged = [rep-narrative]`
  且字节必须与修订前完全相同。
- **`rev-net-2`** 第二次修订同一缺口：累计 2 次，恰好等于上限，仍允许。
- **`rev-net-3`** 把值改回 v2 已核验过的旧值且**没有新来源** → `REVISION_NO_NEW_EVIDENCE`（exit 3），
  不生成 IR 版本、不改任何字节。
- **`rev-net-4`** 带 `#质子平衡` 这一新来源重入同一缺口：允许，但累计 3 次已超出 `max_revisions = 2`，
  缺口随即升级为 `escalated` / `manual_judgment_required`。
- **`rev-net-5`** 在缺口已升级后再改 → `REVISION_LIMIT_ESCALATED`（exit 3），不再接受自动修订。
- **`rev-pdc-1`** 给 `a-pdc-nadh` 补上值与单位但**证据为空**：允许写入，结果按无来源补充处理 ——
  原子仍是待核验，缺口状态变 `pending_verification`，渲染里必须出现 `【待核验】`。
- **`rev-pdc-noop`** 的值、单位、证据与当前 IR 完全相同 → `REVISION_ARGUMENT`（exit 2），
  不生成新版本（不允许用空修订刷版本号）。
- **`rev-carboxylase-1`** 把定位从「线粒体」细化为「线粒体基质」：引用该原子的表示是
  `rep-narrative` 与 `rep-trace`，故这次 `unchanged = [rep-energy]`——与前几次恰好相反，
  用来证明「依赖范围」是逐原子算出来的，不是固定清单。
- **最终账本**：IR 版本链 v1…v6（只有 5 次成功修订生成新版本，3 次拒绝一次都没写盘）；
  缺口 `gap-net-account` 升级、`gap-pdc-nadh` 待核验、`gap-carboxylase-site` 已解决；
  修订共 5 条；核验计数仍是 `verified = 4` / `pending_verification = 1`。

## 冻结规则

- 本包成员一经冻结不再修改；要改行为就新建 `revision-feedback-v2`，不原地改。
- `frozen-sha256.json` 只能由脚本重新生成：`python3 scripts/verify_revision_feedback.py --package fixtures/revision-feedback-v1 --freeze`。
- 账本里的 `metadata.author` 与 `metadata.corpus` 必须与这里的 `author.json`、`corpus-manifest.json`
  逐字节规范等价，否则视为损坏。

## 边界

`scientific_status = not_run`。语料是自造教材样例，句子为了可确定性渲染而写得刻意规整；
它不证明真实教材上的知识正确率、不证明真实写作质量，也不证明 98% 那类内容指标。
产品在编译报告里仍如实显示研究为未启用，本包只覆盖「写作反馈 → 修订 → 依赖重编译」这一层工程契约。
