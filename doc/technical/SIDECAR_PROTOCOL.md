# 单请求 Sidecar 协议 v1

实现入口为 `noetome_cli::sidecar::run_sidecar`，严格 DTO 和纯校验在 sidecar_model，进程生命周期在 sidecar_process，Win32 调用只在 noetome-sidecar-platform 的私有 Windows 模块。当前契约验证原始字节分区及资源关系；`input_bytes` 不是 PDF 页或正文定位。实际验证结果与未测范围另见 [B03-T01 验证记录](../validation/B03-T01.md)。

## Host 的输入能力

可信调用者提供绝对普通可执行文件、原生 argv、预期 AdapterIdentity、明确选取的一个文件和尚不存在的新工作区。材料、请求和响应均不能决定执行程序、根目录或预算。Host 生成 UUID v4 request_id，完整有界读取所选文件，保持 Windows 读守卫，计算原始字节长度和 SHA256，只把自己的快照交给子进程。

工作区父目录必须存在且不含链接/reparse/junction。既有目标即使为空也拒绝，不清空、不接管。所选文件和工作区不能互为祖先；在明确指定的相邻新目录输出合法。路径最多 1024 个 UTF16 code unit；不含原始 `..`、Windows 保留名、尾随点/空格、非法组件或不安全卷前缀。

固定布局如下，失败默认保留现场：

```text
<new_workspace>/
  .owner.json         {schema_version:1, request_id}
  request.json        发送的完整 JSON 字节
  input/raw.bin       完整原始快照
  artifacts/          新建、初始为空的资源目录
```

cwd 是该根，输入能力只用固定相对路径，不把原选取文件路径或用户目录传给 sidecar。Host 保持目录身份及文件句柄，子进程继承列表只含自己的 stdin/stdout/stderr 管道端，不含输入、工作区或 Job 句柄。Host 的快照句柄允许子进程读取，阻止其他打开方式写入或删除该文件。

## 严格结构

所有对象只接受带名称字段的 JSON 对象，拒绝按字段顺序排列的数组；所有枚举只接受字符串，拒绝外部标签对象。对象拒绝未知字段和重复键；所有字段必填，包括允许显式 null 的字段。对象检查直接保留原始 MapAccess，不经会丢重复键的 Value 转换。使用 UTF8，不接受 BOM、lossy 替换、浮点/负数冒充整数、未知枚举或版本。stdout 必须恰好一份 JSON，允许 JSON 空白；不接受其他前缀、后缀或第二份对象。

公共约定：schema_version 恰为 1，protocol 恰为 `noetome-parser-single-v1`；SHA 是 64 位小写十六进制。对象 ID 为 1..128 字节 ASCII `[A-Za-z0-9][A-Za-z0-9_-]*`，同类 ID 和资源路径拒绝大小写碰撞。adapter_version 是 1..128 字节非空白 ASCII 打印字符串，不含控制字符。长度/范围为 u64，序号/数量为 u32，所有外部范围检查转换和实际字节边界。

| 请求对象 | 必填字段 |
| --- | --- |
| SidecarRequest | schema_version, protocol, request_id, operation, input, workspace, artifact_policy |
| operation | 固定 `inspect_bytes` |
| input | relative_path=`input/raw.bin`, byte_length, sha256 |
| workspace | artifact_directory=`artifacts` |
| artifact_policy | max_file_bytes, max_total_bytes, max_files；与 Host 实際政策一致 |

| 响应对象 | 必填字段 |
| --- | --- |
| SidecarResponse | schema_version, protocol, request_id, adapter, input, status, unit_count, units, artifacts, issues |
| adapter | producer=`fake` 或 `parser`, adapter_id, adapter_version, capabilities |
| capabilities | locator_kinds 必须为 `[input_bytes]`；artifact_kinds 为非空且无重复的 binary/utf8_text 子集；与可信 Host 预期相同 |
| input | 三字段必须逐项等于请求及实际完整输入 |
| status | complete / partial / failed |
| unit_count | 显式整数或 null；只允许未知库存的 failed 响应以 null 配空 units/artifacts |
| Unit | unit_id, ordinal, locator, status, content |
| locator | kind=`input_bytes`, start_byte, end_byte |
| Unit.status | parsed / failed / needs_review |
| content | 显式 null 或 artifact_id, start_byte, end_byte |
| Artifact | artifact_id, kind=binary/utf8_text, relative_path, byte_length, sha256 |
| Issue | code, message, unit_ids；message 为 1..4096 UTF8 字节，允许 LF/Tab，拒绝其他控制字符 |

完整清单 ordinal 从 0 连续排列，原始字节范围为非空半开区间，按清单顺序无重复、漏号、重叠或间隙地覆盖整个输入。空输入的完整清单可以为零。parsed 单元必须引用实际资源字节；资源范围与原始范围逐字节一致，utf8_text 另外检查合法 UTF8 和字符边界。失败/需复核单元必须有可定位问题，问题引用不能越出本轮清单。

资源只允许 `artifacts/{artifact_id}.bin` 或 `.txt`，分别对应 binary/utf8_text。Host 拒绝未知文件、目录、孤立资源、绝对路径、父级跳转、ADS、保留名、尾点、碰撞、reparse/junction 和硬链接。先检查实际长度、数量和总额度，再有界读取；独立计算 SHA，保留文件身份。读取后、返回前再次复查资源身份、长度/SHA、链接数及整个资源目录精确清单。校验结果描述本轮检查事实，不承诺返回后任意外部编辑仍保持有效。

## 完成与失败

成功必须具备成功原生退出码、全部管道完成、Job 无活动进程、身份/能力/完整清单/实际资源都通过，且 status=complete。调用结果保留 producer 和 validation_scope=protocol_only。

合法 partial/failed 保留已确认库存和结构化问题，以 `SIDECAR_INCOMPLETE` 返回 error.response；它不算完整解析成功。只有非 Complete 状态可豁免纯校验的单一成功阻断规则 `SIDECAR_REPORTED_FAILURE`，其他协议违规全部继续拒绝。最终资源检查失败时不附可被误用为已验证的 response。普通 CLI 没有 fake 或 sidecar 执行开关，项目登记仍使用既有 MD/TXT 契约。

主要公开分类为 SIDECAR_INPUT_INVALID、INPUT_LIMIT、UNSAFE_PATH、WORKSPACE_CONFLICT、LIMITS_INVALID、REQUEST_LIMIT、SPAWN_FAILED、PIPE_FAILED、STDOUT_LIMIT、STDERR_LIMIT、TIMEOUT、CANCELLED、EXIT_NONZERO、CLEANUP_FAILED、PROTOCOL_INVALID、RESOURCE_INVALID、INCOMPLETE、UNSUPPORTED_PLATFORM（后续各项均加 SIDECAR_ 前缀）。SidecarError 包含稳定 code、message、受限 diagnostics 原字节、可获得的 pid、规则问题，只有合法不完整结果包含 response。严格 JSON 错误只报告类别与行列，不把未知字段中的大段原文拷贝到诊断。

## 限额与进程生命周期

| 政策 | 默认值 |
| --- | --- |
| 输入 | 64 MiB |
| 请求 / stdout / stderr | 64 KiB / 4 MiB / 256 KiB |
| 单资源 / 总资源 | 16 MiB / 64 MiB |
| 单元 / 资源 / 问题数 | 4096 / 64 / 128 |
| 运行 / 清理 | 30 s / 2 s |

公开 Host 只允许收紧默认上限，清理期限必须大于零；测试私有传输层可以用更大缓冲验证真正阻塞的 stdin。默认值是政策，不是性能指标。stdin writer、stdout reader、stderr reader 并行，stdin 完整写入后关闭。超限停止使用响应，以明确错误返回；受限失败诊断可保留已读前缀，不把它称为完整 stdout 或成功结果。

所选输入、快照复制、实际资源及最终复核按 64 KiB 分块检查取消，验证阶段和最后交付入口也检查。同一取消标志在子进程退出后仍生效，取消不能被资源错误包装丢失，也不能附成功响应。CPU 校验及同步 OS 调用不承诺可被任意时刻强制抢占。

当前执行环境只传真实 Windows 中已存在的 SystemRoot，不继承 TERM、Cargo 或其他任意 Host 配置，不硬编码系统路径。真实对照中 TERM 使私有 libtest 子程序用户入口延迟约 10.82 s；同步创建仍约 3.44 ms，移除 TERM 后整次约 23.55 ms。该触发条件已经隔离，底层等待机理未解释；后续实际 Python 需要的环境另行明确和验证。此修正没有扩大任何超时政策。

Windows 10+ 使用 CREATE_SUSPENDED 和 STARTUPINFOEX。HANDLE_LIST 只含三个子管道端；JOB_LIST 在 CreateProcess 时原子关联不允许 breakaway 的自有 Job，然后检查成员身份、恢复确切主线程。Job 配置 KILL_ON_JOB_CLOSE，不支持该模式时失败，不先运行再补绑定。参见 Microsoft [进程属性](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)及[Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)。

根进程退出不等于整树完成。缓存根退出信息、保守关闭根引用后检查 ActiveProcesses=0；普通 EOF 仍不能放过无管道的后代。超时、取消、输出超限、非零退出和其他失败终止 Job，并有界等待根、后代及全部管道线程，原生准备失败也使用调用者收紧的清理期限。使用 CancelSynchronousIo 解除阻塞线程，真正 join 完成才返回原始失败分类；不能确认清理时返回 CLEANUP_FAILED，关闭 Job 是最后兜底，不伪造清理成功。独立观察者退出后及时释放进程引用；当前真实准备失败探针表明，query/synchronize 观察句柄仍存活时计数也能归零，不能宣称所有观察句柄必然阻止归零。参见[Job 记账说明](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_accounting_information)。

CLI 和 desktop 保留 unsafe_code=forbid；唯一小型平台 crate 默认 deny，只在私有 Windows 模块以明确安全注释封装 FFI。它提供受控 spawn/终止/等待、链接数和安全进程观察，不提供从材料执行任意命令的接口。

## 后续版本

真实 PDF 页/区域和提取文本不是原始字节分区。B03-T02 复用已验证的有界宿主和资源保护，在真实解析选型后显式定义支持 PDF 的协议版本、页清单、坐标和文本资源范围，并重跑契约验证；不得扩大 v1 的含义或把 PDF 二进制 offset 当正文 locator。来源项目 Schema2 和独立编译 Schema1 本任务均未变。
