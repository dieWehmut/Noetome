# Responses 适配器离线合成 fixture v1

这些文件是 2026-11-02 冻结的纯离线样例，使用自造灯箱短句、固定 UUID 和明确带 synthetic 的 provider response id/model。不是服务商真实返回，不读取现有 R7 expected，不修改旧 candidate/response fixture，不证明真实 API 或科学质量。

`request.json` 是完整原 domain wire；`http-request.json` 是无认证 HTTP body；`http-response.json` 是合成已完成 Responses JSON；`canonical-response.json` 是本地证据转换后的 T01Outcome；`observation.json` 为其合成审计事实。这组文件只支持离线协议/跨语言重读一致性检查，不作为 B04-T03 独立内容基准。

| 文件 | 冻结 SHA256 |
| --- | --- |
| request.json | `891de1d123cf4cfde682b404628d4d0aa81478baa75b63b3aad38898a0b6c2fb` |
| http-request.json | `2f6db021721fce01d34a917ca94d17a2efd7fe8ac1f91f871c648125eec57353` |
| http-response.json | `497ef2605726630833bb67c468ac076ab4f442107ebf3b0bea0f1bb212dd40b8` |
| canonical-response.json | `5b978ba58f8e9e0534b77e1b3c4975cdaf138b1c48bbbd9bd4c4daf09cc6b4f9` |
| observation.json | `208149fc16e85db6b95f7391e77cee6052273c1fa7532667f477635f6b41632b` |

原文陈述 `灯箱的输入电压为 12 V，仅用于直流供电。` 为 56 UTF8 字节，局部半开区间 `[0,56)`，SHA `33e4ae0ee3597f873b7ee97cd68aae9d10ec3b6cde87cc90ca9e6647ec17cdff`。尾部 LF 属完整输入，未放入该 statement 的证据片段。未知费用保持 null，100/30/130 Token 均为合成 provider-response 数值。
