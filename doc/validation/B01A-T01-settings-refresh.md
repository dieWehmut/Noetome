# B01A-T01 设置重读、缓存同步与最终安装验证

日期：2026年11月1日，Windows x64。续接聊天“继续项目开发”的最后反馈：保存直接退出后重启，×行为恢复默认。沿用同一任务，保留[第一修正版记录](B01A-T01-close-settings.md)和[首次托盘记录](B01A-T01.md)。

最终结果：同日用户已完成最终安装版的任务栏/托盘四项检查，以及正常Windows快捷方式启动下的“保存直接退出 → ×退出 → 快捷方式重启 → 仍为直接退出”检查，均回复全部通过。B01A-T01验收完成；下面保留此前失败、等待和本轮环境诊断，不用后来的通过覆盖历史。

## 实际定位

当时命令进程看到的逻辑设置路径为LocalAppData/com.noetome.desktop/desktop-preferences.json，54字节合法Schema 1，close_behavior为exit。原安装版PID136520启动于11:39:42；用户恢复该窗口后回复“默认值”。执行者实际读取原生可访问树，选择器为“隐藏到托盘（默认）”，状态为“当前设置：隐藏到托盘”，同刻命令读取该路径仍为exit。

另启动相同已核验安装文件得到PID29576，关于页实际读到“直接退出”；点击×后该PID确实退出，原PID136520仍运行。该次观察确认两个实例显示和关闭行为不同，源码缓存问题随后由回归重现。现场磁盘比对使用的是命令进程所见的逻辑LocalAppData路径；后续发现的包内缓存视图见下文，不能仅凭该次文件读取证明原实例实际打开的物理文件。没有证据确定原实例首次缓存默认值的具体外部时序，不能把推测的瞬时共享失败记作现场根因。

源码中查询只返回启动缓存，关于页“重新读取”也不会重新读磁盘。首次加载失败的Err永久缓存；effective安全回退隐藏。因此旧值和一次读取失败都可能继续影响关闭行为。

## 修复与回归

实现提交e95a1b6。PreferencesState保留实际目录，新增reload真读受限文件。读取成功后更新运行状态；失败返回实际错误并保留此前成功值、文件快照和未知磁盘内容。设置查询及主窗口×决策前都重新读取；启动读取失败明确弹窗/日志，并说明安全回退和重读入口。没有增加任意路径或shell命令。

三项真实回归先观察断言失败，再恢复实现观察通过：

| 回归 | 红阶段 | 绿阶段 |
| --- | --- | --- |
| Windows独占读共享阻止首次加载，解除后重读 | 仍返回缓存os32错误 | 重读exit并更新effective，不改文件 |
| 有效exit后磁盘被损坏 | 仍假报Ok exit为新读取成功 | 返回INVALID_FILE，保留旧effective和未知字节，保存拒绝覆盖 |
| 旧state默认，独立state先后保存exit/hide | 旧reload仍为hide，与exit期望不符 | 两次均同步新值，查询字节及目录内容不变 |

实际执行cargo fmt --all --check、cargo_local check/test --workspace --locked --offline；35项Rust测试通过、0失败0忽略（存储5、项目CLI11、既有CLI4、桌面15）。Python 3.12工作区运行时执行12项日志unittest通过；devlog check与git diff --check通过。独立只读审查未发现本次增量阻塞问题。

## 最终构建与安装

最后一次npm run desktop:build -- --ci -- --locked --offline通过，包含严格前端类型检查、生产构建、Windows release及NSIS。停止已核验旧安装版驻留进程后运行install_desktop.ps1，退出0，版本登记与两个快捷方式目标匹配。机器结果见[最终安装证据](B01A-T01-settings-refresh-install.json)。

| 产物 | SHA256 |
| --- | --- |
| 当前release程序（UNK） | ff3fa57cb0d2eee8f0fff24ea793c019b86584d15cbb0ec4d63ab136ad43a1b8 |
| 安装后程序（NSS） | 0a90adab2f6f6c32d9a863db7a254970998943ebcff62fdfa871ea862b1b42d2 |
| NSIS包与正常用户目录副本 | 7c564e308d330fea20bdc427504b1ff53d98ca99986b5f1efdbe76ec87562240 |

安装位置为LocalAppData/Programs/Noetome，正常交付副本在NoetomeBuilds/windows/7c564e308d33。安装程序与当前构建的唯一Tauri NSIS标记变体匹配；不将两个不同SHA称为完全相同字节。

## 初次原生验收与等待历史

最终安装版PID130400打开成功，关于页读到磁盘exit。实际使用原生选择器及Up/Return键选择hide_to_tray：草稿明确显示尚未保存；点击保存后文件变为hide_to_tray，界面显示“已保存：隐藏到托盘”。选择器set_value未改变选择，因此不把该调用记为成功。

安装版实际检查Rust连接正常，桌面/CLI均0.1.0，目标x86_64-pc-windows-msvc；帮助执行成功。随后点击最小化，原窗口14556910仍可在窗口清单识别，PID仍存活。任务栏实际恢复、×隐藏后的左键/菜单恢复与输出保留、托盘退出和快捷方式重启已请求用户操作；尚待反馈，不把窗口存在当作托盘验收完成。

首次读取Accessibility后click返回coordinate input geometry is unavailable；按Computer Use恢复规则重选并刷新一次后恢复输入。一次窗口未前置的截图包含其他应用，本轮不保存或提交该截图。此后前置窗口的截图与控件正常。所有原生输入使用Computer Use，未用其他技术模拟点击。

上轮仍等待完整托盘反馈和最终设置退出/重启流程。已发出可直接操作的四项检查；当时收尾按真实外部验收依赖记录blocked，不记done、不切B02，也不推送本分支。该阻塞与本轮reopen/start历史均保留在账本。

上轮交接时最终安装中的偏好为hide_to_tray，窗口已常规最小化。当时的阻塞表示原生任务栏/托盘操作反馈尚未收到，不代表代码回归失败；程序仍可通过任务栏或托盘使用。指定sample/icon.png SHA为ce6e3cd7d822d26a71dae9d641e6c409e690e7269eb0f9d275df20f326f729f7，保持不变且仍被Git忽略。上轮九份修改文档55个本地链接核对通过。

## 本轮正常用户验收完成

用户首先回复“全部通过”，覆盖以下四项真实操作。桌面自动化没有可靠选取任务栏/托盘控件的接口，因此这些结果明确归属于用户原生操作反馈，而非自动化模拟。

| 操作 | 用户反馈 |
| --- | --- |
| 最小化后任务栏仍有Noetome，从任务栏恢复 | 通过 |
| 运行检查页点击×，左键托盘恢复，帮助输出保留 | 通过 |
| 再点击×，托盘“显示窗口”恢复，输出保留 | 通过 |
| 托盘“退出”后图标/进程消失，从快捷方式重启 | 通过 |

执行者在安装版PID134448中用实际原生选择器及键盘保存exit，界面显示“已保存：直接退出”；离开关于页再返回，真正重读显示“当前设置：直接退出”。点击×后窗口消失，随后实际进程查询为0。另一次由工具直接启动的PID64224显示hide，命令读取的文件也仍为hide；该混合启动上下文结果保留为诊断记录，不称持久化已经失败或已经通过。

只读文件句柄诊断确认：命令进程打开逻辑LocalAppData/com.noetome.desktop/desktop-preferences.json时，GetFinalPathNameByHandle返回Codex包内LocalCache/Local/com.noetome.desktop/desktop-preferences.json。Codex MSIX清单存在文件系统虚拟化配置，Noetome目录不在LocalAppData/OpenAI排除项内。Windows构建为26200.8037。本轮未改包清单、目录权限或系统设置，也未把包身份查询返回无包身份当作没有虚拟化的证据。微软说明了[新文件虚拟化清单项的优先规则](https://learn.microsoft.com/en-us/uwp/schemas/appxpackage/uapmanifestschema/element-virtualization-filesystemwritevirtualization)及[AppData包内视图的读写行为](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-behind-the-scenes#appdata-operations-on-windows-10-version-1903-and-later)。关于不同启动通道是否继承同一视图，属于环境推断；未取得应用实际文件句柄时不宣称每个实例的物理文件位置已证实。

为排除混用测试上下文，最后请用户直接在正常Windows中使用同一桌面/开始菜单快捷方式完成：关于页保存“直接退出”→点击×结束程序→点击快捷方式重启→仍显示“直接退出”且×仍结束程序。用户最终回复“没问题，全部通过”。该结果是正常用户启动下的设置持久性和直接退出验收；按请求最后重启的窗口留着。无需再次混用工具启动来覆盖这份反馈。

本轮重新核对安装后程序SHA仍为0a90adab2f6f6c32d9a863db7a254970998943ebcff62fdfa871ea862b1b42d2，与最终安装证据匹配；没有改产品代码或重新替换已验收安装包。复跑Rust fmt、locked/offline check/test，35项Rust测试0失败0忽略；12项Python日志测试和前端严格类型/生产构建通过。收尾文档及账本检查另在完成事件中记录。下一批仅规划B02三项，未启动产品实现。

## 范围与限制

只验收本机Windows当前用户安装。未运行跨平台、任意断电、全部Explorer重启、多显示器或恶意并发编辑测试。独立state测试为顺序保存与重读，不证明两个进程同时保存的一致性；原有最后快照校验与替换之间仍存在并发时序窗口。保存冲突后可离开关于页再返回以重新读值，或按错误提示重启。

没有引入项目GUI、生产Vault写入、PDF/OCR、模型或持久后台任务；B02尚未启动。
