# B01A-T01 关闭行为设置修正版

2026年11月1日，Windows x64。用户确认：×默认隐藏到托盘，可保存为直接退出；最小化按钮保留任务栏窗口；托盘菜单“退出”始终终止程序。此修订替代[首次托盘版](B01A-T01.md)的关闭/最小化语义，历史不删除。

本文件保留关闭设置第一修正版的构建与当时未验收记录。用户随后报告重启后行为恢复默认；最新修复、安装指纹和实际结果见[设置重读与缓存同步验证](B01A-T01-settings-refresh.md)。下面的旧指纹不代表最新交付。

## 当前实现

- `main.rs` 只在主窗口CloseRequested按偏好拦截关闭；默认隐藏且保留窗口/进程，直接退出时不拦截。Resized不再隐藏窗口，保留Windows常规最小化行为。托盘恢复使用原窗口，托盘退出直接app.exit(0)。
- `preferences.rs` 在固定应用本地目录保存 `desktop-preferences.json`，Schema 1、close_behavior仅允许hide_to_tray/exit，上限4096字节。缺失时读取默认而不写文件；损坏、未知字段/版本、路径冲突和外部修改明确拒绝覆盖，关闭安全回退托盘。
- 保存使用独占暂存、fsync、重查原文件字节和身份，再同目录替换；不先删除旧文件。首次发布不覆盖已出现文件。实际保存成功后才更新内存值，失败不会伪称成功或改变当前偏好。
- 关于页新增“窗口行为”，从实际Rust命令读取/保存；有独立未读取、草稿、正在保存、已保存和错误状态，不使用localStorage。只有成功返回才显示已保存。

这只是窗口偏好，不支持后台任务、开机启动、完整项目GUI或生产Vault写入。实现提交10362b3、e7206c3；任务范围按用户反馈更新并通过日志工具reopen/start。

## 实际检查

| 检查 | 结果 |
| --- | --- |
| cargo fmt --all --check | 通过 |
| cargo_local check --workspace --locked --offline | 通过 |
| cargo_local test --workspace --locked --offline | 32项通过、0失败0忽略；存储5、项目CLI11、既有CLI4、桌面12 |
| 设置8项文件行为测试 | 默认无写入、保存/重读、Windows已有文件替换、真实拒绝删除共享导致保存失败、非法输入/损坏/未来版本、外部编辑删除/同字节替换、文件/目录冲突、junction哨兵保护通过 |
| npm run desktop:build -- --ci -- --locked --offline | 前端类型、生产构建、Windows release及NSIS通过 |
| Python日志工具 | 12项通过；devlog check通过 |
| git diff --check | 通过 |

通过本地in-app browser实际查看关于页：预览没有Rust运行时，显示DESKTOP_SETTINGS_UNAVAILABLE、未读取和禁用选择器；重新读取仍明确失败，没有显示假默认或假保存。设置区可滚动查看且控件/错误没有重叠，截图保存于`.artifacts/desktop-settings-preview.png`。这不是安装版设置成功或原生窗口验收。

## 修正版安装

已结束仍驻留托盘的旧安装版进程，以避免占用安装文件；使用既有当前用户安装脚本更新。安装器退出0，当前构建/包未变，快捷方式及卸载登记匹配，正常输出包默认Medium。机器证据见[修正版安装结果](B01A-T01-close-settings-install.json)。

| 产物 | SHA256 |
| --- | --- |
| 当前release（UNK） | 87591c5875df2f7caccc955c1e794d0029dd4395cb08f7300679dd0f749bd3b3 |
| 安装后的程序（NSS） | 2f6db2b00c37e1d4157409647c67f6032be2ada952f909e187207f7ab308b5ca |
| NSIS安装包/正常输出副本 | 61e1590f94d48975db25dea37ae8114ec2f7c0864b3d5ffdaa665d25ee27fe82 |

程序安装在LocalAppData/Programs/Noetome；正常包在NoetomeBuilds/windows/61e1590f94d4。执行文件严格匹配当前构建的唯一Tauri NSIS标记变体，不把两个不同SHA说成字节相同。

## 原生操作待确认

本轮桌面启动工具返回GetCursorPos拒绝访问(0x80070005)。刷新后未见已启动应用，因此用正常程序生命周期命令启动已核验的安装文件，PID97548；再次读取桌面窗口清单得到Noetome窗口34421010，进程保持运行。

窗口初次返回Accessibility state unavailable；复核截图又报CreateForMonitor无法捕获显示器(0x80070057)。依照[computer-use技能](C:/Users/Lyinn/.codex/plugins/cache/openai-bundled/computer-use/26.1027.30824/skills/computer-use/SKILL.md)引用的[恢复指引](C:/Users/Lyinn/.codex/plugins/cache/openai-bundled/computer-use/26.1027.30824/docs/guidance.md)，没有使用旧坐标猜测点击，也没有通过其他技术模拟输入。当前只有程序/窗口存在证据，不能证明托盘和设置操作通过。

已请求用户确认：常规最小化任务栏；默认×隐藏与两种托盘恢复/状态保留；保存直接退出后×终止且重启持久；改回隐藏生效；托盘退出始终结束。用户此前的回复是需求澄清，不是这些修正版行为通过的反馈。收到实际结果前，任务保持未完成，不推送托盘分支。

没有跨平台、任意恶意并发编辑、所有断电时点或全部Explorer重启测试；严格文件/偏好测试不替代GUI验收。
