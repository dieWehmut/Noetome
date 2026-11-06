# Windows 候选快照第一轮

本目录保留一次真实候选准备与失败，不能作为最终产品回归通过证据。源目录为 `D:/Noetome/target/b06v-t01-source-final-20261106a/repo`；路径中的 final 是当时选定名，后续审查要求暂停，实际状态为 candidate。

544 份当前产品、fixtures、tests、scripts、应用源和明确被测试引用的旧原件被固定复制。全程源字节与外部哨兵指纹相同，旧 `D:/Noetome` checkout 仍仅保留原有两份旧 docs 删除，没有修改旧源码或安装。

实际 Windows CPython 3.12.14 及锁定 PDF 八项依赖通过版本核对，fake runtime 也实测为 3.12.14。fmt 退出 0，workspace check 退出 0，CLI prebuild 退出 0，但它沿用共享 target 中旧 B06-T03 的 CLI SHA256 `8b062568e44014bcc6fa79deba26e563a98649273e0192cf08026069e58598ab`。随后的 workspace tests 编译退出 101，提示 bin 找不到 image_fidelity 及 CliCommand::Image；新快照 lib.rs 实际包含两项，源指纹保持相同，旧 CLI 的 knowledge image --help 实际拒绝。此记录表明共享缓存错用旧产品产物，不能把前置退出 0 视作当前编译成功，也不能宣称 Rust 测试运行通过。

暂停时没有正在运行的 Rust 或原生 Obsidian。没有调用最终新 CLI、Python full 或四项视觉 audit，没有在此候选上进行 native capture。下一轮会保存本轮证据后，在选定缓存中显式记录仅对 noetome-cli package 的缓存清理，真实重建并检查 image namespace，再执行完整检查。

[源码原件](source-originals.zip)和[源码读回清单](source-originals-manifest.json)保存全部输入及 snapshot/config；[运行原件](regression-originals.zip)和[运行读回清单](regression-originals-manifest.json)保存命令、stdout/stderr、runtime、PID/退出、指纹与哨兵记录。后续背景对比反例修复需另立 fresh source snapshot，本候选不覆盖或更新。
