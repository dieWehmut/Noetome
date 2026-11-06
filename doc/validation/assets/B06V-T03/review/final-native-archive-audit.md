# B06V-T03 最终原生矩阵与图片原件独立审查

最终索引独立核对通过：`matrix.json` SHA256 `8934eeef639665d0dc7c2e652dc1590d38fe452a21d4c1e0bae59d0c03bc294d`，254,957 字节，16 项独立用例、两版应用和浅深主题组成 64 个唯一组合，其中 60 项为冻结旧产品输出，4 项为单独标记的手写阅读格式探针。每行对应实际原 ZIP/result 原件、独立 expected 指纹、实际配置/Vault 身份、安装三原件、主题、所需执行脚本与来源文件。没有根据索引的 success 标记替代 collector 的原语义检查。

独立重读四个初始 source/native ZIP 的 17,824 个库存成员，逐项 CRC、完整 SHA 和长度通过；64 次捕获中的 812 个 Vault 原文件逐字等于冻结原输出/探针，实际 source-before 和输入 pins 一致。初始矩阵 current 的 32 pass / 10 fail、minimum 的 38 pass / 4 fail 均按原实际 capture receipt 与 result 重算；重复通过没有增加唯一用例，失败没有删除。原始审查见 [final-native-archive-audit.json](final-native-archive-audit.json)，它继续作为旧 64 行及旧格式记录的历史原件。

本轮补充格式记录只替换旧四行。当前版浅深两次及最低版浅深两次都保存实际 `result.identity`，原始配置目录、Vault 和插件值与各自真实 input 相等；三安装指纹、应用/包版本、主题、原 Markdown、Properties 数据类型和可见字段值、两个真实链接点击及 unique visible target 全部独立重验。最低版 aliases 原生类型是 row 上 `multitext`，当前版是 value 子元素上的 `aliases`；YAML 数组与实际 chips 仍逐值相等。块链接保留实际 cache ID/位置及 terminal BR 导致的原单个 LF，只去掉该一个渲染终末 LF 作比较，原正文和作者 expected 不变。

当前补充 [override-records](../current/format-identity-supplement/override-records.json) 的 native ZIP SHA256 `e44b2d924668346f5d34469341db1f146d2d2d7c3472bceaebfbaeec642634d9`，92 个库存成员；最低补充 [override records](../minimum/format-identity-override.json) 的 ZIP SHA256 `5077573e580f1bc6fbf3942aa969ff4df86174e0c97e632ecf887d815b9634d1`，1,276 个成员，均独立完整重读。四项补充的实际 collector 全字节等于最终准备工具派生结果。当前 PID 63456/20980 及最低补充 PID 均正常退出，listener 和 profile 进程清单为零，无 emergency cleanup。当前首次补充 06 在 Node CDP 回环请求遇到 502，未执行 DOM 语义，cleanup 实际有一次 emergency；其 [失败原件](../current/format-identity-supplement-failed-06/receipt.json) 单独保留，不计作成功。

更新后的 64 行见 [final-native-format-overrides-audit.json](final-native-format-overrides-audit.json)。最终 `matrix.json` 每一行又独立比较其 raw result、实际 identity、安装/模块记录和 expected，见 [final-index-rust-source-audit.json](final-index-rust-source-audit.json)。索引没有填入推断的 userData/Vault 字符串。

当前 signed SVG 的浅深两案全部使用最终图片 collector，SHA256 `77926caeab897da3bb3a1f35ff24241cfce37ae1cc321e85f085f109e51ea03a`。原生图片保持实际 app-protocol src，其 URI 解码后精确指向 owned Vault 的 pinned `redrawn.svg`；原 DOM 的图片没有改成 data URL。辅助图片只在同实例辅助窗口使用原 SVG 的 data URL，保存的辅助 DOM 解码后逐字等于 pinned SVG 原件。

用 Pillow 独立解码两个原生显示 PNG 与匹配辅助 PNG，均为 700×380，完整 RGBA 每一个像素相等，没有可接受差异比例。浅色 RGBA SHA256 为 `5a2da9b980b69557ed8238dc80c10a9ced1638b45f036ceb51cf5e8690066990`，深色为 `1e8ed1286d47a98574eed23db5c281480836d31cdd34a217295e46c69333a026`。分别在 960×520 原内在尺寸上重算全图、16 个单 text 隐藏截图和恢复截图；32 个真实差分 bbox/改变像素数量全部等于所存原观察，所有 source-associated 边界都在原作者形状/legend 的原一像素上限内。全标签恢复后的完整 RGBA 与未隐藏原全图相等。另实际重新编码一个 CRC 正确、仅一像素改变的 PNG，其完整 RGBA hash 被拒绝，证明不是仅靠 PNG 损坏检测。

此前 SecurityError 和 nominal font bbox 失败都保留。新方法使用真实可见墨迹证据，没有扩张 nominal margin、修改来源 SVG 或作者 expected，也没有把改变后的原生 DOM 宣传为原生成功。原生图片不变；临时隐藏只发生在独立辅助 SVG，恢复门槛通过后才进入原件/辅助像素对等。

复用边界经过逐字作用域核对。最低版 signed SVG 使用实际 b7 collector，最终 b22 新增的 current-only 墨迹 helper/guard/gate 去掉后，整个 derived 恢复为实际旧脚本；最低观察中没有 `pixelMethod` 或 `actualInk`，原 canvas/bbox 路径及阈值保持。当前 curve-no-data 两案的独立 expected 和 actual 均无 SVG，删除两版全部未行使的 SVG-only 函数/分支与新 PNG helper 后，其余 collector 逐字相等。最后 identity 元数据行只属于格式 collector；其他 60 行的实际检查及原件不受影响。没有宣称整个最新图片脚本曾在最低版执行。

公式的原 MathJax 数学树、脚本/分数几何、真实 glyph 与附件点击门槛保持；Pele 的节点形状、文字、源靶方向、边/箭头及符号标签按实际原 SVG 数据读取；扫描未知负号/下标仍不出现成功表达库存。原生 matrix 只覆盖这些冻结有界工程样例。实际模型请求为零，科学质量 not_run、published false；Linux 完整 Rust test、任意科学内容与生产 Vault 人工保护不由本审查建立。
