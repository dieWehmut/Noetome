# SVG 原生能力探针

本项是公开自造 SVG/PNG 能力探针，先于最终产品实现和 CLI 实测。原图和 SVG 由明确作者结构产生，只证明有限结构可在此安装中显示，不证明一般图像自动理解或本任务产品闭环。

实际安装 Obsidian 1.6.7，Electron 30.1.2，Chromium 124.0.6367.243；安装 exe SHA256 为 `2156f15d12da8cc292fe0cc1884f32410fa187fede67beb6102cfcdc6442fbe2`。新 Vault 和用户配置仅位于 `D:/Noetome/target/b06v-t01-svg-probe-20261106a/`，community plugins 为空。没有重装应用、读取生产 Vault 或修改旧 Windows 项目。

成功第五轮在真实笔记中加载 `app://` 本地 SVG 和来源 PNG，均为 760×430，预览有效区域 700×396.046875。实际截图显示入口 Inlet (+) 在左、Outlet (-) 在右，水平箭头指向右侧出口；箭头上方保留 2 mL/min，绿色 Probe 位于两框下方，Green = sensor 图例也在图内。SVG 和自造 PNG 的字体及箭头宽度有差异，均明确保留同一结构，不将像素不同称为原件精确重现。

Obsidian 将 SVG 作为外部 img attachment 显示，笔记 DOM 没有内部 SVG 节点。因此直接采集原生已加载 img 的 canvas RGBA，再由已核对归属的同一 Obsidian 实例创建临时 BrowserWindow，打开相同指纹 SVG 字节并记录实际渲染文本 bbox、形状 bbox、line 和 marker-end。两次解码的 RGBA SHA256 均为 `a522226ad7a0d50def8eacd656a6b5cba0094116fa501c802042d3d05a7794de`；所有五个实际标签为正面积。独立结构预期核对了 Inlet 在 Outlet 左侧、Inlet 在 Probe 上方、line 从左到右且箭头只在终点。记录如实区别辅助窗口的 vector DOM 证据和笔记原生 img 显示，不把 source metadata 的出现当作图内语义。

来源 PNG 链接经真实鼠标 click 打开原生 image view，activeFile 为 `probe-original.png`。原件、复制文件、capture spec 三者前后指纹一致，成功轮和四轮技术失败后自有 PID、CDP 监听均消失，profile 专属进程数为零。

失败历史保留在[探针原件 ZIP](probe-originals.zip)：第一轮 Electron CDP 不支持 Target.createTarget；第二轮 SVG marker 定义没有 getBBox；第三轮 SVG XML document 创建的 canvas 没有 HTML getContext；第四轮 Playwright fullPage screenshot 依赖 HTML 滚动根而在 SVG 文档超时。第五轮改为自有 BrowserWindow、定义元素 bbox 为 null、HTML 命名空间 canvas、Electron capturePage 后通过。未修改产品、放宽图像预期或覆盖早期失败记录。

[完整原件清单](probe-originals-manifest.json)保存逐个读回 SHA/长度。后续最终 CLI 输出仍须独立核对、实际原生显示及完整回归；本探针不是本任务完成证据。
