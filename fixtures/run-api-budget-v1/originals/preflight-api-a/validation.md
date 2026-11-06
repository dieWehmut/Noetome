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
      "sha256": "411539bd1c0eeba1296be190fc93d5d661441a4bf1608ea173f3e505f1bf7ebe"
    },
    "request-body.json": {
      "byte_length": 5238,
      "sha256": "b7c4109b5f4d88979381e9881b943f2d5404f3ebd28527587a14f0c6a2fff53b"
    },
    "request.json": {
      "byte_length": 2189,
      "sha256": "7fa9f52962de353b846c70088a55651de95ee6eb93d7e553fd2c2e6ff60a2156"
    }
  },
  "binding": {
    "content_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
    "import_id": "173e1a14-3be0-427c-aa27-1a710c60f541",
    "manifest_sha256": null,
    "normalized_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
    "parse_id": null,
    "project_id": "74b8210b-40bb-454e-bd7f-ea1853fcec37",
    "source_id": "3e77abe6-f1f3-41fd-9579-f97e6eb8a6fd",
    "version_id": "ef4d41a5-c1a6-4a97-9254-bc71e68288bb"
  },
  "configured_model": "noetome-test-model",
  "endpoint": "http://127.0.0.1:1/v1/responses",
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
  "provider": "openai_responses",
  "published": false,
  "report_kind": "text_api_extraction",
  "request_id": "29b6eda9-e16a-49d0-9693-be1b5b3c3f24",
  "request_strategy": "openai-responses-strict-json-schema-v1",
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
    "input_reserved_tokens": 6262,
    "model_tokenizer": null,
    "model_usage": null,
    "model_usage_status": "not_validated",
    "network_attempts": 0,
    "paid_calls": 0,
    "price_usd": null,
    "real_model_calls": 0,
    "request_utf8_bytes": 5238,
    "reservation_policy": "request_utf8_bytes_plus_1024_v1",
    "reserved_tokens": 8310
  },
  "validated_atoms": 0
}
```
