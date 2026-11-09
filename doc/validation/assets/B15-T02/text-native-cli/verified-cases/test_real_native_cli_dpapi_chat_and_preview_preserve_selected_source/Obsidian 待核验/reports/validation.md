# 本地HTTP假服务 API 候选验证

状态："candidates_validated"；候选原子：1。知识：pending；发布：false；科学质量：not_run。

配置模型："noetome-test-model"；实际返回模型："noetome-test-model"；HTTP状态：200；完整响应：true。

实际网络尝试：1。input_reserved_tokens 为实际请求UTF8字节加1024的工程预留；实际Token用量只记录已验证的原响应。价格、tokenizer及账单未知。

来源绑定、请求策略、用量、失败诊断及实际文件指纹如下，逐字段对应 validation.json。

```json
{
  "actual_model": "noetome-test-model",
  "artifacts": {
    ".owner.json": {
      "byte_length": 185,
      "sha256": "d5ac2b77c8519129a4cf3c5abcfa27025f51759fb36722ef87546fc776b96404"
    },
    "candidates.json": {
      "byte_length": 2753,
      "sha256": "6b71e7a5888a915e0850a36484e32476d3956e3eafc39eeb8f68d768672d5f5f"
    },
    "request-body.json": {
      "byte_length": 7509,
      "sha256": "79a3a62b5d026f21cef1620eb5f10cb45cbf8246fd06fc4263f964476b56d2c7"
    },
    "request.json": {
      "byte_length": 2196,
      "sha256": "6f8b685968211331a60ac6ed6cffe2bb623d5e36da126b373ace1212ab6e7a4e"
    },
    "response.bin": {
      "byte_length": 2092,
      "sha256": "11ddc63bbba5429d88157ad5772af493517e47de5dc237510205851240960f50"
    }
  },
  "binding": {
    "content_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
    "import_id": "a86045a2-4344-4109-8786-4f24da858967",
    "manifest_sha256": null,
    "normalized_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
    "parse_id": null,
    "project_id": "cb0e39c7-57f7-4bff-9152-800b6dd54527",
    "source_id": "ed7e0239-63b7-4dda-a2d4-35c82fa39885",
    "version_id": "cdc8ef80-b2a2-4cfd-944a-ee96b00a1cba"
  },
  "configured_model": "noetome-test-model",
  "endpoint": "http://127.0.0.1:38371/v1/chat/completions",
  "errors": [],
  "http_request_id": null,
  "http_status": 200,
  "knowledge_status": "pending",
  "producer": "api",
  "provider": "openai_chat_completions",
  "published": false,
  "report_kind": "text_api_extraction",
  "request_id": "88ca16f9-dabb-4837-ba8b-d47589dfbf92",
  "request_strategy": "tangmao-deepseek-chat-positive-revision-v6",
  "response_complete": true,
  "response_id": "chatcmpl-noetome-test",
  "response_receipt": {
    "byte_length": 2092,
    "omission_reason": null,
    "sha256": "11ddc63bbba5429d88157ad5772af493517e47de5dc237510205851240960f50"
  },
  "schema_version": 1,
  "scientific_quality": "not_run",
  "service_tier": null,
  "source_statuses": [
    "parsed"
  ],
  "status": "candidates_validated",
  "transport_mode": "local_http_test",
  "usage": {
    "input_reserved_tokens": 8533,
    "model_tokenizer": null,
    "model_usage": {
      "input_tokens": 100,
      "input_tokens_details": {
        "cached_tokens": 0
      },
      "output_tokens": 600,
      "output_tokens_details": {
        "reasoning_tokens": 0
      },
      "total_tokens": 700
    },
    "model_usage_status": "validated",
    "network_attempts": 1,
    "paid_calls": 0,
    "price_usd": null,
    "raw_model_usage": {
      "completion_tokens": 600,
      "completion_tokens_details": {
        "reasoning_tokens": 0
      },
      "prompt_tokens": 100,
      "prompt_tokens_details": {
        "cached_tokens": 0
      },
      "total_tokens": 700
    },
    "real_model_calls": 0,
    "request_utf8_bytes": 7509,
    "reservation_policy": "request_utf8_bytes_plus_1024_v1",
    "reserved_tokens": 16725
  },
  "validated_atoms": 1
}
```
