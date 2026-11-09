# 本地HTTP假服务 API 候选验证

状态："failed"；候选原子：0。知识：pending；发布：false；科学质量：not_run。

配置模型："noetome-test-model"；实际返回模型：null；HTTP状态：null；完整响应：false。

实际网络尝试：0。input_reserved_tokens 为实际请求UTF8字节加1024的工程预留；实际Token用量只记录已验证的原响应。价格、tokenizer及账单未知。

来源绑定、请求策略、用量、失败诊断及实际文件指纹如下，逐字段对应 validation.json。

```json
{
  "actual_model": null,
  "artifacts": {
    ".owner.json": {
      "byte_length": 185,
      "sha256": "4e545110a630efe555c94e86f755509af31a3b3afbdc28a631f189ccd84d8373"
    },
    "request-body.json": {
      "byte_length": 7509,
      "sha256": "a0d3a00be6c1cf4d32a2ce62891e305958effe6b4423d8cf5abced143c1470b8"
    },
    "request.json": {
      "byte_length": 2196,
      "sha256": "a18623b7b3ec35d7ccd19b849f7a4f32c7a9c27d6236f3f869818cbc6e023a8e"
    }
  },
  "binding": {
    "content_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
    "import_id": "41f5c930-30fa-4985-8c9e-a4a9b230b5f1",
    "manifest_sha256": null,
    "normalized_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
    "parse_id": null,
    "project_id": "1e1617c1-008f-453c-934d-4a1d82264f02",
    "source_id": "327ec1b2-8747-49be-810d-40cd7b063ad0",
    "version_id": "554699a3-e7c1-42a5-b727-d0392af65393"
  },
  "configured_model": "noetome-test-model",
  "endpoint": "http://127.0.0.1:38291/v1/chat/completions",
  "errors": [
    {
      "code": "EXTRACT_BUDGET_STOP",
      "message": "EXTRACT_BUDGET_STOP：单次请求次数或预留总量超过明确预算；未调用。"
    }
  ],
  "http_request_id": null,
  "http_status": null,
  "knowledge_status": "pending",
  "producer": "api",
  "provider": "openai_chat_completions",
  "published": false,
  "report_kind": "text_api_extraction",
  "request_id": "b1accb01-d9eb-44cf-b463-69ae45b7f1fb",
  "request_strategy": "tangmao-deepseek-chat-positive-revision-v6",
  "response_complete": false,
  "response_id": null,
  "response_receipt": null,
  "schema_version": 1,
  "scientific_quality": "not_run",
  "service_tier": null,
  "source_statuses": [
    "parsed"
  ],
  "status": "failed",
  "transport_mode": "local_http_test",
  "usage": {
    "input_reserved_tokens": 8533,
    "model_tokenizer": null,
    "model_usage": null,
    "model_usage_status": "not_validated",
    "network_attempts": 0,
    "paid_calls": 0,
    "price_usd": null,
    "real_model_calls": 0,
    "request_utf8_bytes": 7509,
    "reservation_policy": "request_utf8_bytes_plus_1024_v1",
    "reserved_tokens": 16725
  },
  "validated_atoms": 0
}
```
