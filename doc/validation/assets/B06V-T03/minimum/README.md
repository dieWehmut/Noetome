# B06V-T03 最低支持 Obsidian 1.6.7 原生矩阵

本目录保存实际 Windows Obsidian **1.6.7 / MathJax 3.2.2 / Mermaid 10.6.1** 的最低支持版验收原件。最终选择为 **15 个产品用例和 1 个明确手写阅读格式探针，各自浅色、深色，共32个完整通过组合**。它不记录当前稳定版的验收结果，也不扩大科学质量、通用OCR或生产Vault发布声明。

## 完整性和选择规则

[最终32组合索引](final-index.json)逐项联结原生 `result.json`、准备记录、实际命令退出码、运行时输入、安装及CDP执行脚本指纹、源文件、附件、清理和归档成员。[先前完整32组合审计](audit-result.json)保留为历史：当时32份采集脚本与最低版冻结工具 `b7c97beceb536a437f5fa76b542b6299b47feb93d715d1a1430b60c3b935409f` 派生字节完全一致；table/flow/formula/scan旧派生字节相同，因此保留已通过捕获，格式与图像的6个受影响组合另行用该冻结工具实际重测。

最后补充H轮格式浅深2个组合，使用 `9c28fd119634c14d2c28d24451799175fe3fa9aceaeae10566f000e52767c83f`，保存采集器此前已实际断言、但未序列化的原 `identity`。新旧FORMAT严格只多一行原值保存；UI、语义、链接及身份断言不变。[格式补充原件与索引](format-identity-override.json)保留实际profile、Vault及空communityPlugins，未填推断路径。其余30组合继续复用原执行脚本与成功原件。

最终9c28源码的额外变化已由上文H轮单行identity补充核对；下述b22范围证明使用归档的两版旧工具原件复现，其脚本若直接指向当前9c28源码会按指纹约束拒绝。

主采集器此前增加当前稳定版SVG的实际标签墨迹验证，当时冻结源码为 `b22fc9c0f91812fac99dd0087291c67cc98249f00f4da068271bdd2c0f551a49`。[严格源码范围核对](final-collector-scope.json)及[可重复脚本/两版原件](final-collector-scope-originals/verify_scope.py)证明所有非image派生和共享运行时/格式/流程helper逐字节相同；旧1.6.7 native分支逐字节相同。新增墨迹只在 `native.pixelMethod && semantic.source` 下执行，已捕获最低版native没有pixelMethod、实际geometry元素没有actualInk，因此标签仍检查原bbox。只移除新增当前墨迹helper/分支、将fallback bbox改写恢复后，整个image采集器严格等于原b7派生字节。这里明确复用已实测最低路径，不声称整份b22 image脚本曾在最低版执行，不重写旧成功/失败归档。

最终记录选择：原C轮22个未改变的产品组合；纠正包F轮扫描符号/下标4个组合；G轮图像4个组合；H轮格式2个组合。D/G轮旧格式及C轮旧图像的原成功记录仍保留，不重复计入最终矩阵。共有44次真实原生应用捕获，40次成功、4次失败；另有E轮一次准备目录缺失失败，没有启动应用。每次均用新的独立Vault/profile/owned PID/CDP端口。

最终32组合实际核对18次本地原PNG/扫描页附件点击、共94次附件及内部定位点击。所有选定组合的原始输出、Vault副本及capture-spec保持原字节，owned PID与端口自然退出、profile进程数为0，未使用紧急终止。所有使用MathJax的6个组合读取实际 `window.MathJax.version=3.2.2`；8个实际原生流程组合绑定Mermaid执行源原字节。未要求引擎的组合明确记录 `not_loaded_or_not_required`。

Mermaid未暴露全局 `.version`，不伪造为已暴露值。10.6.1来自实际安装库原头部 `// Source: https://unpkg.com/mermaid@10.6.1/dist/mermaid.min.js`，准备时独立读取此版本锚，并由实际CDP执行脚本SHA绑定完整安装成员。执行源只允许原成员字节及精确显式sourceURL后缀；独立审计从已认证的原成员重建完整执行SHA。

## 最终组合

下表每行浅色和深色均有完整成功原件。实际成员路径及result SHA以 `final-index.json.records` 为准；其中原30行指向 `native-originals.zip`，H轮格式两行指向 `format-identity-originals.zip`。

| 用例 | 范围 | 最终原件轮次 |
| --- | --- | --- |
| table-escaping-names | 中文路径、方括号/竖线/星号/别名及两个实际详情链接 | C |
| flow-branch | 判定菱形、真假条件、节点/边方向及实际详情链接 | C |
| flow-multiline-labels | 保留LF与Tab的有据文字回退及链接 | C |
| flow-complex-labels | 复杂标点的原生流程标签及方向 | C |
| flow-promotes | 正号促进标签、原生箭头方向与目标联结 | C |
| flow-inhibits | 负号抑制标签、原生箭头方向与目标联结 | C |
| formula-native | 原TeX、负号、分式/脚标数学树及原PNG实际点击 | C |
| formula-native-relational | 关系运算符、分式/上下标数学树及原PNG实际点击 | C |
| formula-image-fallback | 原区域PNG回退、符号/条件及本地图片视图点击 | C |
| image-signed-arrows | 真实SVG原生解码、原白底、正负箭头/单位/图例与原PNG点击 | G |
| image-curve-no-data | 无原始测量数据的原PNG回退、单位/限制及原PNG点击 | G |
| scan-clear | 既有选定扫描重建产物的MathJax树、原表达、原区域与完整页点击 | C |
| scan-fallback | 原区域PNG回退、原表达及原区域/完整页点击 | C |
| scan-sign | 关键符号不明保持needs_review/blocked，无成功计划或预览 | F，独立纠正包v2 |
| scan-subscript | 关键下标不明保持needs_review/blocked，无成功计划或预览 | F，独立纠正包v2 |
| hand-authored-format-probe | 显式手写：Properties实际文字/布尔/别名chips、Callout及标题/块实际定位，保存原identity | H |

格式探针保留YAML字符串、布尔及数组原语义；仅把作者的list概念与旧Obsidian实际 `multitext` 控件枚举显式对照。最终原件读取可见字段名称、文本编辑值、unchecked复选框与完整两个别名chips，并验证可见几何。块链接检查原段落与邻接块ID、真实metadata cache块ID/原start.line、clicked data-href及点击后的唯一完全可见目标；仅在实际P元素末端BR后有纯LF时移除一个比较用的末尾LF，原文字和DOM均保存。

人工实看了最低版的流程促进/抑制、深色比较表、关系公式、正负SVG、清晰扫描公式与原区回退、符号/下标阻断报告及浅深格式截图。实际截图可见扫描负号/分式/下标，回退原图保留相同表达，Properties复选框未选中且两个别名完整。截图只补充DOM/数学树/实际点击等门禁，不能单独替代它们。

## 原件归档

| 原件 | 完整SHA256 | 字节 / 成员 |
| --- | --- | --- |
| [全部原生与准备原件](native-originals.zip) | `911d0e8fa33d8b086bfc7b9f0a273f421423b16f7c3fb20c17ae2d8b2d3e5d3c` | 21,125,377 / 3,187 |
| [冻结源码、作者包与安装成员原字节](source-originals.zip) | `e587c64844be74d9cb5c498093092d056c61c6daaf9822dc9a52c97cdaec19c9` | 20,035,882 / 6,392 |
| [先前已单独封存的失败原件](early-failures-originals.zip) | `e38493f1dadd505142e946c775baaaa64fcb33dd0b9e3a78da54834096c36ec6` | 879,221 / 168 |
| [最终格式原身份补充原件](format-identity-originals.zip) | `5077573e580f1bc6fbf3942aa969ff4df86174e0c97e632ecf887d815b9634d1` | 4,665,440 / 1,276 |

对应manifest列出每个成员原路径、长度、SHA。native、source与新format补充 ZIP所有成员完整读取、CRC及SHA核对通过；原6,364份声明冻结源文件和H轮1,163份声明源文件重读匹配。native包含各轮原始JSON、命令/stdout/stderr、实际DOM/PNG、准备脚本、复制Vault原件与 `.obsidian` 配置，保留profile三份顶层配置，只省略新测试profile的浏览器缓存和Python字节码。source包含所有实际冻结工具/作者输入/旧采集器，以及安装ASAR中MathJax、Mermaid、app.js的精确原字节，记录并独立核对其安装ASAR锚。format补充独立保存H轮新原生捕获和完整冻结源码，不覆盖前述归档。

最低运行时原记录见[准备记录](../runtime/minimum-runtime-prepared.json)与[新观察记录](../runtime/minimum-runtime-observed.json)。旧prepared原件没有修改，observed记录SHA为 `88e00bc3481311959e6e13f0fdaf57cf16f13cae937058d541f1eb4a46dabfa5`。既有 `C:\Program Files\Obsidian` 仅运行明确launcher并每次用新profile；未覆盖安装、未启动更新、未注册协议、未写日常profile或生产Vault。所有捕获实际重读可执行文件和两个ASAR，与明确原件指纹一致。

## 保留的失败与有据修正

1. A轮格式浅色：Node继承代理环境，CDP环回连接502；未进入产品语义检查。后来只对捕获子进程移除HTTP代理并显式环回no_proxy，未改全局环境。原失败清理确认owned进程，保留紧急终止事实。
2. B轮格式浅色：Obsidian实际list UI枚举为 `multitext`；独立作者YAML数组未改，采集器增加明确语义到原生枚举映射。
3. C轮格式浅色：真实块定位段落有移除块ID后的末端BR/LF，不能用泛化trim。保留首个标题点击成功、第二个块目标等待失败及原DOM；之后加入严格缓存/原行号/唯一可见块验证。
4. C轮scan-sign浅色：原验收required_texts误写单数 `SCAN_SEMANTIC_UNKNOWN`，产品原件和契约为 `SCAN_SEMANTICS_UNKNOWN`。独立[纠正包v2](../../../../../fixtures/visual-compatibility-corrected-v2/README.md)仅纠正两案诊断码拼写；原13包、源/识别/未知/阻断/输出全保持字节，其他成功结果继续使用原freeze。F轮仅这两案浅深各自新根复测。
5. E轮scan-sign浅色只在prepare失败：新的父目录尚未建立；无应用启动。原receipt保留，之后新F根先创建明确父目录，重新准备和正式capture。

审计脚本自身初始三次运行发现并修正源快照schema、图片尺寸注册与明确点击清单、table/flow字符串note适配；这些属于只读审计实现问题，不改变或清除产品捕获。[原审计stdout/stderr与实际receipt](audit-execution-originals/execution-receipts.json)同样保留，最终audit-04 exit0。

## 独立复核入口与边界

```text
python doc/validation/assets/B06V-T03/minimum/audit_minimum.py
python doc/validation/assets/B06V-T03/minimum/verify_format_identity_override.py
```

该命令只读原件并重新核对全部ZIP成员、声明冻结源、最终32原生记录、完整执行脚本/安装成员SHA、选定源字节、附件实际点击、主题与owned清理，写新的审计摘要。`--build`只用于首次新归档，已有archive拒绝覆盖。运行时必须仍有明确原捕获和本轮 `.artifacts` source snapshots；迁移后先从归档解包到记录路径或调整验证器的显式根，不能把路径找不到视为验收通过。

本轮没有新模型请求。既有识别/人工环回测试产物的live字段和原HTTP/usage保持原样；本矩阵是跨版本显示验收，不能从它推出一次新的识别请求、真实服务质量、通用OCR、模型权重身份、98%知识召回/事实准确率、24小时任务或3000篇人工库性能。当前稳定版、完整回归、独立终审及B07进入条件由本任务主执行者分别收束。
