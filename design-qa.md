# 侧边栏与设置视觉验收

本次以用户提供的两张 Codex 截图作为侧栏样式参考。图中未展开设置内容，因此设置页按同一中性配色、列表层级和留白实现，另以真实交互验收。用户已澄清主题按钮为设置上方的太阳/月亮直接切换亮暗。

## 第一次对照

- Source visual truth：用户附件 `codex-clipboard-0b7695ac-4582-4df1-9bb5-e6d4c592b0c8.png`，原尺寸 584 × 1964；位于本轮用户提供的 Temp 路径。
- Implementation：`.artifacts/b05u/browser-sidebar-before-qa.png`，实际 IAB 页面截图 1200 × 1030。
- Viewport：1200 × 1030 CSS px。参考图按 2 倍密度缩为 292 × 982；实现裁出 x=0、y=42、width=292、height=982 的完整侧栏内容，同图并排于 `.artifacts/b05u/sidebar-comparison-before.png`。
- State：亮色工作台。参考图有用户其他项目与聊天；Noetome 显示当前真实空项目和实际访问过的页面。对照对象是栏宽、图标、层级、留白与选中态；没有复制参考中的其他项目或聊天。
- Full-view comparison：已打开并检查上述完整并排图片，顶部与底部也在同一比较输入中可见。参考私有内容和比较图保留于本地，不发布到远端。

发现：P2，首行操作与项目分组相对参考图偏低约 24px；P2，未选中主题图标与背景对比度实测 2.77，图标偏淡。

修正：收紧侧栏标题、首行及分组间距和列表行高；保留滚动条空间，避免列表宽度随滚动变化；亮色未选中图标颜色改为 #868686。

第一次对照保留上述失败，随后实际重新截图和检查，结果如下。

## 最终完整对照

- Source visual truth：同一用户附件 584 × 1964，归一化到 292 × 982；图中设置未展开，因此设置内容依据本轮已确认的左侧分类、右侧字段布局。
- Implementation：实际 IAB 截图 `.artifacts/b05u/browser-sidebar-final.png`，1200 × 1030 CSS px；完整侧栏裁切与参考并排于 `.artifacts/b05u/sidebar-comparison-after.png`。
- Viewport and state：1200 × 1030、亮色工作台；主图标栏 52px，内容侧栏 238px。项目空状态与最近访问为本应用实际数据。
- Full-view comparison：实际打开并逐段检查完整并排图，包含顶部品牌/首行、项目和最近分组、滚动范围、底部主题及设置。参考图与含其他项目标题的并排图仅留本地；仓库只保存 Noetome 自身截图。
- 差异收束：首行及分组间距已收紧，当前项目标题距参照约 6px；主栏宽度、列表层级、细边框、浅灰背景及圆角选中态与参考结构一致。参考图没有主题按钮，本次按用户要求将月亮/太阳加在设置上方。
- 对比度：亮色未选中图标 #868686 / #f3f3f3 实测 3.28；暗色未选中图标实测 5.33。旧 2.77 结果保留在历史，未作为最终通过。
- 设置与窄窗口：实际检查 1200、640、360px；640px 长内容独立滚动且底部按钮固定，360px 默认收起侧栏、打开后导航自动收起，设置字段和键盘焦点可见，无横向溢出。真实搜索遗漏和覆盖焦点的两项 RED 已修复并取得 GREEN。
- 安装版：实际当前用户安装程序 WebView 的亮/暗截图均已完整打开检查，1200 × 800 CSS px、DPR 2；设置分类、字段和底部太阳/月亮与浏览器一致。实际 Rust 帮助/版本、连接检查、关闭偏好保存/重读、退出及程序重启后的主题持久化均通过。

归档截图：[侧栏](doc/validation/assets/B05U-T01-sidebar.png)、[亮色设置](doc/validation/assets/B05U-T01-native-light.png)、[暗色设置](doc/validation/assets/B05U-T01-native-dark.png)、[360px](doc/validation/assets/B05U-T01-browser-360.png)、[640px](doc/validation/assets/B05U-T01-browser-640.png)。行为与错误记录见[本项验收](doc/validation/B05U-T01.md)。

Windows 原生输入工具本轮未能可靠触发控件；安装版行为使用仅本次测试进程的本地 WebView 诊断接口完成。该工具失败仍记录，未宣称实体指针、窗口拖动或托盘全矩阵通过。视觉与本项已要求的浏览器/安装版行为均有实际证据，无未解决的必要视觉问题。

final result: passed
