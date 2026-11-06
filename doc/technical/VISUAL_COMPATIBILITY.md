# 视觉目标能力与运行时证据

目标 profile 是用户选定的渲染环境声明。它不能独立证明环境已验收，`verified` 或 `supported` 为真也不能放宽来源语义和资源检查。Rust 的精确允许表位于 `visual_capabilities.rs`；独立 Python 校验器另维护自己的精确允许表，不读取 Rust 判断或报告的通过标志。

公式与扫描先分别核对原始表达、识别表达、未知符号、前提和单位，再以共享 Obsidian / MathJax 精确组合及兼容策略选择目标表达。图像先核对本地附件、源语义、颜色与几何，流程先核对完整来源图、方向、关系符号、判定分支及标签；随后才按各自目标能力和配置选择表示。未测版本或引擎混配触发有据回退；关键语义未知继续阻断。

## 流程目标版本

旧 Schema1 的必填字段和序列化保持原状：`schema_version=1`、`producer=flow_render_profile`、`profile_id`、`app`、`app_version`、`mermaid_version:string`、`flowchart_support`、`label_encoding=decimal_ascii_punctuation_v1`、`html_labels`。Schema1 完全禁止 `renderer` 字段，传入 null 也拒绝。

新 Schema2 保留这些字段，但要求 `mermaid_version:null`，并必填 `renderer={name,version,source_sha256}`。这三个字段分别表示实际引擎名称、可知版本和执行脚本的原字节 SHA256；SHA 必须是64个小写十六进制字符。拒绝缺失、额外字段、null renderer、旧新字段混配及不支持的 Schema。有效的 Schema2 输入不自动获得原生能力，未知组合仍回退。

Obsidian 1.15.0 实际加载 Pele 的 Mermaid 兼容接口，执行脚本头声明 Pele 1.8.0。它没有可报告的 Mermaid 运行时版本，因此不能把上游声明的 Mermaid 语法版本写成实际引擎版本。Pele 的 SVG 文本标签使用 `html_labels=false`；旧 Mermaid 原生组合的该字段仍为 true。

## 原生验收

兼容矩阵在明确的新测试 Vault 与隔离配置中执行，分别保留最低支持版和执行时官方稳定版的浅色、深色结果。安装可执行文件、两个 ASAR、实际应用版本及主题、MathJax window 版本、执行脚本 SHA、源文件和附件字节独立核对。

数学树、负号、脚标、分式与关系运算符，流程的实际节点形状、标签、线端点与箭头三角形，以及图片自然尺寸、本地附件点击和同实例栅格证据均须通过。Properties、Callout 和标题/块链接的手写阅读探针另行标明，不能作为产品已实现对应生成能力的证据。

运行时探测、输入准备、截图存在、成功报告及 Git 提交都不能代替上述验收。原始失败、冻结包勘误和检查器适配必须保存；勘误不能改变来源、独立语义预期或消除真实不兼容。

已实际通过的 Windows x64 精确能力如下。对应原件为[B06V-T03矩阵](../validation/assets/B06V-T03/matrix.json)，16个冻结用例在两版明暗主题共64个唯一组合；手写阅读探针单独标记。完整回归与独立审查见[验收记录](../validation/B06V-T03.md)。

| 能力 | 最低产品支持版 | 执行时稳定版 |
| --- | --- | --- |
| 公式及扫描公式MathJax | Obsidian1.6.7 / MathJax3.2.2 | Obsidian1.15.0 / MathJax4.1.3 |
| 有界本地SVG | Obsidian1.6.7 | Obsidian1.15.0 |
| 有界流程 | Schema1 / Mermaid10.6.1 / html_labels=true | Schema2 / Pele1.8.0 / html_labels=false |

两种流程都要求固定producer、profile_id、supported及decimal_ascii_punctuation_v1编码。新profile_id为`obsidian_1_15_0_pele_flowchart`，renderer的name为`pele`，source_sha256必须精确等于`9bf34b3a9a19351dc69d0d238e365560331a8ae392c41656df7f5090653253d0`。数学及SVG的能力flags仍须满足各自契约；表中版本不代替这些条件。

未知版本、引擎或混配不纳入允许表。后续稳定版变化必须另行核官方原件并真实验收，不能把“current”解释为任意新版本。源语义、原响应、附件和报告即使同步改动也不能绕过独立作者锚；科学正确率、通用OCR、生产Vault和未运行平台仍按各自任务验收。
