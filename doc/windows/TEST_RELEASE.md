# Noetome 0.1.0 Windows 测试版

本次交付用于在 Lyinn 当前 Windows 11 x64 电脑上亲自测试「自己的 PDF → 模型候选 → Obsidian 待核验笔记」。按 2026-11-10 用户指令，独立部署的 VC 运行库问题后续处理，不作为本次测试版交付的前置条件。

- [Release 下载页](https://github.com/dieWehmut/Noetome/releases/tag/v0.1.0-test.1)
- [安装包](https://github.com/dieWehmut/Noetome/releases/download/v0.1.0-test.1/Noetome-0.1.0-B15-6fe4983e8886-windows11-x64-setup.exe)
- [完整测试材料](https://github.com/dieWehmut/Noetome/releases/download/v0.1.0-test.1/Noetome-0.1.0-test.1-test-materials.zip)
- [SHA256 校验清单](https://github.com/dieWehmut/Noetome/releases/download/v0.1.0-test.1/SHA256SUMS.txt)

安装包为 release 优化构建，产品版本 `0.1.0`，Release 标签 `v0.1.0-test.1`，源提交 `6fe4983e8886501374c71af73963ed927d109fd9`。体积 259675053 bytes（247.65 MiB），SHA256：

```text
cf3cc5fdbe4446d8b5d2458931d1d7134983baad38e9e88d6fb1dd91164d12ea
```

## 第一次动手测试

1. 双击安装包，完成当前用户安装，从开始菜单启动 Noetome。安装器尚未代码签名；系统显示发布者未知时，可先按上述摘要核对文件。
2. 在首页点击「运行离线样例」。成功后查看输出目录和 Vault 路径；此步骤不调用模型。
3. 新建测试项目，导入随附 `single-page.pdf`，确认显示可用文字和物理页定位。`scanned-page.pdf` 应显示需复核，这一版没有通用 OCR。
4. 导入自己的文字 PDF，先选择一页或一个小节。在设置中检查模型服务、凭据和预算，预览原文后生成候选。
5. 将候选导出到尚不存在的新测试目录，在 Obsidian 中打开其中的 `vault` 文件夹，核对解释、原文、条件、单位和 PDF 链接。

完整操作见[中文快速开始](QUICKSTART.zh-CN.md)。测试材料 ZIP 包含快速开始、两份作者 PDF、原始 delivery 与载荷清单、运行时凭据、安装/卸载与原生界面验收证据。安装包已经包含 CLI 和 PDF Python 环境，无需自行安装 Rust、Node 或 Python。

## 验证范围

该源的 Windows CI 已完成安装、快捷方式/卸载登记、PDF/扫描状态、离线输出和保护测试目录的卸载验证。安装后的相同程序字节已经在当前 Windows 11 上通过 12 项真实桌面检查，产生的待核验笔记在 Obsidian 1.15.0 中通过 3 篇 Markdown 和 5 次链接点击检查。模型工程测试使用明确的本地假服务，真实外部模型结果留给本次手动实测。

此包允许使用当前电脑已有的 VC 运行库；其他电脑的依赖完备性和缺 WebView2 的首次安装分支仍待验证。原生文件选择器、托盘及多显示器/DPI 人工条目尚未验收。输出始终标为待核验，首次测试请使用新的项目和 Vault。
