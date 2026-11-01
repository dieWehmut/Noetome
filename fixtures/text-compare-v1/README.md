# 自造文本比较 fixture v1

本包全部文字为本项目自造并可公开的虚构材料，没有引用教材、用户资料或生产 Vault。甲容器、乙容器、5 cm、8 cm 和所有条件/例外仅用于验证数据结构、来源位置与显示映射，不能用于事实推断。所有 JSON 均明确声明 `producer=fixture`。

本包由执行者依据 [B02-T02 方案](../../doc/execution/B02-T02_PLAN.md) 手动安排来源块、知识原子和独立预期。机器进行的结构及来源片段审阅，不代表独立人工科学金标准、模型抽取质量或真实科学正确率。`case verify` 只读本包；实际 Markdown 渲染、Obsidian、Vault 发布与科学质量仍未运行。

## 内容和稳定身份

`case.json` 声明两份 UTF-8/LF 来源与三个输入 JSON；`ir.json` 含两个概念、四个必要原子、五条当前位置支持证据及十二条 Coverage；`note-plan.json` 含两份关联笔记、两处规范章节、两个段落、一个有序列表及一个比较表；`expected.json` 是作者独立声明的必要内容、全部审阅证据身份/范围/指纹、覆盖决策、规范位置及其解释原子、语义链接和逐格比较表，验证器不得改写。

| 类型 | 固定 ID | 含义 |
| --- | --- | --- |
| case / coverage scope | C7k / R8s | 自造比较包 / fixture 固定审计作用域 |
| 来源 / 版本 | S4m / V8r，S9q / V2t | 材料甲与材料乙 |
| 概念 | C2f，C6h | 甲容器与乙容器 |
| 定义原子 | A3d，A7e | 蓝砂收纳盒与红砂收纳盒的虚构定义 |
| 数值原子 | A5p，A9w | 虚构刻度 5 cm 与 8 cm，均含条件和例外 |
| 证据 | E3a，E5a，E3b，E5b，E7b | 来源定义/数值与第二位置的重复甲定义 |
| 笔记 / 章节 | N4u / H2j，N8v / H6n | `笔记/甲容器.md` 与 `笔记/乙容器.md` |
| 显示节点 | Q2z，T5x，Q6z，L9x | 甲段落、比较表、乙段落、乙有序列表 |

这些是不透明的短 ID；显示名称、路径和内容不得用于重新生成身份。四个必要原子均为 `known` / `fixture_reviewed`，定义无条件时 `qualifiers=[]`。数值的正文、条件、单位和例外在各自单元格或列表项内完整显示，不向其他单元格借字符串补足。

甲段落以概念 ID 链接乙的唯一规范位置，乙段落反向链接甲。比较表的列为比较属性/甲容器/乙容器；虚构刻度一行分别映射 A5p 与 A9w；单价一行的两格明确标记 `missing` / `未给出`。列标签、行标签、缺失状态、单元格种类/正文/原子映射由 expected 逐字段固定。

## 独立位置与字节复核

来源块由作者先确定为单行标题、空白、定义、空白、限定数值；材料乙再增加空白和完全相同的甲定义。起止为规范 UTF-8 字节半开区间，行号从 1 开始，ordinal 从 0 开始。没有调用待测 Rust 分段器产生这些期望。

| 来源 | Span ID | 字节区间 | 行 | kind | Coverage |
| --- | --- | --- | --- | --- | --- |
| 材料甲 | P1a | [0,24) | 1 | heading | excluded / fixture_review |
| 材料甲 | P2a | [24,25) | 2 | formatting | excluded / deterministic_format |
| 材料甲 | P3a | [25,65) | 3 | text | included / A3d |
| 材料甲 | P4a | [65,66) | 4 | formatting | excluded / deterministic_format |
| 材料甲 | P5a | [66,170) | 5 | text | included / A5p |
| 材料乙 | P1b | [0,24) | 1 | heading | excluded / fixture_review |
| 材料乙 | P2b | [24,25) | 2 | formatting | excluded / deterministic_format |
| 材料乙 | P3b | [25,65) | 3 | text | included / A7e |
| 材料乙 | P4b | [65,66) | 4 | formatting | excluded / deterministic_format |
| 材料乙 | P5b | [66,167) | 5 | text | included / A9w |
| 材料乙 | P6b | [167,168) | 6 | formatting | excluded / deterministic_format |
| 材料乙 | P7b | [168,208) | 7 | text | duplicate / P3a / A3d |

标题排除理由固定为“自造材料标题，仅标识来源。”；空白理由固定为“纯空白分隔，无知识内容。”。任意非空理由不能替代作者预期。P7b 与 P3a 全文 SHA 相同，P7b 仍有独立 E7b 支持证据；归并不能删除第二个出现位置。

证据范围为各实际 Span 内的 `[0,len)`，包括行末 LF。独立 expected 的 `evidence` 固定 E3a/P3a `[0,40)`、E5a/P5a `[0,104)`、E3b/P3b `[0,40)`、E5b/P5b `[0,101)`、E7b/P7b `[0,40)` 与各自真实 SHA；范围缩短到一个有合法 SHA 的字也不符合已审阅预期。两处 `canonical_placements.atom_ids` 分别固定甲定义 A3d 和乙定义 A7e，不能用同概念的其他数值代替移走的定义。

`audit_fixture.py` 使用固定字面区间和 Python hashlib 核对来源 SHA、各片段 UTF-8 边界、行号、完整覆盖、IR 与 independent expected 的全部五条证据、四个必要内容及两处规范解释原子声明；它只读文件，不调用 Rust，也不产生或接受新 expected。

```powershell
python fixtures/text-compare-v1/audit_fixture.py
./scripts/cargo_local.ps1 test -p noetome-cli --test fixture --locked --offline
```

固定来源和 IR 指纹：材料甲 `4f5c914504aaab35408dfcc88813b838b9b5ff99efd7023af60888e258425562`；材料乙 `ed8491e5b6935d46ba4708da6d969593f1c79c06739294bde7376435bbfa6535`；完整 IR `3dd1e99ef340d1f603c9514be9e5a3b2b2b2225135f0397d20bcffbe7577fc50`。补齐审查约束后的独立 expected 指纹为 `0477e6dc10ee3e5be654a3bbce4c22cb214972bf658a58e9f8a0cf16db39eadd`。

## 未交付 v1 契约的审查收紧记录

初始独立 expected 冻结指纹为 `f7b2d14cc91efcd1eb461920520c684b9bebc4ba61152783d53cb0bc802c9afd`。它没有独立锁证据的局部范围和规范位置的具体定义原子；首轮旧 CLI 已观察到未接线的真实红灯，接线后原有 15 组测试通过。随后代码审查和主执行者真实复现发现以下四类坏包仍错误退出 0：

1. E5a 缩到 P5a 首个字“仅”的 `[0,3)` 并配真实切片 SHA、刷新完整 IR 绑定，丢失数值/条件/例外证据。
2. 必要原子正文保留在 HTML `hidden`、HTML 注释或 Obsidian `%%` 隐藏注释中，仅凭字符串包含被算作完整显示。
3. 段落直接附加未登记的 `[[ghost]]`，没有对应的语义引用记录。
4. 乙定义 Q6z 搬入甲笔记，乙规范位置只剩数值列表，却仍被视为具有完整解释。

本次补充独立审阅的原始五条完整证据与两处定义原子约束，是收紧尚未交付的 Schema 1 草案。原始来源、IR 和原有必要内容/比较表都保持原样；没有把缩短、隐藏、断链或移走定义的坏输出采纳为新预期。验证器继续禁止重写 expected，全部恶意输入变异逐字保持更新后的 expected。

新增 `review_regression_` 四组真实 CLI 测试已在规则修正前成功编译并观察 **0 passed / 4 failed**；四类坏包都返回退出码 0，而失败断言要求 3。该记录证明审查缺口真实存在，不能用此前 15 组通过代替本次修复验收。明文显示规则覆盖段落、列表项、内容/缺失单元格、列/行标签、概念别名和语义显示别名；它拒绝手写 wiki/嵌入、原始 HTML/注释及未登记 Markdown 链接，不代表已经运行实际 Markdown 渲染器。

规则修正并补齐概念别名接线后，本机 Windows 实际运行 `./scripts/cargo_local.ps1 test -p noetome-cli --test fixture --locked --offline '--' --nocapture`，结果为 **19 passed / 0 failed / 0 ignored**，共 **135 次 CLI 调用**：原有 107 次和本次审查 28 次。新增回归包含三类 scope 各三种隐藏包装、八类显示字段/别名各两种 raw wiki/HTML 后缀、证据缩片、规范定义搬移以及普通数学明文 `x < 2` / `y > 1` 的正例。每次调用的输入字节、mtime、目录和包外哨兵均保持原样；真实 Windows 独占来源锁及根/来源 junction 测试仍通过。

随后独立复审又发现未规划的可选原子仍可带HTML正文或隐藏条件。新增实际CLI回归先观察到退出0的真红灯，再对所有Atom正文/qualifiers应用相同明文约束；可选、unknown或unverified状态不能绕开输入文本契约，正常数学明文仍保留。该回归另含两次语义拒绝与一次合法数学正例，共3次CLI调用；最终全量结果由[B02-T02验收记录](../../doc/validation/B02-T02.md)记录。独立expected未因这个修补改变。

`audit_fixture.py` 随后重新核对两份来源、12 个固定 Span、IR 和独立 expected 的五条完整证据、四个必要原子与两处规范解释原子声明并通过，输出新 expected SHA `0477e6dc10ee3e5be654a3bbce4c22cb214972bf658a58e9f8a0cf16db39eadd`。总工作区检查、独立审查和日志证据由主执行者保存；当前结果仍不代表实际渲染、Vault 发布或科学质量通过。

## 进程测试与限制

进程测试直接调用编译后的 `noetome case verify`。每个变异从固定包复制到新的自有测试目录；改变 IR 时仅刷新计划完整 IR SHA 来隔离语义规则，另有专门的不刷新 SHA 空白变异。所有变异都逐字保护原始 independent expected。测试前后核对文件字节、mtime、只读属性、目录清单和包外哨兵；Windows 测试真实打开独占来源锁、创建根目录/来源目录 junction 并核对失败保护。

命名变异覆盖缺原子、缺证据、仅反证、重复位置失去支持、Coverage 缺失/重复/自环/环/伪排除、必要状态漂移、旧 IR SHA、删内容/单位/条件/例外、空表、移格、标签/缺失状态漂移、规范位置与标题歧义、未知字段/版本/枚举、路径越界/大小写/ADS/保留名以及 UTF-8 范围和无效字节。expected 未根据失败结果自动更新。

有效测试首次已在旧 CLI 上编译成功并观察真实失败：`CLI_INVALID_ARGUMENT`、退出码 2，而成功断言要求 0。该红灯证明实际 case verify 接线缺失会被进程测试捕获；它不能代替最终绿色结果。完整验收记录由主执行者另存入 `doc/validation`。
