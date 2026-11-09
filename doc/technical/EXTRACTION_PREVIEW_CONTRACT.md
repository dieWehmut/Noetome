# 模型候选笔记预览

B15-T02 接通「自己的 PDF → 模型整理 → Obsidian 笔记」。本契约补充[来源数据契约](DATA_CONTRACTS.md)、[PDF 来源契约](PDF_SOURCE_CONTRACT.md)和[有界 API 契约](EXTRACTION_API_CONTRACT.md)，不改变 fixture、已审阅 IR、正式编译或生产发布的身份。

## 明确选择与恢复入口

`preview choices <项目> <import_id>` 返回保存的真实 Span ordinal 或 PDF 物理页 region_id、状态、字节数及是否可选。`preview units <项目> <import_id> <selection.json>` 通过同一 SourceSession 重新读取并验证真实原文，返回 binding 与完整 RequestUnit；最多64单位、正文64KiB。不接受前端自造原文、整页假区域或图像 OCR 猜测。

`preview export <项目> <import_id> <成功API根> <新输出根>` 零模型调用、零网络请求。导出前持有保存结果与来源文件的只读身份保护，重查请求、实际 HTTP 正文与原始响应、候选 wrapper、成功 JSON/Markdown、owner、文件清单与实际摘要；要求 request_id、binding、实际选取单位与当前来源完全一致，再执行既有领域候选验证。失败或篡改现场不导出成功笔记。

CLI 的 selection 文件采用严格 JSON、64KiB与身份保护。桌面传来的 selection 仅允许 text ordinal 或 pdf page_index/region_id；付费调用前先执行相同来源预检。

## 输出与身份

输出只创建尚不存在的新根，拒绝项目或 API 根内目标、既有根、链接/重解析点、路径穿越和选定目录的已有 `.obsidian` 祖先。不递归扫描用户 Vault；marker 检查的访问错误不会视为“不存在”。写入使用既有 OutputRoot 的独占文件与目录身份保护，失败只清理仍确认属于本次操作的产物。

```text
new-preview-root/
  .owner.json                  producer=extraction_preview
  vault/
    待核验笔记.md              总览、全部所选来源单位全文
    concept-0001.md            概念、候选、条件、单位、证据链接
    evidence-0001.md           实际引用、字节范围、SHA、来源定位
    attachments/source-0001.*  原件字节与来源摘要一致
  reports/
    preview.json               来源绑定、输出摘要、计数及预览事实
    request.json               原保存请求与其他 API 证据副本
    ...
```

Obsidian 打开 `vault/`。模型提供的 ID 不决定文件路径；采用固定序号文件名。模型及来源文本转义 Markdown、HTML 和 wiki 链接语法，包含 Obsidian 的 `%%` 隐藏注释、数学/删除线/高亮、围栏、列表及行首缩进；只有程序生成的概念、证据与原件链接有效。概念标题和链接别名把换行/Tab显示为可见的 ↵/⇥，原始模型字段仍完整保留在审计中。PDF 链接使用真实物理页 `#page=N`，同时保留 page_index、region_id、bbox、parse_id、资源摘要及 UTF-8 字节范围；不伪造印刷页码。

每份笔记明确 `pending`、`unverified`、`required=false`、`published=false`、`scientific_quality=not_run`。未知数值或单位保留 null/未知状态，不补成0。`local_http_test` 保留“本地 HTTP 假服务，仅用于测试链路”身份，不当作真实模型验收。离线安装自检使用独立作者 fixture 和新目录，与自己的资料流程分开。

## 桌面与共享预算

当前项目和工作流状态由 App 持有，切页保留，切项目清空旧资料/结果并拒绝迟到响应。模型调用只由主动按钮触发，单操作闸门防重复提交；结果根与临时配置保存在应用数据目录，位于来源项目外。

固定所选服务与模型沿用已授权糖猫云 Chat Completions / deepseek-flash，Windows CurrentUser DPAPI 存储凭据，不经前端传递密钥。共享累计10000000 Token账本不能按项目、安装或点击重置；没有账本显示未知并拒绝调用。历史恢复须用户明确确认无额外未记账调用，复制已冻结完整10条记录、33,185已用；访问失败、非法账本和既有账本均不可覆盖。单次 API 不自动重试，用量未知继续保留预留。

实际代码、运行检查、原生平台范围与未运行项以 B15 验证记录和任务账本为准；接口定义或文件存在不构成完成验收。
