# B03-T01 严格单请求 sidecar 实施计划

日期：2026-11-01。仅执行当前任务 B03-T01；依赖 B02-T03 已验收。此文件固定实现与验证设计，下面的默认值和预期断言不代表测试已运行。真实结果另写 validation。

## 范围与裁定

这是新的进程边界，按架构任务先固定契约、输入能力、生命周期和验收，再实现。依据 [近期步骤](NEXT_STEPS.md)、[PRD](../requirements/PRD.md)、[架构](../technical/ARCHITECTURE.md)及[测试标准](../standards/TESTING.md)。本轮仅验证 `input_bytes` 定位的 fake 协议，不实现 PDF 坐标、解析库、网络、模型、项目登记或 GUI。

Ruling：沿已采纳任务卡继续设计及实现，不重新请求常规开发授权。用户已要求连续开发、自动工具、小步提交和检查后推送。一个任务内部的 DTO、平台适配和测试由子代理分工，不启动第二项。

Ruling：在托管工作区的任务分支开发，保留原 D 盘 checkout 的两项用户删除。Cargo 缓存复用原 target；只对新工作区自有 target、桌面 gen 和 .artifacts 生成目录设置 Low 标签，以允许 Low 标签构建及测试程序写入。初次编译访问拒绝、随后测试目录访问拒绝以及修正后的 247 项基线通过保留日志；这些是工作区设置，不修改产品目录授权。

Ruling：CLI 与 desktop 继续 `unsafe_code=forbid`。新小型 `noetome-sidecar-platform` crate 单独封装 Windows FFI，crate 默认 deny unsafe，只有私有 Windows 实现允许，并要求逐个说明句柄、缓冲区和生命周期。使用已缓存锁定 windows-sys 0.61.2，未引入 PDF 或外部运行时。

## 接口和协议

`sidecar::run_sidecar(&AdapterSpec, &Path, &Path, SidecarLimits, &AtomicBool)` 返回完整 `VerifiedResponse` 或稳定分类 `SidecarError`。可信 Host 配置原生 program/argv 和预期 AdapterIdentity；资料和响应不能决定执行程序、参数、根目录或预算。普通 CLI 无 sidecar/fake 命令，项目来源仍只接受既有 MD/TXT 契约。

Host 完整有界读取明确选定的单文件，保持读守卫，计算 SHA256；仅在尚不存在的新根独占创建 `.owner.json`、`request.json`、`input/raw.bin` 和空 `artifacts/`。传给子进程的路径是固定相对快照和资源目录，cwd 为这个根；不传原材料路径。保留目录及文件身份，最终验证固定树和全部资源，失败保留现场，不自动递归删除或接管。

版本固定 `schema_version=1`、`protocol=noetome-parser-single-v1`、`operation=inspect_bytes`。请求字段为 request_id、input 的 relative_path/byte_length/sha256、workspace 的 artifact_directory、artifact_policy 的单文件/总字节/数量上限。request_id 是 Host 生成的 UUID v4。

响应固定 request_id、相同输入指纹、adapter 的 producer/id/version/capabilities、status、显式可 null unit_count、units、artifacts、issues。所有字段必填；未知字段、枚举、重复键、错误版本、非法 UTF8、BOM 和额外非空白 stdout 均拒绝。stdout 只允许唯一 JSON，stderr 为有界诊断。

单元使用 input_bytes 半开区间和 0 起 ordinal；完整成功必须覆盖全部原始字节，无重叠、漏号、重复或重排。parsed 单元引用实际资源范围且字节等于所指原始范围。资源仅允许 `artifacts/{artifact_id}.bin` 或 `.txt`，对应 binary/utf8_text；UTF8 范围必须在字符边界。fake 始终 producer=fake、validation_scope=protocol_only，不能作为实际 PDF/知识验收。partial/failed 的合法报告保留于错误，不能作为完整成功。

## 有界宿主与 Windows

| 政策 | 默认值 |
| --- | --- |
| 原始输入 | 64 MiB |
| 请求 / stdout / stderr | 64 KiB / 4 MiB / 256 KiB |
| 单资源 / 总资源 | 16 MiB / 64 MiB |
| 单元 / 资源 / 问题数 | 4096 / 64 / 128 |
| 运行 / 清理时限 | 30 s / 2 s |

这些是可收紧的协议政策，不是吞吐或容量基准。线程并发写 stdin、读 stdout 和 stderr；关闭 stdin 标记单请求结束。累计字节超限立即分类失败，停止使用输出、终止受控 Job 并等待，不能静默截断后解析。

使用 Windows 10+ CreateProcessW 的 CREATE_SUSPENDED 和 STARTUPINFOEX：HANDLE_LIST 只列三个子进程管道端，JOB_LIST 在创建时原子加入自有 Job，随后 ResumeThread。Job 配置 KILL_ON_JOB_CLOSE，不允许 breakaway。相比启动后再 Assign 的竞态，这保证子进程执行之前已受控；无支持时显式失败，不降级为只 kill 根进程。核查 Microsoft [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)、[进程属性](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)和[Job 记账](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_accounting_information)。

成功退出、超时、取消、超限和错误均处理根及后代。根退出后保存退出信息并释放根句柄，再等 Job ActiveProcesses=0；根早退而后代持有管道不能被当作完整成功。独立测试观察者并发 wait 子/孙进程且及时释放句柄，避免观察者自身阻止 Job 记账收敛。等待或管道清理失败返回 CLEANUP_FAILED，不能声称清理成功。此控制不等于 OS 文件/网络沙盒。

## 分工与实现顺序

1. Root 写这份计划、公共 Host 类型及编译用最小接口，接线 Cargo。模型代理拥有 sidecar_model.rs，先运行严格解码、身份、完整覆盖、范围和数量负例红，再实现。模型 nullable 字段用 required deserializer，不能采用 serde 普通 Option 的缺省接受行为。
2. 进程代理拥有平台 crate 和 sidecar_process.rs，先真实子进程/管道行为红，再实现安全 FFI 封装及并发生命周期。包含真正超过管道容量的 blocked-stdin 传输测试；不增加正式 DTO padding。
3. 测试代理拥有 harness=false 的 tests/sidecar.rs 和私有 tests/support/sidecar_fake.rs。current_exe 的显式测试 argv 进入 fake，不创建生产 bin，不剥除 libtest stdout 横幅。先以实际有效响应、spawn failure 和协议负例确认红，再扩展验收矩阵。
4. Root 拥有 sidecar.rs：输入快照、独占根、文件/目录守卫、精确树、资源 SHA/长度/硬链接和身份验证，结合模型与进程结果。对外哨兵保持原字节，冲突现场保留。平台提供安全 file_link_count，避免把硬链接资料当自有新资源。
5. 定向真实进程测试覆盖启动失败、空/崩溃/非零有效 JSON、非法 UTF8/JSON、截断/额外 stdout、版本/ID/SHA错误、重复/漏单元。限额验证精确边界和 +1；真实子/孙握手后测 timeout/cancel/超限/根早退，核对退出而非仅 PID 名称。资源测 abs/../保留名/尾点/大小写碰撞/链接/junction/硬链接/未知文件/既有根。
6. 审查后修复，再完成 fmt、locked/offline workspace check/test、Python unittest、devlog check、diff check、相关链接。保存真红/绿命令、实际数量、Windows 环境及明确未测范围；用 devlog complete 追加证据。按契约/宿主/验收拆 commit，push 当前分支并核对远端 HEAD。当前任务验收后停止，不 start B03-T02。

## 预期稳定分类

INPUT_INVALID、INPUT_LIMIT、UNSAFE_PATH、WORKSPACE_CONFLICT、REQUEST_LIMIT、SPAWN_FAILED、PIPE_FAILED、STDOUT_LIMIT、STDERR_LIMIT、TIMEOUT、CANCELLED、EXIT_NONZERO、CLEANUP_FAILED、PROTOCOL_INVALID、RESOURCE_INVALID、INCOMPLETE 均加 `SIDECAR_` 前缀。校验问题另有 rule_id/object_refs/message；只附受限诊断，不保存无关材料。实现或真实试验发现冲突时记录裁定并更新本文件，不隐藏失败。
