# B03-T02 实际红灯记录

在任何 PDF adapter/CLI 实现之前，已运行：

```powershell
./scripts/cargo_local.ps1 test -p noetome-cli --test pdf_parse --locked --offline
```

结果：退出 1；成功编译后 5 组全部失败、0 ignored。单/多页与位置、边界矩阵、原文件移动后 inspect、新根/缺失输入保护及语义篡改均因普通入口还没有 `parse-pdf` / `inspect-pdf` 而失败。首个正常用例 stderr 为 `CLI_INVALID_ARGUMENT：参数无效或缺少项目路径。`，边界期望退出 3 实际退出 2；不是测试语法或依赖编译错误。

固定输入与 `expected.json` 由独立 authoring generator 先创建，未运行待测 PDF 解析器。后续 Core 定位负例和修复红/绿另在最终验收中追加，不能把这次缺入口失败视为所有定位校验已经证明有效。

初始 fixture/spec 独立审查核对了 14 个 raw SHA、19 个实际渲染、矩阵角点和真实红灯，指出两项重要验证缺口：非零旋转/review 页没有字形位置 oracle；语义变异只断言非零，可能被旧 Markdown 提前拒绝。已按独立 authoring 加入逐字形位置、旋转 CropBox 偏移与 UserUnit 预期，并要求变异失败属于具体 PDF 语义/资源错误族。重新渲染新输入另存，不抹去初始证据。另修正 review 工具漏 PNG 可继续的缺口，可渲染页必须完整。

Core 定位审计空实现上的实际红灯：`./scripts/cargo_local.ps1 test -p noetome-cli --lib pdf_validate::tests --locked --offline`，退出1，1个合法 synthetic DTO 通过、10个语义负例全部断言失败，0 ignored。负例覆盖缺页、错误页、页内错误 bbox、缺正文、UTF8字符切断、错矩阵、不诚实 parsed、未知页总数、越界字符及资源SHA引用。实现后同命令退出0，11项全部通过；这只证明 Core 契约，不代表真实 PDF 已通过。

adapter 直接实跑又发现新增 glyph oracle 错用了视觉行序：`layout-order.pdf` 的原始 drawing 确为 Second 后 First，glyph ordinal 应保留原 drawing 顺序，region/fulltext 才使用 First 后 Second。按未压缩原始操作符纠正 authoring oracle 的行序来源；原 PDF 字节不变，未根据 parser 输出补造正文/坐标。真实子进程 argv 曾超过 Windows CreateProcess 长度（WinError206），adapter 缩减重复守卫/DTO后继续实测，失败与修复命令保存在最终验收中。

首次 Core/adapter 集成5组失败：测试fixture路径保留显式 `../..` 被既有安全守卫拒绝；修正只发生在受信测试fixture的 canonicalize，不放宽产品路径。另一个中文重命名文件真正进入 parser，但区域正文范围错误包含LF，Core返回PDF_TEXT。直接 authoring 字节断言随后真实先红再绿，范围改为只含正文，artifact仍追加LF。

可信压缩payload首次重编后，同一 `--test pdf_parse` 实跑2通过、3失败（退出101，0 ignored）：

- 扫描页 manifest 本身 needs_review，但 generic `unit.text=[0,0)` 与已验收protocol-v1的非空范围不兼容，宿主返回SIDECAR_UNITS。新Core专测在能成功编译后真实0通过/1失败，错误为PDF_TEXT“unit has no whole-page text range”；随后Core支持空artifact但wire引用null，12个Core语义测试通过。adapter同一扫描/空白专测另存红绿证据。
- traversal 已正确返回SIDECAR_WORKSPACE、未创建unused/escaped目录；原测试错误地要求CLI语法错误族，现按实际文件安全层断言。
- 调换完整region对象也调换region_id，先被PDF_LOCATOR拒绝。修正测试将ID重新编号后，继续要求PDF_TEXT，才能证明真实阅读顺序/字形分配审计而不是仅名称规则。

第一次编译新增Windows PID边界测试被workspace `unsafe_code=forbid`拒绝，尚未进入测试，不计为功能红灯。去掉测试Rust FFI，改用可信stdlib Python ctypes查询真实PID；权限错误必须报错，不能假称进程已退出。未放宽Rust lint。手动Low标签CLI向Medium `.artifacts` 输出也实际AccessDenied；后续验证使用明确的新 `target/compiler-temp` 沙盒，未更改ACL或绕过路径守卫。

第一次整项code review在已有360 Rust / 40 Python全绿后，另用真正作者PDF复现3个P2：基线错开双栏仍parsed、三个空格仍parsed、ToUnicode解码A+space后B被adapter多插空格导致Core PDF_TEXT。旧支持事实/23页渲染、原expected指纹及变动前两个raw均归档，未覆盖该阶段真实结果。

作者在columns-page/blank-page各加第2页，另外12份原始PDF逐字节不变。新Core负例真实允许了三个错误parsed，专测退出101（3项新失败，两个同filter的旧测试通过）；未因编译或runtime缺失失败。新真实CLI7组先5通过/2失败：新矩阵逐页状态发现parsed应needs_review，ToUnicode独立CMap字面A B被写成A双空格B并被Core拒绝。随后Core相邻行几何候选、无非空白正文、多码点字形的复核政策令15个Core测试通过；adapter三项分别先红再绿后31方法全部通过。重生成短嵌入payload后7组真实CLI全绿，含原始保留、UTF8片段、needs_review和readonly inspect。

跨相邻行规则是保守候选检查，不是完整分栏算法；CMap专测不证明该编码字体宽度真实位置已独立验收。当前主样例为14PDF/25可渲染页，新增页已独立Poppler回看；直接adapter/主CLI/全回归分别记录，不以旧全绿数量冒充新修复完成。

最终整workspace第一次实跑失败：库271项中270通过/1失败，取消测试在看到`handshake.pid`创建后读取空内容，`ParseIntError Empty`，并非parser产品失败。给原helper在create/flush后暂停0.2s，定向真实0通过/1失败、退出101，稳定复现该写入发布竞态。随后只将helper改为写pending、关闭文件、flush诊断、rename后发布，仍保留暂停和真实PID存活/退出断言；定向cancel/timeout两项全绿，最终完整364 Rust全绿、0 ignored。失败、红绿和最终输出均归档于[验收](B03-T02.md)，没有放宽产品限额或跳过测试。
