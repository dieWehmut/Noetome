# B04-T02 本地准备的实际验证

本记录只验收完整短文本预检和瞬时凭据交接。**B04-T02 未完成真实 API 验收**：尚缺用户配置的提供商/API 地址、模型、凭据环境变量名称、资料外发范围及单次预算。没有读取真实凭据或执行真实 API 请求，没有费用。唯一真实适配器、配置/授权绑定、预算准入、远端调用事实和严格持久重载仍须实现；没有推进 T03。

实现提交为 `470262b6a7747e43bcee9b74158c7c93d5e975f2`，基线为 `99108767c09bfea955096727f33cd01594c1b489`。接口和能力限制见[本地边界](../technical/API_PREFLIGHT_BOUNDARY.md)，剩余任务门槛见[执行卡](../execution/NEXT_STEPS.md#b04-t02-单次小文本真实api候选抽取)。机器记录见[B04-T02-local.json](B04-T02-local.json)，原始证据共 **279 文件、1,807,580 字节**，包含15份实现/标准文件的工作区与Git提交两种SHA。

## 已实现的行为

`knowledge preflight <项目路径> <version_id>` 对真实登记的一份MD/TXT重建完整规范文本，保留Formatting、空行和末尾字节。它持续持有配置、数据库、raw/normalized及祖先守卫，再次核对身份、登记、实际字节与全部Span；输出仅含源摘要、`api_status="not_called"`、`externalization_authorized=false`。原文件移走仍可读取登记快照，未知版本没有来源回退。项目整体一致性检查会复核同项目其他已登记快照；不会扫描未登记资料。

实际8192字节限额作用于完整规范UTF8，raw沿用来源登记的64MiB上限。BOM/CRLF造成 **12290字节raw → 8192字节normalized** 时完整接受；normalized8193整体拒绝，不截断。完整输入视图与原T01的非Formatting证据子集分别保留，预检成功不代表真实适配器或完整请求已通过准入。

`TransientCredential`是不透明的1..4096字节UTF8值，通过固定stdin帧交接给一个本地有界进程。原请求指纹不包含密钥，环境仍只有SystemRoot；stdout只有通过原始严格解码及实际凭据的原始/解码键值扫描后才可返回正文和SHA，所有stderr以及敏感失败stdout均不返回正文、长度、SHA或前缀。合法但坏证据/Token超限的干净响应保留可审计用量，仍须上层拒绝候选。`ExtractionOptions`的Debug也已隐藏哨兵值。

凭据对象及指定owned writer缓冲的fill仅为best effort。reader、serde临时值、调用者及OS副本没有全面清除保证；没有做内存dump或优化器防护测试。此次入口没有连接旧v1 CandidateReport，PID也不能证明远端API请求次数。

## 实际执行的检查

本机Windows x64，Rust/Cargo1.98.1，选定工作区Python3.12.14；真实不同的PDF配置负例使用Python3.12.7。所有Rust检查明确使用 `CARGO_TARGET_DIR=D:/Noetome/target`，未运行工作区内过期的target。普通检查全部离线。

| 检查 | 实际结果 | 原始记录 |
| --- | --- | --- |
| `cargo fmt --all --check` | exit0 | [静态退出记录](B04-T02-local-evidence/static-checks.json) |
| `cargo_local.ps1 check --workspace --locked --offline` | exit0 | [check](B04-T02-local-evidence/logs/b04-t02-check.log) |
| `cargo_local.ps1 test --workspace --locked --offline` | **468通过、0失败、0忽略** | [完整workspace](B04-T02-local-evidence/logs/b04-t02-workspace.log) |
| Python `unittest discover -s tests -p "test_*.py" -v` | **12通过、0失败** | [Python](B04-T02-local-evidence/logs/b04-t02-python.log) |
| `scripts/devlog.py check`、`git diff --check` | exit0；日志与差异检查 | [devlog](B04-T02-local-evidence/logs/b04-t02-devlog.log)、[差异](B04-T02-local-evidence/logs/b04-t02-final-diffcheck.log) |

468项由302项CLI库单元、4项CLI基础、12项compile、6组凭据、20项fixture、15组T01模型、3项预检CLI、10组PDF parse、12组PDF来源、13项project、25组sidecar、19项text source、15项桌面和12项platform组成。新增测试数量不作为真实模型效果或科学准确率。

本地能力的实际红绿保留：

- 全文预检最终11项stub行为红，再11项green；最初范围讨论的红也保留。[预检红](B04-T02-local-evidence/logs/b04-t02-preflight-red.log)、[绿](B04-T02-local-evidence/logs/b04-t02-preflight-green.log)。
- 3项实际CLI先因命令缺失失败，接入后全部通过。[CLI红](B04-T02-local-evidence/logs/b04-t02-cli-preflight-red.log)、[绿](B04-T02-local-evidence/logs/b04-t02-cli-preflight-first.log)。
- Debug哨兵打印先真实失败，修正后通过。[Debug红](B04-T02-local-evidence/logs/b04-t02-debug-red.log)、[绿](B04-T02-local-evidence/logs/b04-t02-debug-green.log)。
- 凭据交付先编译通过但stub返回NotCalled而行为失败，最后6组全部通过。更早的模块注册编译失败独立保留，不计行为红。[注册尝试](B04-T02-local-evidence/logs/b04-t02-credential-registration-attempt.log)、[行为红](B04-T02-local-evidence/logs/b04-t02-credential-red.log)、[最终带证据路径的绿](B04-T02-local-evidence/logs/b04-t02-credential-final-evidence.log)。

最后凭据target的6组含 **19个真实private child**。父端先以ProcessObserver捕获native PID及创建时间，再ACK并观察实际退出，19份原始观察记录逐项核对。18个实际读帧场景有帧数/原请求SHA、SystemRoot及argv检查；只有blocked-stdin场景未读帧且只报告PID；sleep与cancel在停顿前已读取完整帧。帧交付和本地PID均不证明已发API。矩阵覆盖1与4096字节凭据、原始/转义/键名回显、未知/重复/截断、非零退出、双输出超限、stderr回显、坏候选/Token超限、300ms超时、阻塞stdin及取消。实际普通host回归11/11通过，亦包含于workspace。

## 独立复核与归档重读

独立审查没有发现未修复的代码阻断。审查者在新目录执行[13项CLI探针](B04-T02-local-evidence/independent-preflight/review-results.json)，覆盖规范字节边界、Formatting/尾部、CR-only、移动原件、非法UTF8、未知版本、删除Formatting/末尾Span并同步改顺序、仅修改normalized及SQL SHA等。所有探针均得到预定接受或拒绝，且调用前后项目、来源和哨兵指纹一致。另以[双来源探针](B04-T02-local-evidence/independent-project-scope/review-scope-results.json)实证其他已登记源损坏也会阻断首源预检，并据此修正范围文档。11项含Windows活守卫的单元与Debug1项也由审查者重跑通过。

根代理以真实CLI建立三个新的自有项目，分别使用已冻结的虚构纸灯箱R7、12290→8192边界和8193超限材料，共12次init/import/preflight调用。原选取文件移走后，每份预检执行两次，项目5份文件、原件和外部哨兵均不变，输出逐字节一致。[完整命令和结果](B04-T02-local-evidence/demo/results.json)。

归档复制完成后，使用归档内的三个项目实际再执行 **6次preflight**。来源文件、哨兵、项目均不变，四次成功及两次超限拒绝的stdout/stderr与原演示逐字节一致。[归档重读](B04-T02-local-evidence/archive-reloads.json)。复制文件或SHA存在本身没有被当作产品重读成功。

执行辅助故障亦保留：独立探针首次未关闭SQLite写连接而触发PROJECT_DATABASE_BUSY，关闭连接后在新目录复测，不是产品bug；[夹具错误记录](B04-T02-local-evidence/independent-preflight/review-initial-fixture-failure.json)及原失败目录均归档。归档助手第一次因Windows扩展路径的词法比较拒绝复制，第二次错误要求blocked-stdin提供环境字段；当时又错误解释sleep/cancel为未读帧，独立复核最终facts后已修正为18读帧/1未读，并在原历史JSON追加纠正；修正后保留已有证据并完成归档，两次错误分别有[路径记录](B04-T02-local-evidence/archive-initial-path-failure.json)与[字段记录](B04-T02-local-evidence/archive-initial-child-facts-failure.json)，均不是产品行为红。

## 尚未执行的验收

没有实际提供商/模型调用、真实密钥引用成功解析、远端请求/费用或可审计真实usage；没有PDF模型抽取、科学正确率、正式200原子/98%基准、跨平台或任意并发竞争穷举。64MiB raw上限仅静态核对，本轮未重新造其完整边界样本。容量和300ms故障政策不是性能基准。

依据已采纳D08和T02执行卡，收到提供商/API地址、模型、环境变量名称、明确资料外发范围与单次预算后，才能实现并执行唯一适配器的真实冒烟。密钥本身不应发到聊天或写入仓库。当前本地部分已提交，任务保持未完成；阻塞和后续恢复均通过devlog工具追加记录，不能把离线回归当成真实API成功。
