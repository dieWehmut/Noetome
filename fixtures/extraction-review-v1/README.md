# 离线来源复核工程夹具

gold.json 是独立作者先保存的 Schema2 基准，包含两条件和一个例外。source.txt 明确标记工程用途；不用于证明真实模型或科学正确率。候选是随后构造的 synthetic local_http_test 保存包装，绝不发送请求。测试从普通 source import 的实际身份填入 binding，从原文 UTF8 字节计算证据范围和 SHA。review.json 的 gold SHA 固定原字节，不修改 gold 补通过。示例 candidates.json 的占位身份须由实际登记身份替换才能运行。
