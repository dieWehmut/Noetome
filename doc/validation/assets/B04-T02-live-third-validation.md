# 单请求文本 API 候选验证

状态："failed"；候选原子：0。知识：pending；发布：false；科学质量：not_run。

配置模型："deepseek-flash"；实际返回模型："deepseek-flash"；HTTP状态：200；完整响应：true。

实际网络尝试：1。input_reserved_tokens 为实际请求UTF8字节加1024的工程预留；实际Token用量只记录已验证的原响应。价格、tokenizer及账单未知。

来源绑定、请求策略、用量、失败诊断及实际文件指纹如下，逐字段对应 validation.json。

```json
{
  "actual_model": "deepseek-flash",
  "artifacts": {
    ".owner.json": {
      "byte_length": 174,
      "sha256": "7076dd2ea3a88606aa3832fd1b903323cb2a120b56922867c21acecd2c29edf6"
    },
    "request-body.json": {
      "byte_length": 6447,
      "sha256": "6e2c1aef140976852614777464dae7e68adb596713665327b5e092be26de4766"
    },
    "request.json": {
      "byte_length": 2208,
      "sha256": "bc973e8500cbf967854b3316754e804443e0c777779241c4c5a7c933fbb33abe"
    },
    "response.bin": {
      "byte_length": 14489,
      "sha256": "6ed4b15a330aeb8d552d8bd7ef2d684345427d662cbb54b2aae4b1a5732fb04b"
    }
  },
  "binding": {
    "content_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
    "import_id": "9f0abe91-99c5-44d0-8e15-2e9ea48da551",
    "manifest_sha256": null,
    "normalized_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
    "parse_id": null,
    "project_id": "2c9dd41b-e63a-4858-bc56-18039ca25f33",
    "source_id": "50cf1923-ffea-432e-b37c-199a5223a141",
    "version_id": "d944ab9b-0eaa-4d17-a9df-53bb4a8fba1b"
  },
  "configured_model": "deepseek-flash",
  "endpoint": "https://ai.1343263.xyz/v1/chat/completions",
  "errors": [
    {
      "code": "EXTRACT_API_CANDIDATES",
      "message": "EXTRACT_API_CANDIDATES：文本 API 响应未通过严格契约校验。"
    }
  ],
  "http_request_id": "afd69105-077a-473e-b03d-bb20fb87fe6f",
  "http_status": 200,
  "knowledge_status": "pending",
  "producer": "api",
  "provider": "openai_chat_completions",
  "published": false,
  "report_kind": "text_api_extraction",
  "request_id": "e1ef3592-f2a6-450a-a3ec-3e741cbac16b",
  "request_strategy": "tangmao-deepseek-chat-json-object-no-thinking-v3",
  "response_complete": true,
  "response_id": "addb2d96c7bb4dc38a064f5edac2c75e",
  "response_receipt": {
    "byte_length": 14489,
    "omission_reason": null,
    "sha256": "6ed4b15a330aeb8d552d8bd7ef2d684345427d662cbb54b2aae4b1a5732fb04b"
  },
  "schema_version": 1,
  "scientific_quality": "not_run",
  "service_tier": null,
  "source_statuses": [
    "parsed"
  ],
  "status": "failed",
  "transport_mode": "live",
  "usage": {
    "input_reserved_tokens": 7471,
    "model_tokenizer": null,
    "model_usage": {
      "input_tokens": 1583,
      "output_tokens": 3329,
      "total_tokens": 4912
    },
    "model_usage_status": "validated",
    "network_attempts": 1,
    "paid_calls": null,
    "price_usd": null,
    "raw_model_usage": {
      "completion_tokens": 3329,
      "prompt_tokens": 1583,
      "total_tokens": 4912
    },
    "real_model_calls": null,
    "request_utf8_bytes": 6447,
    "reservation_policy": "request_utf8_bytes_plus_1024_v1",
    "reserved_tokens": 15663
  },
  "validated_atoms": 0
}
```
