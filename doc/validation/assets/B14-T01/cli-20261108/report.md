# 审计事件日志（journal-traceability-v1）

两来源两任务的事件日志样例：一条失败被重试并解决，一条失败没有后续动作。

## 事件规模

- 任务 2 个、事件 6 条：started 2 / failed 2 / retried 1 / resolved 1。
- 来源 2 个、知识对象 2 个、受影响文件 2 个。

## 失败定位

| 事件 | 任务 | 分类 | 来源范围 | 知识对象 | 受影响文件 | 后续动作 |
| --- | --- | --- | --- | --- | --- | --- |
| e6 | task-compile | COMPILE_LINK | src-notes-b | obj-gluconeogenesis | file-gluconeogenesis | 未处理 |
| e2 | task-parse | PDF_PARSE | src-textbook-a | obj-glycolysis | file-glycolysis | 已重试或已解决 |

## 任务状态

| 任务 | 标题 | 事件 | 失败 | 当前结果 |
| --- | --- | --- | --- | --- |
| task-compile | 编译糖异生笔记 | 2 | 1 | failed |
| task-parse | 抽取教材A 第三章 | 4 | 1 | resolved |

## 边界

- 事件只追加，不重写、不重排；同一冻结输入两次运行字节相同。
- 声明里出现疑似凭据即整包拒绝：先过滤再继续会得到一份看起来完整、实际缺关键上下文的日志。
- 模型调用与网络请求 0 次；本日志只做定位，不判断内容质量与科学正确性。
