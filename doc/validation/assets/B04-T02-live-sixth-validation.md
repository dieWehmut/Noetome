# 单请求文本 API 候选验证

状态："candidates_validated"；候选原子：1。知识：pending；发布：false；科学质量：not_run。

配置模型："deepseek-flash"；实际返回模型："deepseek-flash"；HTTP状态：200；完整响应：true。

实际网络尝试：1。input_reserved_tokens 为实际请求UTF8字节加1024的工程预留；实际Token用量只记录已验证的原响应。价格、tokenizer及账单未知。

来源绑定、请求策略、用量、失败诊断及实际文件指纹如下，逐字段对应 validation.json。

```json
{
  "actual_model": "deepseek-flash",
  "artifacts": {
    ".owner.json": {
      "byte_length": 174,
      "sha256": "6ea55c13194fea48247ff1d4597a26d5c3d24c35fc84f2c3c35168efee83345d"
    },
    "candidates.json": {
      "byte_length": 3503,
      "sha256": "92fed9123fa6f711ce76df524e4ff79a36aada79a7c4f5fb72f64f68027915fa"
    },
    "request-body.json": {
      "byte_length": 7505,
      "sha256": "a16e16a0a1636a10c54d8869cc171ef79572fe86801ee9b24e4bb154fba75939"
    },
    "request.json": {
      "byte_length": 2202,
      "sha256": "4b8b46d76c3b95f796bbd5945cf371720fbe479c99aff734f51b28589ed955d1"
    },
    "response.bin": {
      "byte_length": 19760,
      "sha256": "019f0ae95e154b230175cd6df31912e8130593f657344db723a4c3da7d8e82ac"
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
  "errors": [],
  "http_request_id": "cf562687-a7db-44fa-8eed-cab2b7f0963a",
  "http_status": 200,
  "knowledge_status": "pending",
  "producer": "api",
  "provider": "openai_chat_completions",
  "published": false,
  "report_kind": "text_api_extraction",
  "request_id": "4bed77a5-3409-4f72-9709-fa35ba0b752f",
  "request_strategy": "tangmao-deepseek-chat-positive-revision-v6",
  "response_complete": true,
  "response_id": "dce25394bdf24086868a87e11890aa69",
  "response_receipt": {
    "byte_length": 19760,
    "omission_reason": null,
    "sha256": "019f0ae95e154b230175cd6df31912e8130593f657344db723a4c3da7d8e82ac"
  },
  "schema_version": 1,
  "scientific_quality": "not_run",
  "service_tier": null,
  "source_statuses": [
    "parsed"
  ],
  "status": "candidates_validated",
  "transport_mode": "live",
  "usage": {
    "input_reserved_tokens": 8529,
    "model_tokenizer": null,
    "model_usage": {
      "input_tokens": 1792,
      "output_tokens": 4512,
      "total_tokens": 6304
    },
    "model_usage_status": "validated",
    "network_attempts": 1,
    "paid_calls": null,
    "price_usd": null,
    "raw_model_usage": {
      "completion_tokens": 4512,
      "prompt_tokens": 1792,
      "total_tokens": 6304
    },
    "real_model_calls": 1,
    "request_utf8_bytes": 7505,
    "reservation_policy": "request_utf8_bytes_plus_1024_v1",
    "reserved_tokens": 16721
  },
  "validated_atoms": 1
}
```
