# 明确选定公式的保真与回退 v1

B06-T03 的独立契约，验收状态由任务账本和实际验证记录维护。此入口处理明确选定的原始 TeX 或公式原区域，不调用模型、不扫描目录、不执行 OCR，也不恢复 PDF 作者源码。旧 fixture、比较表和过程流程契约不因新增公式入口而放宽。

## 入口与明确来源

```text
noetome knowledge formula <formula-case目录> <profile.json> <尚不存在的新测试Vault>
```

动词后可加 `--` 以接收连字符开头的路径。三个路径均为显式参数；只创建已有普通父目录下的新测试根，不写生产 Vault。该来源命名空间直接绑定选定文件及 SHA，不冒充已登记的项目 SourceVersion 或知识原子。

`case.json` 全部字段必填，拒绝重复键、未知字段和省略 nullable 字段：

| 字段 | 含义 |
| --- | --- |
| schema_version / purpose | `1` / `formula_fidelity` |
| selection_id / title | 明确选择身份与显示标题 |
| source_kind | `tex`、`formula_region` 或 `experimental_curve` |
| tex | `null` 或 `{file, sha256}`，仅明确原始 TeX 来源可提供 |
| semantics | `{file, sha256}`，选定来源的作者语义声明 |
| region | `null` 或 `{file, sha256, page, canvas, bounds, legibility}` |
| data_available | 来源是否具备原始实验数据；不因此提供本项未实现的曲线重绘 |

附件为 case 的普通直接子文件，清单精确等于 manifest 和明确引用的文件。禁止路径越界、文件复用、链接及硬链接；不遍历其他资料。case 最多保留一份 TeX、一份语义声明和一份已经选取的 PNG 原区域。

`semantics.json` 严格字段为 `schema_version=1, expression(string|null), symbols, conditions, ranges, exceptions`。每个 symbol 必填 `{symbol, definition(string|null), unit(string|null)}`。`expression` 是选定来源的作者手工语义转写，不是独立验收 expected，也不是从图像恢复的作者 TeX。数组为空表示来源未列出，不代表一般事实中不存在前提或例外。

原始 TeX 与该转写必须具有相同的受控数学 token 结构。所有出现的变量均需要明确符号定义，定义未知时阻断；单位、前提、范围和例外以原声明保存并进入必要正文。独立 expected 只由外部校验器读取，CLI 不读取预期答案生成笔记。

## 有界数学语义

当前 grammar 只覆盖有界标量表达、数字、明确变量、括号、正负运算、关系、分式及上下标。允许的命令集合和递归 grammar 以[冻结作者样例](../../fixtures/formula-fidelity/README.md)及实现为准；不据此宣称支持任意宏、矩阵、积分、化学式或推导。

语义比较保留符号、数字、上下标、分式参数、运算顺序及关系方向。忽略数学空白，并只对经检查的 `\dfrac` / `\frac` 使用相同分式语义；不能删除负号、交换下标或凭相似外观通过。解析还必须验证操作数、分式参数、脚标和括号结构，语法仅平衡大括号不足以成功。

未知命令、未确认符号或表达不一致是语义问题，不能用图片回退洗成成功。依赖原图的表达在原图不可辨时保留原附件并阻断；语义已核对的原始 TeX 可以使用原生表示，不依赖附带图片。无数据实验曲线作为拒绝虚假重绘的保护输入；本项不重绘曲线，也不交付一般图像知识提取。

## PNG 区域与目标能力

区域坐标为 `canvas=[W,H]` 上的 `bounds=[x,y,w,h]`，页号为明确来源页；矩形必须在画布内。附件已经是选定区域，PNG 实际尺寸必须等于 `w,h`，本项不执行裁剪。PNG 完整解码、CRC 和完整文件边界需验证，禁止用仅有合法首部的伪附件通过。资源受单文件、像素和解码内存限额约束；不接受执行脚本的 SVG、外部 URL 或动画。

profile 全部字段必填：`schema_version=1, app, app_version, mathjax_version, native_math, compatibility_dfrac, local_images, verified`。能力声明不等于实测记录；原生公式只使用实际验收的 Obsidian/MathJax 组合。其他目标须按未经验证处理，不能仅凭 `verified=true` 接受未知版本。

当前组合为 Obsidian 1.6.7 / 运行时 MathJax 3.2.2。辅助模块中的旧 3.2.1 字符串不能作为实际引擎版本证据。实机同时确认 `\dfrac` 可渲染；`compatibility_dfrac` 表示是否启用本编译器已核对的 `\dfrac` 到 `\frac` 规范化规则，关闭时采取原图回退，是编译器策略，不主张该引擎缺少命令支持。

只有语义可核对时才选择表示：原生 MathJax；明确等价改写后 MathJax；或保留原区域的本地图片回退。目标原生能力不足可以回退图片，原因需记录；仅原区域来源使用图片，不将手工转写输出为作者源码。图片也不可用时阻断。

## 产物与保护

新 Vault 保存 `.owner.json`、`source-case.json`、`source-semantics.json`、`profile.json`、选定原件 `original.tex` / `original.png` 和 `formula.json` / `formula.md` 双报告。原件保持逐字节相同及来源 SHA。原生成功另存 `compiled.tex`，兼容改写必须有明确记录，不能覆盖原件。成功计划为 `plan.json`，必要正文与表达进入 `公式预览.md`。

报告区分来源语义、兼容决策、保存及渲染检查，保存选择身份、source_kind、输入/附件指纹、原区域定位和逐块 UTF-8 来源映射。知识仍为 `pending`，科学质量为 `not_run`，`published=false`，网络与付费调用为零。语义阻断返回 3，保存问题及原件，没有成功 plan、compiled TeX 或笔记。

输入、profile、目录祖先和输出文件在整次运行中持有并收尾重验。碰撞或不可信路径不得覆盖旧内容。发生外部变更时，清理仅限仍为本次持有且字节未变的生成结果，保留原始附件和未知现场，不能留下未变的成功报告。

验收需包含冻结输入和普通 CLI RED、实际 CLI 新 Vault、独立 expected 与同步篡改矩阵、资源/祖先/碰撞/哨兵保护、完整 Rust/Python 和比较表/流程图回归，以及记录版本的干净 Obsidian 实际公式、原图和本地附件验证。作者样例、token 对照及渲染成功均不证明真实文献科学正确率、扫描重建或完整发布兼容矩阵。
