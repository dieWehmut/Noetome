# 当前数学、流程、图像与扫描的最终 WindowsC 编译源绑定

本轮在有实际编译源码原件的最终 WindowsC PE 上重新运行全部 **30 个 current 主 CLI 调用**，并为五个成功流程作者在新本地项目执行 **15 次额外 init/import/process-review**。总计 **45 CLI、27 个领域场景均通过独立 verifier**；source/PE 原字节前后相等。扫描仅数字环回重放四份原响应，**4 synthetic POST、real_model_calls=0、paid_calls=0**，外部真实 33,185 ledger 未读取或修改，UnknownConfidence 保留。

旧 19 数学/流程、4 扫描和 7 图像调用的 ELF 只有 binary receipt，缺少实际 build-source 锚。本轮不推断旧 ELF 的编译来源；旧 30 原件作为历史检查/行为证据保留，由本轮强 PE 原件证明当前最终实现行为。

## 实际编译来源

实际 WindowsC PE 为 **25,939,968 bytes / SHA256 `e42c534d07f66662411fc52f22561510b79ba1c63f14f8b6f419db7c4e0e3274`**，来自原 native archive `c/noetome-selected.exe`。源 authority 是已有 [Windows Rust source ZIP](../../windows-rust/source-originals.zip)，SHA `a83d9c0ee111344ba8c45433dcc70021897b3ae1bb9256fd3a6a43f9440a0370`；PE/build receipt authority 是 [native ZIP](../../windows-rust/native-originals.zip)，SHA `058fed0632bcfc69f43764ad1893cebad831c48e0d99b3b6a49c1bd1923329e0`。

其中 C 阶段实际 `cargo build -p noetome-cli --locked --offline` 的 cwd、真实 pid、exit0、原 stdout/stderr 指纹、CLI before/after 与 snapshot 指纹均直接保存并核对。实际编译 snapshot **1,915 文件**，保守 Rust 编译/输入子集 **1,825 文件**；全部原件逐 SHA/长度读回。详见 [build-source-binding.json](build-source-binding.json)、[原实际 build receipt](cli-build-attempt-1.json)、[snapshot](snapshot.json) 和 [Rust dependency subset](rust-dependency-subset.json)。

运行时独立 Python 另行从当前固定源码选择，不从 Rust 实现或产物生成判据。实际 C 完整编译源可由本包 runtime 原件加两份旧非 Rust Python 原件（prepare 和 flow test）恢复；**全部 1,915 snapshot SHA/长度实际逐值相等**，1,825 Rust 子集与运行时源全同。[完整重建核对](complete-compiled-source-reconstruction.json) 明确区分编译原件与当前独立 Python，不把新增测试 setup 算作已经参与旧 C 编译。

## 真实调用与原生原件联结

| 领域 | 主调用 | 实际覆盖 |
| --- | ---: | --- |
| formula | 6 | 当前 native/native-relational、unknown/mixed fallback、源未知符号阻断 |
| flow | 13 | 5 当前作者、4 unknown/mixed/renderer SHA fallback、来源未知的 3 个准备调用和 1 个阻断调用 |
| image | 7 | 当前 SVG/原图、unknown/unverified/svg flag fallback、源关系未知阻断 |
| scan | 4 | 当前 clear native、native_math=false 原图回退、sign/subscript 均仍阻断 |

额外 15 次 flow 准备调用单独计数，不混入主 30。Windows Core 不接受冻结旧项目中的 Linux 绝对 import_path；本轮通过普通 CLI 构造真实 Windows 本地项目，读取原作者 source/gold/expected，不改原 SQLite，不伪造 SourceSession。

**13 个 byte-binding case**（包含两个 blocked 扫描的原区/整页/PDF）实际逐文件与已正式 native-captured 冻结产物比较。**16 份产品 Markdown** 及存在的 SVG、原/compiled/recognized TeX、PNG、PDF 全部原字节相同；原识别 JSON 语义和原 response.bin 也相同。目标 metadata、平台路径、fresh UUID 和 synthetic HTTP receipt 允许如实不同，没有将这些报告冒称相同，也未新增原生应用 capture。[actual-checks.json](actual-checks.json) 保留每个文件两端 SHA/长度和实际 bytes_equal 结果。

七类有意义同步造假被独立拒绝：公式负号丢失、真实 `decision.representation` 的数学目标伪造、流程判据改变/renderer 伪造、SVG 箭头头部丢失、真实 `decision.representation` 的 SVG 目标伪造、无原数据的假曲线 CSV。所有普通 artifact hash/双报告已同步；数学和 SVG 目标伪造保持真实 schema，拒绝来自独立计算的 source-grounded representation。另两次早期 top-level 多余字段只证明 exact schema 拒绝，原件作为 preliminary 单独保留，**共 9 次实际 verifier 造假检查，7 类语义/目标门禁**。

首个新 root 缺输出父目录被 Core 正确拒绝 exit2；过早启动 Python 尚未找到脚本；后置 runner 的原文件名和判据位置假设分别导致 FileNotFound/AssertionError。所有原记录/脚本/副本都保留。后置检查直接复用已完成 45 调用的真实输出修正，不重新调用 CLI、HTTP 或模型；初版多余 schema 字段随后在新的副本改为真实 decision 字段并独立重验，没有放宽 verifier 或 expected。

## 封存

| 原件包 | 成员 | Bytes | SHA256 |
| --- | ---: | ---: | --- |
| [source-originals.zip](source-originals.zip) | 1,946 | 14,826,159 | `0a1e2782df12a5d9cd9a91fc76e4d2f65c01c68889da116da6ebc5478a3792b6` |
| [native-originals.zip](native-originals.zip) | 813 | 1,756,810 | `764629cf2b6e5cde1dff2cee1778493e3e89f8163aa72e2267e87ef308bb6bbd` |

两包 2,759 成员均实际通过 CRC 与每成员 SHA/长度读回；完整库存见 [source manifest](source-originals-manifest.json) 和 [native manifest](native-originals-manifest.json)。source 包含真实 PE、运行时源、C 编译原件变体及原 build/snapshot 证据；native 包保存 45 调用的 stdout/stderr/receipt、真实新项目、领域产物、原响应重放、全部失败和造假副本。

本轮总收据 [actual-checks.json](actual-checks.json) 为 **106,074 bytes / SHA256 `b2ae2b65dd682bf05ea5c957eefbde0913fef6132bb0784b858495f74ef0dc85`**。执行临时根 `D:\Noetome\.artifacts\b06v-t03-c30-02` 原件仍保留。最终 portable flow test 后的完整 Linux 380 项另见 [Linux04](../../linux-final-after-portable-test/README.md)；Windows full441/原生矩阵和 Rust全套由根代理独立汇总。本轮未证明科学正确率、通用 OCR、模型内部权重或生产 Vault 人工保护。
