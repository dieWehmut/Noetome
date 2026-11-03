# B04-T03 用户追加的单行窗口栏验证

用户2026-11-03明确要求“形成两行了啊，只要第二行”。现有36px Codex菜单已实现，但Windows的Noetome系统标题行仍在上方；当前改动移除系统标题行，仅保留菜单一行，并把最小化、最大化/还原及关闭按钮放到同一行右侧。

![安装后实际单行顶部](assets/B04-T03-ui-native-header.png)

[完整原生窗口](assets/B04-T03-ui-native-full-window.png)来自实际已安装应用的完整屏幕窗口捕获，按DWM可见边界去除窗口外的不可见调整大小边框；顶部像素保留，没有裁掉系统标题行。[裁切记录](assets/B04-T03-ui-native-crop.json)保留原捕获SHA、完整窗口/DWM坐标和裁切范围。原始含窗口外背景的屏幕文件只保留在本机。对照[用户提供的修正前两行截图](assets/B04-T03-ui-native-before-user.png)，第一行已消失。

## 实现与新鲜组件审查

`tauri.conf.json`设main窗口decorations=false；仅main capability增加5个必需窗口命令及event listen/unlisten。新的WindowControls只处理当前窗口状态和三个操作，关闭继续走既有CloseRequested与托盘/退出偏好，最小化保持普通Windows行为。拖动来自菜单以外的空白区域；菜单、导航和窗口按钮不会启动拖动。双击空白只走一次toggleMaximize，未叠加自动drag-region处理。

实际浏览器RED为1通过/8失败，第一版GREEN9/9；窗口查询恢复新增RED9通过/1失败，最后GREEN10/10。TypeScript检查与Vite生产构建exit0。[保留的浏览器检查](assets/B04-T03-ui-implementation-check.mjs)明确标native_os_proof=false；浏览器结果用于键盘/布局/错误/IPC边界，不代替原生窗口操作。新鲜[Task3审查](assets/B04-T03-root-task-3-review.md)SPEC/QUALITY通过，没有重要问题。一个非阻断建议是失败的状态读取恢复后可精确清除旧状态读取提示；现有提示可在下一次窗口操作时清除，[最终整分支审查](assets/B04-T03-final-review.md)已确认该Minor R2可继续延期。

## 实际构建、更新安装与完整窗口

当前NSIS SHA为`79872d9ca0795f736f649d4e0a2fcc3927450c8943ed4c790d1f3995b1ba834f`，实际构建exit0，更新安装exit0。安装位置为当前用户`LocalAppData/Programs/Noetome`；安装后程序11101184字节，SHA为`870efa1a55bc33d48a731765150834f2caf06fe6be8f8294af6b1086dfb476c7`。安装器依照既有Tauri NSIS标记比较当前构建，快捷方式和卸载记录相符。[构建命令](assets/B04-T03-root-desktop-build-command.json)、[安装命令](assets/B04-T03-root-desktop-install-command.json)及根证据01中的完整安装收据保留实际过程。只停止已核对路径与SHA的原Noetome进程，没有停止无关程序。

原生客户端顶部实际y=0、高36px；File/Edit/View/Help与3个启用的窗口按钮都处于该行。完整窗口的client_top_offset为1px，扣除边框后的native_title_band_height为0。首个辅助检查错误地要求WS_CAPTION位必须为0而失败，原记录保留；锁定Tao0.37.2的window_state.rs保留该样式位，而WM_NCCALCSIZE在decorations=false时重算客户端，因此样式位不能代表可见标题行。修正oracle后以[原生几何](assets/B04-T03-ui-native-title-band.json)和完整可见窗口共同验收，没有为了错误样式判据改产品。

## 原生操作与人工实机确认

| 检查 | 实际结果与方法 |
| --- | --- |
| 最大化、键盘Enter还原 | 点击真实DOM按钮触发已安装Tauri API；OS IsZoomed/IsIconic/IsWindowVisible与按钮名称相符 |
| 最小化与恢复 | 真实最小化后OS minimized=true且窗口对象visible=true；通过现有maximize按钮从最小化恢复，再还原，未声称物理点击任务栏 |
| 空白双击处理 | CDP发送mousedown detail=2，实际原生状态变为最大化；浏览器边界检查另证明单次命令，未声称物理双击 |
| 菜单、侧栏、历史、键盘与滚动 | 侧栏两次切换恢复；File箭头与Escape；Help真实version/help IPC及runtime_info；后退/前进、Edit正文全选；滚动后顶部仍y0/高36px，无剪贴板改动 |
| 默认关闭 | 真实关闭按钮使已安装窗口hidden而原进程存活 |
| 托盘恢复及拖动 | 桌面自动工具连接缺少命名管道；用户在当前安装版手动恢复并拖动，直接回答“能恢复并正常拖动”。[用户确认](assets/B04-T03-ui-native-user-confirmation.json)与随后同一原生窗口visible/normal/band0记录相符；未补造位置移动量 |
| 直接退出与持久化 | 实际设置为exit，离开再重新读取；实际关闭按钮使关闭前已绑定的原生进程exit0。[退出收据](assets/B04-T03-ui-native-exit-process.json)与[设置操作](assets/B04-T03-ui-native-exit-settings.json)保留真实结果 |
| 重启及恢复偏好 | 重新启动已安装程序后实际读到exit，再用原设置UI保存原值hide_to_tray。[恢复收据](assets/B04-T03-ui-native-preference-restored.json)只承诺行为值恢复 |
| 最终普通启动 | 停止本轮自己的调试进程后正常启动，debug_port/address及isolated folder均null；9243端口已关闭，窗口可见/非最小化/非最大化、title band0。[启动](assets/B04-T03-ui-native-normal-launch.json)与[原生窗口](assets/B04-T03-ui-native-normal-window.json)保留实际结果 |

第一组原生脚本有11项实际通过，最后尝试通过maximize显示托盘隐藏窗口的假设失败，保留[完整途中结果](assets/B04-T03-ui-native-actions-01.json)。这不等于托盘show失败；用户随后通过真实托盘恢复。后续直接退出/偏好验证再有5项实际通过，根证据02记16项自动结果及两项人工实机确认。没有把失败脚本整体改写为passed。

进程观察驱动第一次因CRLF未识别ready而没发送close，等待超时；失败及原驱动保留。修正测试驱动后真实exit0，未改产品。两个结束的调试进程stderr各有7条libpng iCCP profile警告，图标资源本项没有修改；这些警告未计作窗口操作失败，也未声称日志完全无警告。

完整Rust532、前端10项及计数修复后的最终Python108项均已实际通过，详见[B04-T03主验证](B04-T03.md)。整分支唯一Important是独立Python数量真实性，最小修复及新鲜限定复审已通过，未改动当前已安装的单行窗口；状态读取提示Minor继续延期。日志及远端同步以实际工具记录为准。本记录的原生证据来自当前实际安装及用户直接确认。
