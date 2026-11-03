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
      "sha256": "022cb932df5c997858113231b424bd2cd46a20207c3c020327a36ab2d1308bf5"
    },
    "request-body.json": {
      "byte_length": 5182,
      "sha256": "98d6eb8488830737286bce69a79d247405c38e0b1d64a87a4bf372299e0644ec"
    },
    "request.json": {
      "byte_length": 2187,
      "sha256": "52b3ec97a779cc7abc1db08ef3efb59fc4e7d41031dc597a80964655ed7d9836"
    },
    "response.bin": {
      "byte_length": 2210,
      "sha256": "73de263b3d469facd25f3a80244c430165d0383b309efb37630fd280dab81d9b"
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
  "endpoint": "https://ai.1343263.xyz/v1/responses",
  "errors": [
    {
      "code": "EXTRACT_API_RESPONSE",
      "message": "EXTRACT_API_RESPONSE：文本 API 响应未通过严格契约校验。"
    }
  ],
  "http_request_id": "d28dfa72-6535-4ffa-ba2e-ee813af5ad8d",
  "http_status": 200,
  "knowledge_status": "pending",
  "producer": "api",
  "provider": "openai_responses",
  "published": false,
  "report_kind": "text_api_extraction",
  "request_id": "518b8ec8-a1d3-45bc-bdca-6141601d72dd",
  "request_strategy": "tangmao-deepseek-responses-json-schema-v1",
  "response_complete": true,
  "response_id": "1b32e9c459fbd2ff67f674edcb3e7835",
  "response_receipt": {
    "byte_length": 2210,
    "omission_reason": null,
    "sha256": "73de263b3d469facd25f3a80244c430165d0383b309efb37630fd280dab81d9b"
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
    "input_reserved_tokens": 6206,
    "model_tokenizer": null,
    "model_usage": {
      "input_tokens": 551,
      "output_tokens": 385,
      "total_tokens": 936
    },
    "model_usage_status": "validated",
    "network_attempts": 1,
    "paid_calls": null,
    "price_usd": null,
    "real_model_calls": null,
    "request_utf8_bytes": 5182,
    "reservation_policy": "request_utf8_bytes_plus_1024_v1",
    "reserved_tokens": 14398
  },
  "validated_atoms": 0
}
```
