# 三来源协调作者样例

这是在协调器和候选产物实现前冻结的离线结构样例。原文为自造的“样例 A”，没有真实教材或科学正确率主张。gold-a/b/c.json 与 expected.json 是作者预期；候选不能反向生成这些文件，测试失败后不能按实际输出改写预期。

source-a.txt 与 source-b.txt 有意逐字节相同，通过普通 source import 分别获得不同的 source_id。二者的速度事实应归为一个事实并保留两份位置。source-c.txt 的质量事实保留为另一互补属性。全部例外和条件必须保留，原值没有单位换算。

identity-map.json 是显式身份决定。输入 ID a/b/c 和原概念/原子 ID 在样例中固定；实际项目、source/version/import/span ID 和 SHA 由普通 CLI 取得并绑定，不能伪造。

每份候选都使用两项组件概念，原子链接“样例 A”和本份属性，保留 known value、单位、两条条件和一条例外；producer=api、transport_mode=local_http_test、interpretation_timing=before_synthetic_response。此标记表示本地人工候选，协调验收不发送 HTTP 或真实模型请求。

准备驱动在新的测试根先读取并保存本目录全部作者字节，再创建候选、执行 extract verify，最后写入协调 case 的 manifest 和真实九文件指纹。每份 source_review 必须由普通 CLI 成功保存，不能直接构造带 passed 标志的依据。

输入分母为 3 个单原子依据；事实分母为 2 个完整事实；出现分母为 3 个实际来源位置；证据分母为 3 条 supports 链接；独立来源分母为 3 个真实 source_id。科学质量 not_run、研究 not_requested、知识 pending、published=false。此样例不证明三本真实教材已完成或一般实体识别已验收。
