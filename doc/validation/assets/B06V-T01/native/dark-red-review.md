# 实际 CLI 透明画布暗色主题红灯

2026-11-06 00:10:23Z 开始，00:11:53Z 得到真实原生红灯，00:11:55Z 完成自有进程清理。当前 CLI 先复制到新的测试根固定为 SHA256 `a81020af4f0c1569bd8e176e6d6c0866faa75144350ca640e115f4f1cc7055f6`；仅运行 `signed_arrows` 普通 `knowledge image`，退出 0。生成的 `image.json` 自报 passed，而本次原生显示检查明确失败，不能据此前置报告声称视觉已通过。

新 Vault/profile 只位于 `D:/Noetome/target/b06v-t01-dark-red-capture-20261106a/signed_arrows/`。实际应用为 Obsidian 1.6.7、Electron 30.1.2、Chromium 124.0.6367.243、无第三方插件。独立配置请求 `theme=obsidian`，实际 body 有 theme-dark，背景为 `rgb(30, 30, 30)`，原生 preview 的 background-primary 为 `#1e1e1e`。

实际 `app://` SVG 以 960×520 解码，源 SHA256 `1a18142bd45802dfc81e1140956fff972212bbac28972231b085aed07b4b2ec7`，native RGBA SHA256 `67c59b021c5fcde3601f2a0310e81e7f9496a55fc12d605603b8973f19a247c5`。499200 个像素中 382726 个不是不透明像素，左上像素为 `[0,0,0,0]`。同一应用实例的辅助 SVG DOM 记录了黑色 `+`、`−`、activates、inhibits 和 s^-1 标签的实际 bbox；它们在暗色背景中几乎不可读。[真实截图](dark-transparent-red.png)显示节点与图例底色可读，而箭头旁关键标签、符号和单位明显缺乏对比。

这项失败来自真实原生显示和透明像素断言，capture 退出 1。前置独立 checker 在当时 SHA `f88b0ee5f114cb49066858dcf612d5e93ec1462943cdf71f226e991b798b13a1` 下实际退出 0；它随后并发更新，准备后观察的副本 SHA 为 `aa7a924775a60e6b79863b78bbc9c0addba9f4c95cc3d900bbeee7aa0f8c0203`。保存文件如实命名为 `checker-observed-after-preparation.py`，不冒称它就是前置执行原件，也不把后续 checker 与旧 CLI 的版本错配当作本轮 native RED。

原输出、复制到原生 Vault 的文件及 capture spec 前后指纹均相同；owned PID、CDP 监听均已退出，profile 专属进程数为零。没有修改原作者 expected、生产 Vault、安装版本或旧 Windows 源码。修复目标是保留来源 PNG 的完整白画布，并使 SVG 的 line/head 保留来源 PNG 的黑箭头；之后须对最终 CLI 新输出重新执行全部原生场景及暗色对照。

[红灯原件 ZIP](dark-red-originals.zip)和[逐成员读回清单](dark-red-originals-manifest.json)保留 113 份命令、输出、来源/expected、实际 SVG/PNG/报告、截图、DOM、版本和清理证据；可重复构建的 CLI 大文件只记录指纹，没有放入 ZIP。
