# 最终图像原生验收

实际完成 17 个干净 Obsidian 1.6.7 场景：冻结的 12 个作者用例、signed_arrows/spatial/escaped_labels/curve_no_data 四个暗色主题重复、独立黑底白字颜色反例的暗色原图回退。17 次均成功，包含 6 次 SVG、6 次 PNG、5 次阻断报告，以及 12 次真实原始 PNG 链接点击。三组 SVG 的明暗主题解码像素完全相同。完整汇总和程序联结见[final-summary.json](final-summary.json)。

原生输出全部由最终 Windows PE 的 immutable 副本产生，SHA256 为 `ed54203ae6a6a7eaac499045b3a221843fa7c03bf249d2902c4580031ba1dc12`，长度 25182720；它来自最终固定产品/测试源的 Windows 完整回归。十二作者用例分别执行普通 CLI 和独立 frozen expected check，另一个颜色反例使用此前独立 closure 的明确来源指纹和完整 note 字段/映射核对，没有加入或放宽十二 expected 的固定 roster。

每个场景都建立新的 Windows Vault 和 profile，只复制明确选定的 CLI 输出。实际记录 userData、Vault basePath、Obsidian/Electron/Chromium 版本、本地 app:// 图像 URL、原 PNG/SVG 尺寸和加载完成、实际相对链接解析、鼠标 click、完整两次滚动 DOM 与截图。全部记录第三方插件为空。实际 Obsidian 1.6.7、Electron 30.1.2、Chromium 124.0.6367.243；安装 exe SHA256 为 `2156f15d12da8cc292fe0cc1884f32410fa187fede67beb6102cfcdc6442fbe2`。

SVG 是 Obsidian 的外部 img attachment，笔记 DOM 没有图内 SVG 子节点。验证先直接从原生已加载 img 获取实际 RGBA，再在同一已核对归属的应用实例内建立临时 BrowserWindow，打开同一 SHA 的 SVG 字节并取得渲染 bbox、text/shape/line/polygon 和 computed visibility/fill。辅助 decode 的全部 RGBA 与原生 decode 相同。辅助 SVG DOM 明确是补充几何证据，不冒称为笔记的内联 DOM。

六次 SVG 均实测完整白色不透明画布；所有图内标签/单位/正负和图例值精确匹配独立作者声明，实际有面积且可见，源 shape bbox 逐坐标相同。SVG text getBBox 含字体名义 ascent/descent，label 与所属 shape/legend 边界允许 1 像素 font-metric 余量，shape 几何不使用该余量。line 原端点与实际黑色 stroke、polygon tip 的独立方向、双翼在 tip 后方且两侧均通过。实际 left_of、above、inside 的形状关系通过。标签黑色 ink 与实际白 canvas 或保留的图例填色计算最小对比为 13.280532277783054；箭头旁黑标签在白 canvas 中为 21:1。

实际回看了[正负箭头暗色截图](signed-arrows-dark.png)、[空间暗色截图](spatial-dark.png)及[字面转义暗色截图](escaped-labels-dark.png)，并逐图比较来源 PNG 与原生解码 SVG：[正负/单位/图例](signed_arrows-source-vs-native.png)、[包含/左右/上下](spatial-source-vs-native.png)、[特殊字面与 Unicode 单位](escaped_labels-source-vs-native.png)。重绘字体与来源 PNG 不同，图意、重要标签、单位、符号和形状坐标保留；caption/条件/例外移至实际正文并由独立 note 核对和滚动 DOM 验收，原 PNG 仍保留并实际点击。这里不声称重绘与来源 PNG 像素相同。

[颜色反例暗色截图](contrast-original-dark.png)实际保留黑底白字 Payload/mg 和 Dark/Selected payload，正文记录有据颜色回退，没有 SVG；curve_no_data 保留完整原 PNG，未生成虚构曲线数据。五个阻断场景实际打开 image.md，确认 needs_review 及相应问题，缺少受影响 preview/plan/SVG，原件仍保留。

17 个场景的明确原件、复制件、spec 指纹前后相同；所有自有 PID、CDP 监听和 profile 专属进程均已退出。未重装 Noetome 或 Obsidian，未读写生产 Vault 或扫描未选私人资料。当前/最低版本兼容矩阵、一般图像自动理解、OCR 和科学质量均未运行，本结果只声明此安装下明确有限结构。

[最终原件 ZIP](final-originals.zip)和[逐成员读回清单](final-originals-manifest.json)保存 1173 份真实来源/expected、普通 CLI/check 命令、双报告、原始 SVG/PNG、DOM/几何/截图、profile/theme 与清理证据。早期[能力探针](probe-review.md)、[真实暗色透明 RED](dark-red-review.md)和[并发 checker 变化的准备失败原件](preparation-attempt-1-originals.zip)保留，不替代最终程序实测。
