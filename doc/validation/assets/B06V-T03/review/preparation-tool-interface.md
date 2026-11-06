# B06V-T03 冻结输入准备工具

`scripts/prepare_visual_compatibility.py` 为一次明确用例、一个独立运行时记录和一个主题准备采集输入。它读取原件，执行对应旧独立校验器，复制原产品字节并派生采集脚本；不启动应用、不发送模型请求。`prepared` 只表示这些准备步骤通过，原生显示仍为 `not_run`。Core、旧四份 collector、两个作者 fixture、模型请求及预算链均不由本工具改写。

命令接口如下。`PYTHON` 使用本轮已选定且有 Pillow 的解释器；实际绝对路径与原始 stdout/stderr 保存在检查收据中。新根的父目录须已存在，新根须不存在，不能位于被冻结的 fixture 内。

```bash
"$PYTHON" scripts/prepare_visual_compatibility.py \
  --fixture-root fixtures/visual-compatibility \
  --fixture-sha256 9d65fc4ae8b3d4b70608c0453b894675aff0d61e2d317acf31b442c54c57915c \
  --runtime-record doc/validation/assets/B06V-T03/runtime/current-runtime-observed.json \
  --runtime-sha256 f7f74bbfcd029d6a9effca499fffd4e380b0b17877fcb17c448f7c573fac60cd \
  --case-id flow-branch --theme light \
  --new-root "$NEW_PREPARATION_ROOT"
```

最低版换成其独立 `minimum-runtime-observed.json` 与实际锚。补充流程包换成 `fixtures/visual-compatibility-flow-extensions`、锚 `e9f6a8b0e86718a38dba5cbf5073c25ffb0a6e39ca0c1d8c43a3648f4952339e`，用例只允许 `flow-complex-labels`、`flow-promotes`、`flow-inhibits`。原包保持严格 13/12/1 清单，补充包严格 3/3/0，两个冻结均未改变。

新根产出 `selected-output/`、`capture-spec.json`、`capture_visual.mjs`、`prepared.json`。根的原件库存、每项 SHA/长度、传递固定的五份旧作者冻结文件、独立 expected 和原输出全字节均先后核对。表格调用真实 SQL 校验包并核对三完整 Markdown 与旧 native 原件相等；其 bridge 属于另一真实导入，因此按独立作者的 objects/properties/cells 等完整语义核对，保留原身份。流程 bridge 与同一独立校验包的完整 plan 和 Vault 字节相等。公式、图像、扫描运行原校验器；扫描不传 budget，复用范围为原响应与语义显示输入，其旧实付链另有 B06V-T02 原件锚。

运行时记录固定 app version、EXE、app.asar、obsidian.asar 各自实际路径/SHA/长度，两个 ASAR 的真实 package version、MathJax/diagram/app.js 内嵌 member 身份。实际新 MathJax window.version 为 4.1.3；静态 bundle 中其他 VERSION 不能替代它。Pele 1.8.0 是独立后端身份，Mermaid version 保持 null；旧 Mermaid 10.6.1 在不导出 global.version 时，仍须有 installed source literal 加实际执行 member SHA。派生 collector 用 CDP executed script source 绑定 member，并核对真实 EXE/安装原字节、profile/Vault、主题、原输出及数学/图形/字段。

采集执行者可沿用旧 `launch_capture.py` 的输入接口，或保存其派生 launcher。`visual-input.json` 须含 `profile`、`vault`、`port`、`spec`（新 capture-spec 全内容）和 `source_pins`；`--capture-script` 传新 `capture_visual.mjs`。启动、干净配置和拥有的 PID/监听端口清理属于 launcher 的独立验收。准备工具不会代替这些行为。

Pele 分支按原生 data-id、SVG text、实际形状、线段、箭头三角原路径与几何核对源靶、判据和促进/抑制，保留旧 Mermaid 所有检查。Properties 中作者 list 概念仍按 YAML array 逐值核对；最低版真实控件 enum 为 `multitext`，只作显式原生类型映射并记录两种身份。失败前保存实际 DOM/截图/检查中记录，便于追查结构差异。PNG 附件继续有 T02 的真实点击、image view、固定自然尺寸、完整加载和本地 URL 检查；来源关键未知仍不允许成功产物出现。

focused 原件分别保存在 `preparation-tool-validation-originals.zip` 与 `preparation-tool-validation-02-originals.zip`，各自有 JSON 收据、实际命令及源指纹。首份有 17 项测试、六类 JS syntax 和一次真实 current 安装/flow 原件准备；第二份针对有据 Properties enum 适配后复查。inert runtime 单元样例明确为非原生证据，Node 几何篡改检查验证符号/源靶/反向箭头/脱离目标/隐藏标签拒绝，实际跨版本显示由 root 和 runtime agent 正式运行。未执行 Linux 完整 Rust test，不从这些准备检查或 Windows 捕获推定 Linux 完整运行兼容。

最终工具固定为 SHA256 `9c28fd119634c14d2c28d24451799175fe3fa9aceaeae10566f000e52767c83f`（68,922 字节），测试文件 SHA256 `324fa89215b147da96284f3e6d8a39cadc7ebceeef0bf10a25003449d2e1c796`。最终实际 21 项 focused tests、六类派生 syntax 和 diff 检查通过，见 `preparation-tool-validation-05.json` 及其不可变原件 ZIP。当前 UI 的 type 位于 `.metadata-property-value` 子元素，明确记录 row/child 原属性，aliases 映射到独立作者 list 概念并继续检查 YAML/chips；实际 title 控件和 checkbox 也逐值核对。结果保存实际配置目录/Vault/plugins 身份，最后仅一行元数据增补已在两版浅深四次格式捕获复测。

独立诊断勘误包 `fixtures/visual-compatibility-corrected-v2` 的锚为 `c174f543947b6e72c7ef7e786624c4d2fa870342df79abcd9d0fc7fbb90cc60d`，仍使用相同严格 13 用例清单。它只纠正 sign/subscript expected 的旧诊断字符串录入错误，原产品输出/作者原字节与旧 freeze 保留；语义、未知阻断和成功库存未改写。需要该两个阻断显示用例时，显式更换 `--fixture-root` 和 `--fixture-sha256`，不修改原 13 包。

当前 SVG 用原生 app-protocol 图片截图与同一已验证应用实例中的 pinned SVG 辅助图比较完整 RGBA；原生 src 不变。来源几何先在内在尺寸核对，每个 text 的真实墨迹由原全图与单 text 隐藏图逐像素差分取得，恢复后的全图必须原样相等，原一像素 source 边界不扩大。最低版旧 canvas/bbox 路径保留。真实 64 行、源码复用边界及四项格式身份补测见 `final-native-archive-audit.md`；源码 schema、版本/脚本绑定、同步伪造拒绝见 `final-code-capability-audit.md`。
