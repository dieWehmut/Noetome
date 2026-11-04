# B05-T03 独立限定复审：R1 与 R2

本次只复审[独立整分支审查](independent-review.md)的两项 Important，不重做整分支广泛审查，不将本项复审替代完整回归或任务验收。

## 结论

R1、R2 均已用实际限定验证闭合；本次未发现修复引入的新 Important。完整任务仍须满足控制流程记录的必要门槛，Windows sidecar/桌面完整回归尚待支持环境，不宣称 B05-T03 全项完成。

## R1：正确对应主张的反证样例

`55a8af2` 追加第 20 个调用前冻结案例 `counter_to_claim`，源正文支持 10 m/s，其反证句明确表示同一范围的 10 m/s 不获本来源支持；原子 `a-speed` 的 `a-refutes` 链接因而对应同一个值 10，而另一来源保留 12。旧 `counter_support` 在 README 明确标为关系结构保留，未改原始失败/通过历史。

复审以 Git blob 逐项重查：原 19 份作者 case 定义与所有对应 `.txt` 源字节仍与 `dcb6b19` 相同；当前 20 案例及全部签名作者文件匹配冻结 SHA。读取最新 16 项 Rust 与 16 项 Python focused 实际日志，其结果分别为 16 passed/0 failed/0 ignored、16 tests/OK。

从实际保存根 `target/python-conflicts-21b4784793a94fa4a62970500c249098/counter_to_claim` 独立执行普通 Python `check`，退出 0；原报告保留值 10、其 supports/refutes 引文及原链接、另一来源 12、科学 not_run 与未决无成功 IR。

再只复制其输出到新的复审目录，同步删除正确对应的 `a-refutes` 链接及片段、调整真实链接计数，重写 JSON、Markdown 可见摘要/完整事实及 artifact 指纹。逐件 SHA 与双报告同事实均已实际核对。普通 `check` 退出 1，原因定位到重建原子的 `evidence_links: length`；没有依赖陈旧指纹或 Markdown 不一致而提前拒绝。原项目、case、真实输出与作者 gold 前后 inventory/字节指纹相同。

## R2：祖先与直接链接根的准入保护

`guard_path` 在解析/读取 v3 根之前逐个 lstat 传入路径的词法组件，拒绝符号链接和 reparse，同时核对目录/文件类型。generic verifier 以合法 v3 作者版本先分派到该门禁；两个普通 check 入口还在读取作者 gold 前校验其路径。

在新的 `/tmp/noetome-b05-t03-independent-rereview` 建立仅指向该测试案例的链接，分别将 project、case、output 放在链接祖先之后，并另建直接 case 根链接。generic 与 dedicated 两个 Python check 入口共 8 次实际返回 1，均报告 `ancestor link/reparse path`。Rust 普通 coordinate 对祖先 case 与直接 case 两次实际返回 2，未创建所选输出根。普通绝对根与普通相对根 check 各实际退出 0。

以上共 13 次实际子进程调用的完整 argv、stdout、stderr、退出值、CLI/核对脚本 SHA 见[independent-rereview-receipt.json](independent-rereview-receipt.json)。[原流归档](independent-rereview-originals.zip)保存所有调用输出及 receipt，[归档校验](independent-rereview-archive.json)记录 CRC、全部成员字节读回及冻结作者重查结果。没有改动源项目、原依据、作者 gold 或生产 Vault。

## 实测范围

这里验证的是公开自造案例的主张/反证对应、完整保留、同步遗漏拒绝和 Linux 路径保护。科学质量仍 not_run；不证明任意自然语言反证判定、科学真实性、Windows reparse 实测或完整产品兼容性。
