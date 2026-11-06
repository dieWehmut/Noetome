# B06V-T03 最终 Windows Rust 全量原件

最终C轮在真实Windows CPython3.12.14执行固定严格runner，**workspace 726项通过、0失败、0忽略**。`cargo fmt --all --check`、`cargo check --workspace --locked --offline`、当前CLI预构建/scan namespace检查、`cargo test --workspace --locked --offline -- --test-threads=4 --nocapture`及最终CLI构建均实际exit0。完整结果见[独立审计](audit-result.json)，每条实际命令、PID、stdout/stderr指纹和源/外部哨兵保护记录均保存在原件归档。

实际选定CLI为 `D:\Noetome\target\b06v-t03-final-rust-20261106-c\noetome-selected.exe`，SHA256 `e42c534d07f66662411fc52f22561510b79ba1c63f14f8b6f419db7c4e0e3274`，25,939,968字节，独立复制且nlink1。它与最终实际构建CLI逐字节一致。单位测试使用的旧 `repo/target/debug/noetome.exe` fallback也由这轮真实预构建复制并核对；开始时只清理选定shared target中的noetome-cli包缓存，保留依赖，避免前次源根的库/PE缓存被误认作本轮构建。

各轮完整冻结1,915份输入/源文件。最终C轮的1,825份保守Rust编译/输入依赖（Core/Cargo、Tauri Rust、sidecars、所有作者fixture及cargo入口）与当前源逐字节一致，每条实际命令后均核对冻结树与外部哨兵。最终格式采集器随后只新增一行原identity保存；这一Python范围变化在audit中单独列出，不能称其最终版由Rust测试验证。最终完整Python与其完整依赖快照、五视觉独立审计由主执行者分别执行。

PDF解释器是明确的 `D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe`，实际3.12.14，全部PDF包版本与已固定runtime-lock精确一致；native fake解释器是 `C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe`，实际同为3.12.14。每轮新runtime-temp及外部哨兵，live模型凭据从测试子进程环境移除，真实API/模型/付费调用为0。

## 真实失败和最小修复

A轮首次全量在CLI lib阶段467项通过、1项失败，实际exit101。唯一失败为 `unknown_source_cache_entry_added_during_raw_read_is_rejected`：工作线程未观察到Win32共享违例32，真实注入没有发生，因此不能计为保护验收通过。原stderr、源SHA和全部guard记录保留，没有将三次随后选定用例的成功诊断包装成全量通过。

读原测试发现spawn后主线程立即验证，工作线程可能尚未进入raw probe循环。旧源SHA `45dc7f4309f445567ef72ffa75cfb1d1bd56c2f639bd36048513610fd995c3e0`。经本任务主执行者授权，只修改[原测试同步](../../../../../crates/noetome-cli/tests/support/source_cache_race_tests.rs)，加入两个零容量通道：工作线程准备真实raw probe并进入循环后发送ready，主线程收到再release；原5秒观测deadline在release之后建立。没有增加sleep/16MiB文件大小，不接受未注入，不删除共享违例/真实修改或blocked/owner/最终清单断言。新源SHA `2702b9dfb0d782cdf6262a98bb68e98099f2650ef963633e4e3dcafcfadfca7a`。

B轮新冻结源下四项限定缓存并发测试以4线程全部通过，分别实际观测mode0Applied、mode1Applied、mode2Blocked、mode3Blocked。之后C轮再次新建完整immutable snapshot并从cacheinvalidate/fmt/check开始全量，726/0/0通过；完整回归也独立再次观测四种mode。失败、诊断、修复限定验证和最终全量都有原件，见[修复说明](../review/source-cache-race-readiness.md)。

## 原件与复核

| 原件 | SHA256 | 字节 / 成员 |
| --- | --- | --- |
| [A/B/C运行、失败、命令、PE及哨兵](native-originals.zip) | `058fed0632bcfc69f43764ad1893cebad831c48e0d99b3b6a49c1bd1923329e0` | 38,490,577 / 88 |
| [三次完整冻结源码与指纹](source-originals.zip) | `a83d9c0ee111344ba8c45433dcc70021897b3ae1bb9256fd3a6a43f9440a0370` | 22,092,329 / 5,751 |

每个ZIP成员的原路径、字节数和SHA均在对应manifest，完整成员集合、CRC、SHA读回核对通过。所有源快照原字节重读匹配。只省略runtime-temp中的编译和测试scratch、shared Cargo依赖缓存；保留实际测试源、实际命令与原输出、输入版本和保护收据、外部哨兵及选定PE，不推测未保存的临时状态。

```text
python doc/validation/assets/B06V-T03/windows-rust/verify_windows_rust.py
```

此入口只读明确A/B/C原根、当前Rust依赖与全部归档成员，重新核对真实结果及原件，写审计摘要，不运行Rust或模型。`--build`只用于首次归档，存在的ZIP拒绝覆盖。原始Windows源和结果根仍位于 `D:\Noetome\target\b06v-t03-final-rust-20261106-{a,b,c}`；迁移后须从归档恢复明确根，不能把缺失原件当作通过。

这些结果证明当前明确源的Windows Rust回归；不代替Linux完整应用、跨版本真实显示、科学质量、真实OCR、生产Vault人工保护或B07检查点/预算重试的验收。
