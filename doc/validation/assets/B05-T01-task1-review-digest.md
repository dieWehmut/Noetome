# Task 1 fresh spec and quality review

Reviewer: /root/review_coordination_core; base95a6f7b1d1fcd9ceaf8209e055f324e1f0d66808..head8f033e25ffad9ef114bf3c603cbd7ad2089673b3. Controller transcription digest of the returned review below; full original remains in the agent final response. Controller has not adjudicated away the Important finding.

## Spec Compliance

- ❌ 保存依据包的结构错误被改成 COORD_SOURCE_REVIEW/3。knowledge_coordinate_input.rs:295 的统一错误包装未保留 COORD_SCHEMA；额外清单、未知字段、重复键、缺必填字段或错误类型进入307行来源核对分支。Spec7要求结构COORD_SCHEMA/2，合法结构的来源/语义不一致才COORD_SOURCE_REVIEW/3。实际CLI额外文件探测复现。
- ✅ 其余可审查Task1范围匹配，四模块/普通CLI/helper/owner/discard/测试均有diff；无后续裁决、模型、凭据、GUI、生产Vault。
- ⚠️ 独立Python、最终日志、打包、推送、整分支最终审查属跨任务控制器门槛，Rustdiff不能证明B05-T01完成。
- ⚠️ 真实教材/科学质量/兼容性/性能认证未证明；pending/false/not_run/not_requested/零网络付费保持。

## Strengths and named risk checks

来源SourceSession真实重开、validate_gold/validate_candidates/assess及snapshot/IR/fps/report/MD重建（input400/468/498）；不用旧passed作为oracle。完整键/明确本地角色/原概念原子全支持证据保留、205行版本优先needs_review、263行原始分母稳定。results_saved160行后仍持有输入/source/output guards，仅撤去当前三个结果。

8MiB最初疑点已经实际探测解除：extraction_output.rs13的FILE_LIMIT由Artifact::create32..38在新文件创建前强制执行。真实三source_review每份约3.0MB snapshot/report且verify0，协调IR超过8MiB退出EXTRACT_SAVE_LIMIT/3，无三个协调结果保存。测试是新自造根target/compiler-temp/task1-review-probe-bff50z0m；不再为此追加实现。

## Findings

Critical: none.

Important R1 [P2]: knowledge_coordinate_input.rs295..310 把除5种错误外全部包装COORD_SOURCE_REVIEW/3，其中含parse及held.exact323返回的COORD_SCHEMA/2。实际额外依据文件得到exit3 `COORD_SOURCE_REVIEW：a: COORD_SCHEMA：unexpected case/bundle inventory`。将COORD_SCHEMA加入保留分支；普通入口补saved-bundle extra inventory和malformed saved JSON回归。完整内容伪造及实际binding mismatch继续source_review/3。当前tests/knowledge_coordinate.rs328主要修改case，未覆盖包装边界。

Minor R2: knowledge_coordination_model.rs57宏及75..83行DTO多个字段/serde约束压单行，82行report二十余字段难对照契约；可保留同约束按字段换行。当前不影响行为，不阻塞。

## Evidence

提供的workspace原receipt/stdout核对：exit0/源码不变，16suite合计551passed/0failed/0ignored，source_review24、coordination15；无编译警告，未重跑套件。修正replay04原stderrMissingCLI2、73pins、实际archivedCLI指纹存在；原准备/archive失败披露。

Focused probes: only named output-limit and schema-routing risks. Probe original stdout/stderr/exit remain under target/compiler-temp/task1-review-probe-bff50z0m, including coordinate-inventory-stderr.txt. External TEMP firstinit目录访问被拒绝是设置失败、不算行为证明；后来compiler-temp独占根成功。Diff截断后分段读取完整、未扩展整个仓库/最终审查。No source/index/HEAD/branch or protected user deletion mutation.

Assessment: Spec ❌; Task quality Needs fixes. Resolve R1 with actual regression and scoped review; R2 deferred for final triage. Cross-task ⚠️ gates tracked by controller, not RustTask1 completion proof.
