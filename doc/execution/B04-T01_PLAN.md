# B04-T01 实施计划

绑定规范：[EXTRACTION_CONTRACT.md](../technical/EXTRACTION_CONTRACT.md)。本计划全部分工属于同一个进行中任务，不开始 B04-T02/T03。

## 全局约束

保留用户 docs/ 旧 PRD 删除；当前分支 codex/b04-t01-extraction-gateway。离线单 fake，不写真实模型 SDK、生产 Vault 或来源 SQL。每段必要修复与行为验收完成后才记录完成。引用 UTF-8 与 PDF 真实 parse/resource，不复制 fixture 的假来源边界。只有任务账本工具改状态。

## 1. 冻结契约、作者夹具与真实 RED

先提交规范、受控中文来源/独立 gold、Python 普通 CLI 验证器及 Rust 行为入口。作者期待值在输出之前声明。执行现有 CLI，记录未提供 extract 入口的真实 RED；后续每项失效保护都有实际变异负例。验证器自行创建全新项目和输出、读取真实 SQL/来源/逐页资源绑定，不信任被测报告自行宣称只读。

## 2. 只读会话与 DTO、共用知识规则

新 implementer 负责 extraction_model.rs、extraction_source.rs 和 fixture_knowledge.rs 的通用概念/原子规则提取，附独立针对性 Rust 单测。根执行者不同时编辑这些文件。会话在第一次来源读取之前持有根、配置、数据库、缓存所有权和全部已登记快照的读取能力；先应用该入口的总量限制，再复用 current_store 的既有结构/内容检查。对真实导入选定 Span/PDF 区域生成请求单位。结束时验证身份、mtime/SHA/清单。实现者自行执行 RED/GREEN 和旧 fixture/source 回归后交付，不自行提交或修改任务状态。

## 3. 网关、fake、保存与 CLI

根执行者顺序集成 extraction.rs、extraction_fake.rs、extraction_output.rs、lib/main 路由。请求计量、预算预留、明确可空未知值、真实等待超时/外部取消、原样信封 JSON 和严格候选校验。只向全新受控输出写实际文件；成功前双报告与来源会话最终重验。实现过程中运行前述普通 CLI 验证器；维护独立来源前后证据与错误码预期。

## 4. 新审查、最终验收与 Git

Source/DTO 部分 fresh spec+质量审查，集成后另一 fresh broad review。对证据/能力/预算/保存 Important 问题先复现再修复，重验对应行为。运行完整 cargo fmt/check/test、Python unittest、独立 CLI 验证、devlog check、git diff --check。同步 DATA_CONTRACTS/DESIGN/TESTING/docREADME 与实测 validation 产物；工具 complete B04-T01，不把日志当验收证据。分小步提交，push -u origin 并核对远端 HEAD，保持 B04-T02/T03 todo。

## 接口与裁定

2→3：ExtractionRequest/SourceBinding/CandidatePayload 精确 DTO、SourceSession.open/units/verify。1→2/3：固定作者 gold 与 CLI 输入/错误预期。2 中通用校验只提取已存在的基础规则，fixture required 政策留在原处；3 中 fake 所有原子仍 unverified。输出成功是候选合同通过，不等于知识发布。常规接口选择依据已接受 D07/D08 和用户自动推进授权，不另设审批障碍。
