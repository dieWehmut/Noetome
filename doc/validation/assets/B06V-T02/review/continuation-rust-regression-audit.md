# B06V-T02 继续轮：Rust 与既有视觉回归独立复核

本次复核明确的原始归档、实际 Windows 运行目录和当前工作树，随后实际运行两项当前 Linux 短检查；没有重新运行测试、调用模型、读取凭据、修改累计账本或改写旧原件。实际逐文件结果见[审计原件](continuation-rust-regression-audit.json)，该文件保存当前输入指纹、每条历史实际命令及 stdout/stderr 指纹、原始汇总解析值和本轮进程核查。

## Rust 证据可继续复用

从 [Windows 最终 Rust](../windows/echo-rust-final/native-rust-final.json) 对应原件重算，而非只信汇总中的数量：

- 当前 **139 份 Cargo / Rust / sidecar 输入**逐文件等于完成测试时的 snapshot；源码集合覆盖全部 crate、sidecar 与 Cargo 清单、锁文件和工具链配置。
- 原始 `workspace-tests-attempt-1.stdout.bin` 实际解析出 **23 个 suite、720 passed、0 failed、0 ignored**，逐 suite 与最终记录相等。
- 七条命令的各自收据均与原始日志相等；原 snapshot SHA、stdout/stderr SHA、当时来源前后及外部哨兵保护一致。实际 `fmt`、workspace `check`、CLI `prebuild`、四路径帮助、workspace `test` 和 CLI `build` 均退出 0，未超时。
- Rust 原目录、Python 原目录和本次真实识别目录的三个实际选定 PE 均逐字节为 SHA256 `91fe246b522f8261da8ad678f465021bf76660ee402ad35e3a6bb4c4c2762be7`、25,876,480 字节，各自为单硬链接；它们不是仅在收据中自报为同一版本。
- 本轮实际 Windows CIM 复核七个 Rust 命令 PID，全部已不存在，观察时间与结果保存在 JSON 中。

当前完整 snapshot 中有后来改动的文档和 Python 校验器，所以“602 文件全工作树仍相等”不能作为当前结论。本次允许复用的范围为以上未改变的 Cargo / Rust / sidecar 构建与测试；新增证据工具及完整 Python 必须另以最终源码验证。

## 四项旧视觉回归可保留，扫描须重跑

[此前 Windows Python / 视觉阶段](../windows/current-python-final-02/native-python-final.json) 的 603 文件 snapshot 与本次工作树比较，只有 `MODEL_SERVICE.md`、`SCAN_FORMULA_CONTRACT.md`、`verify_scan_formula.py` 三份当时已有文件变化。当前 Rust、sidecar、所有冻结 fixture、其余视觉校验器及其导入的核对模块仍逐字节相等；新增 `capture_scan_budget_originals.py` 和 `prepare_scan_obsidian.py` 不在旧完整 Python 证据内。

| 原视觉审计 | 从原 stdout 解析出的实际结果 | 本轮复用判断 |
| --- | --- | --- |
| 比较表 | 124 次普通 CLI / 39 场景，passed | 校验器、依赖、原件与 PE 未变，可保留 |
| 流程图 | 73 次普通 CLI / 41 场景，passed | 校验器、依赖、原件与 PE 未变，可保留 |
| 公式 | 29 次普通 CLI / 56 场景，passed | 校验器、依赖、原件与 PE 未变，可保留 |
| 图像 | 51 次普通 CLI / 88 场景，passed | 校验器、依赖、原件与 PE 未变，可保留 |
| 扫描公式 | 31 次普通 CLI / 21 次人工 HTTP / 20 次同步篡改拒绝，passed | 固定字节长度读限已变，需要当前校验器的新审计 |

五项最终 stdout JSON 均与原汇总相等，各自普通进程收据、原 stdout/stderr 与归档逐字节相等。扫描第一次错误参数入口实际退出 2 的收据仍保留，第二次成功没有抹去失败。旧完整 Python 确实运行 364 项且 4 skip，但它不能证明本轮新增工具或 Windows 时戳修复通过。

## 原件完整性与局限

本轮另实际运行 Linux `cargo fmt --all --check` 与 `cargo check --workspace --locked --offline`，均退出 0，实际工具链为 Cargo / rustc **1.98.1**。五条已有编译 warning 保留，没有据此顺手修改未变化的源码。两次检查后再次核对 139 份输入仍等于 Windows 测试时源码。原工具返回的组合输出及命令见[Linux 短检查](continuation-linux-short-checks.json)；没有声称单独采集原 stdout/stderr，也没有由此推定 Linux 完整 workspace test 通过。

本轮对四份 ZIP 分别核对归档 SHA / 字节长、精确成员集合、每个成员的解压 CRC 及 SHA：Rust source **604** 成员、Rust regression **26,400** 成员、Python source **605** 成员、Python regression **12,396** 成员，全部相等。七条 Rust 命令和七条 Python / audit 命令的各自收据及 stdout/stderr，也与仍存在的实际 Windows 原文件相等。

以上是已有真实回归的当前源绑定复核，新增测试数量为 **0**。只有四项未变化的旧视觉审计可保留；当前扫描重跑、最终 Windows / Linux Python、新证据工具审查及真实显示 / 累计预算核验应由对应本轮原件另外证明。人工 HTTP 和作者工程样例不证明真实模型识别或科学正确率；当前记录不提供 Linux 完整 Rust、跨版本兼容、通用扫描 / OCR 或生产 Vault 验收。
