# 链接迁移报告（mig-section-promotion-v1）

小节「糖酵解核心机制」提升为独立笔记前后的两套自造作者包；迁移期望表写在 expected.json，先于实现手写。

## 计数

- 笔记：迁移前 3 篇、迁移后 5 篇
- 引用：迁移前 3 条、迁移后 4 条；断链 0 条
- 迁移后规范位置 3 条、跳转位置 1 条
- 知识对象 5 个：被引用 2 个、未被引用 3 个、身份违规 0 个
- 重写目标 2 处；unchanged 1 篇、links_only 1 篇、promoted 1 篇、created 2 篇

## 知识对象身份

| 对象 | 迁移前位置 | 迁移后位置 | 迁移前引用 | 迁移后引用 |
| --- | --- | --- | --- | --- |
| obj-glycolysis-definition | [[笔记/糖酵解.md#定义]] | [[笔记/糖酵解.md#定义]] | 0 | 0 |
| obj-glycolysis-mechanism | [[笔记/糖酵解.md#核心机制]] | [[笔记/糖酵解-核心机制.md]] | 2 | 3 |
| obj-gluconeogenesis-bypass | [[笔记/糖异生.md#四步绕行]] | [[笔记/糖异生.md#四步绕行]] | 1 | 1 |
| obj-ppp-definition | [[笔记/磷酸戊糖途径.md#定义]] | [[笔记/磷酸戊糖途径.md#定义]] | 0 | 0 |
| obj-ppp-mechanism | [[笔记/磷酸戊糖途径.md#核心机制]] | [[笔记/磷酸戊糖途径.md#核心机制]] | 0 | 0 |

## 迁移后引用

| 笔记 | 行 | 链接 | 对象 | 匹配 |
| --- | --- | --- | --- | --- |
| 笔记/糖酵解.md | 5 | [[笔记/糖酵解-核心机制.md]] | obj-glycolysis-mechanism | canonical |
| 笔记/糖酵解.md | 8 | [[笔记/糖异生.md#四步绕行]] | obj-gluconeogenesis-bypass | canonical |
| 笔记/糖异生.md | 11 | [[笔记/糖酵解-核心机制.md\\|糖酵解的核心机制]] | obj-glycolysis-mechanism | canonical |
| 笔记/糖异生.md | 12 | [[笔记/糖酵解.md#核心机制]] | obj-glycolysis-mechanism | redirect |

## 笔记核对

| 笔记 | 变更 | 逐字节相等 | 剥链接后相等 |
| --- | --- | --- | --- |
| 笔记/糖酵解.md | promoted | 否 | 否 |
| 笔记/糖酵解-核心机制.md | created | — | — |
| 笔记/糖异生.md | links_only | 否 | 是 |
| 笔记/磷酸戊糖途径.md | unchanged | 是 | 是 |
| 附录/糖酵解.md | created | — | — |

## 边界

- 迁移只核对与报告，不改写作者包：after 目录由作者提供本层只验证身份是否保持。
- 链接先解析到知识对象再比较身份，文件名只是位置；断链、歧义、目标缺失、表格未转义别名或对象位置缺失一律整包拒绝。
- 模型调用与网络请求 0 次；不判断内容质量与科学正确性。
