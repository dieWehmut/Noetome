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
      "sha256": "c934065acc2181a999d6c4e08da1b332be1f508cf80f148d7c6ac9901fc497d8"
    },
    "request-body.json": {
      "byte_length": 7133,
      "sha256": "6f94aa1e038e25e97093731dd036e61e283feed727c5e1c008ed9ccff3f98f8e"
    },
    "request.json": {
      "byte_length": 2197,
      "sha256": "e214f900506fa9904b27f1009206dcd681703ba661908b78aabb674eb879adce"
    },
    "response.bin": {
      "byte_length": 13739,
      "sha256": "a6081c965ef7f64416639b4cb3ade403ae72bd97331a3af0f8627927f954cbe9"
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
      "code": "EXTRACT_QUALIFIERS",
      "message": "EXTRACT_QUALIFIERS：已知单位和每个条件必须保留在原子限定信息中。"
    }
  ],
  "http_request_id": "a7c92c3e-2087-4527-b0f9-88201fb01189",
  "http_status": 200,
  "knowledge_status": "pending",
  "producer": "api",
  "provider": "openai_chat_completions",
  "published": false,
  "report_kind": "text_api_extraction",
  "request_id": "f40f579a-206b-4c3e-898e-884f4432bec5",
  "request_strategy": "tangmao-deepseek-chat-nested-atoms-v4",
  "response_complete": true,
  "response_id": "571e1a73cb864412a30c8805715f3ec8",
  "response_receipt": {
    "byte_length": 13739,
    "omission_reason": null,
    "sha256": "a6081c965ef7f64416639b4cb3ade403ae72bd97331a3af0f8627927f954cbe9"
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
    "input_reserved_tokens": 8157,
    "model_tokenizer": null,
    "model_usage": {
      "input_tokens": 1712,
      "output_tokens": 3071,
      "total_tokens": 4783
    },
    "model_usage_status": "validated",
    "network_attempts": 1,
    "paid_calls": null,
    "price_usd": null,
    "raw_model_usage": {
      "completion_tokens": 3071,
      "prompt_tokens": 1712,
      "total_tokens": 4783
    },
    "real_model_calls": 1,
    "request_utf8_bytes": 7133,
    "reservation_policy": "request_utf8_bytes_plus_1024_v1",
    "reserved_tokens": 16349
  },
  "validated_atoms": 0
}
```
