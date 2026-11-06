# 选定扫描公式重建预览报告

状态：needs_review；预览：blocked；知识：pending；科学质量：not_run；发布：false。

识别模式：local_http_test；实际网络尝试：1。价格、Token tokenizer 和账单未知；原始 TeX 未知。

选定原扫描区域、单请求正文、实际 HTTP 原响应、已核实用量与语义核对分开保存；本地人工 HTTP 服务只验证工程契约。

```json
{
  "schema_version": 1,
  "producer": "knowledge_scan_formula",
  "scope": "selected_scan_formula_v1",
  "request_id": "a4bf2aa5-eed9-4a88-af5c-bc3956502816",
  "status": "needs_review",
  "preview_status": "blocked",
  "knowledge_status": "pending",
  "scientific_quality": "not_run",
  "published": false,
  "network_calls": 1,
  "paid_calls": 0,
  "case": {
    "schema_version": 1,
    "purpose": "scan_formula",
    "selection_id": "scan_scalar_unreadable_subscript",
    "title": "单页扫描公式下标缺损",
    "source_kind": "scanned_page",
    "pdf": {
      "file": "page.pdf",
      "sha256": "945d35792f24732ab90ad296eadf2596ac53ee847cc37133b6aa22447fe62970"
    },
    "page": {
      "file": "page.png",
      "sha256": "b08f28c60807a30d2777e092a1bf94359332e9ee3cb9bebe3a620ec8195f18de",
      "page": 1,
      "canvas": [
        800,
        1000
      ]
    },
    "region": {
      "file": "region.png",
      "sha256": "8a673a96084428fb3938219a2de990edc645b7e3c64b39b6044ac55363fb881e",
      "bounds": [
        120,
        175,
        560,
        200
      ],
      "legibility": "unreadable"
    },
    "semantics": {
      "file": "semantics.json",
      "sha256": "da8f2665eb37c59b3fa044fb1dbace3e7df149bc99d0c1a6f5efae832c62c733"
    }
  },
  "semantics": {
    "schema_version": 1,
    "producer": "author_scan_declaration",
    "expression": null,
    "symbols": [
      {
        "symbol": "x",
        "definition": "Reported displacement.",
        "unit": "mg"
      },
      {
        "symbol": "a",
        "definition": "Input amount.",
        "unit": "mg"
      },
      {
        "symbol": "b",
        "definition": "Positive scale.",
        "unit": "dimensionless"
      },
      {
        "symbol": "i",
        "definition": "Sample index.",
        "unit": "dimensionless"
      }
    ],
    "conditions": [
      "a_i >= 0.",
      "b_i > 0."
    ],
    "ranges": [
      "i in {1, 2}.",
      "x_i <= 0."
    ],
    "exceptions": [
      "If b_i = 0, the expression is undefined."
    ],
    "uncertainties": [
      {
        "kind": "subscript",
        "description": "Missing scan material covers the denominator subscript; that subscript is unknown."
      }
    ]
  },
  "profile": {
    "schema_version": 1,
    "app": "Obsidian",
    "app_version": "1.6.7",
    "mathjax_version": "3.2.2",
    "native_math": true,
    "compatibility_dfrac": true,
    "local_images": true,
    "verified": true
  },
  "checks": {
    "source_preservation": "passed",
    "region_match": "passed",
    "recognition": "needs_review",
    "expression_match": "needs_review",
    "symbol_definitions": "needs_review",
    "semantic_review": "needs_review",
    "note_plan": "not_run",
    "rendering": "not_run",
    "saved_output": "passed"
  },
  "decision": {
    "representation": null,
    "fallback_reasons": [],
    "compatibility_rewrites": [],
    "issue_codes": [
      "SCAN_REGION_UNREADABLE",
      "SCAN_CRITICAL_SYMBOL_UNKNOWN",
      "SCAN_SEMANTICS_UNKNOWN",
      "SCAN_RECOGNITION_UNCERTAIN",
      "SCAN_RECOGNITION_FAILED"
    ]
  },
  "recognition": {
    "schema_version": 1,
    "recognized_tex": null,
    "uncertainties": [
      {
        "kind": "subscript",
        "description": "The selected critical mark is obscured; cannot transcribe it."
      }
    ]
  },
  "api": {
    "actual_model": "noetome-test-artificial-returned-model",
    "confidence": null,
    "configured_model": "noetome-test-independent-scan",
    "endpoint": "http://127.0.0.1:36901/v1/chat/completions",
    "errors": [],
    "finish_reason": "stop",
    "http_request_id": null,
    "http_status": 200,
    "provider": "openai_chat_completions",
    "request_strategy": "tangmao-vision-scan-v1",
    "response_complete": true,
    "response_id": "noetome-test-independent-response",
    "response_receipt": {
      "byte_length": 499,
      "omission_reason": null,
      "sha256": "6ca17742e93dc9b418704a3234ed19bdcab5389582a534c7c5290e90fdaa1fc4"
    },
    "service_tier": null,
    "transport_mode": "local_http_test",
    "usage": {
      "input_reserved_tokens": 8465,
      "model_tokenizer": null,
      "model_usage": {
        "input_tokens": 100,
        "output_tokens": 30,
        "total_tokens": 130
      },
      "model_usage_status": "validated",
      "network_attempts": 1,
      "paid_calls": 0,
      "price_usd": null,
      "raw_model_usage": {
        "completion_tokens": 30,
        "prompt_tokens": 100,
        "total_tokens": 130
      },
      "real_model_calls": 0,
      "request_utf8_bytes": 7441,
      "reservation_policy": "request_utf8_bytes_plus_1024_v1",
      "reserved_tokens": 16657
    }
  },
  "recognition_request_fingerprint": {
    "sha256": "175324fa7294f2c6bb889d3d08367e55110f04eb55529ba03fdc3376d54f7841",
    "byte_length": 7441
  },
  "recognition_response_fingerprint": {
    "sha256": "6ca17742e93dc9b418704a3234ed19bdcab5389582a534c7c5290e90fdaa1fc4",
    "byte_length": 499
  },
  "tokens": null,
  "plan_fingerprint": null,
  "inputs": {
    "original.pdf": {
      "sha256": "945d35792f24732ab90ad296eadf2596ac53ee847cc37133b6aa22447fe62970",
      "byte_length": 41214
    },
    "original.png": {
      "sha256": "8a673a96084428fb3938219a2de990edc645b7e3c64b39b6044ac55363fb881e",
      "byte_length": 4496
    },
    "page.png": {
      "sha256": "b08f28c60807a30d2777e092a1bf94359332e9ee3cb9bebe3a620ec8195f18de",
      "byte_length": 49716
    },
    "profile.json": {
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
    },
    "source-api-config.json": {
      "sha256": "2c52b874ac483f9b2bba6c945b43aaaa49d143465f46e76b71ba3b5fbab96aa3",
      "byte_length": 624
    },
    "source-case.json": {
      "sha256": "eec48688ef0de35973bcf43ba8df7df6f67158233acf68a7726551f908237bec",
      "byte_length": 847
    },
    "source-semantics.json": {
      "sha256": "da8f2665eb37c59b3fa044fb1dbace3e7df149bc99d0c1a6f5efae832c62c733",
      "byte_length": 852
    }
  },
  "files": [],
  "artifacts": {
    ".owner.json": {
      "sha256": "4be591b9f5828a2aba95b8564c1b5863e867deb3f24ef0e3013b00543c9fdf22",
      "byte_length": 162
    },
    "original.pdf": {
      "sha256": "945d35792f24732ab90ad296eadf2596ac53ee847cc37133b6aa22447fe62970",
      "byte_length": 41214
    },
    "original.png": {
      "sha256": "8a673a96084428fb3938219a2de990edc645b7e3c64b39b6044ac55363fb881e",
      "byte_length": 4496
    },
    "page.png": {
      "sha256": "b08f28c60807a30d2777e092a1bf94359332e9ee3cb9bebe3a620ec8195f18de",
      "byte_length": 49716
    },
    "profile.json": {
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
    },
    "recognition.json": {
      "sha256": "4b5a4220a7b7e9a3b1543fffb64245d7b65d4545af4f6b44325a594d84872586",
      "byte_length": 201
    },
    "request-body.json": {
      "sha256": "175324fa7294f2c6bb889d3d08367e55110f04eb55529ba03fdc3376d54f7841",
      "byte_length": 7441
    },
    "request.json": {
      "sha256": "312bfbde5a97a6c55b1934ad32b259d53c9407010fa4880d6dc90090120fe78d",
      "byte_length": 1524
    },
    "response.bin": {
      "sha256": "6ca17742e93dc9b418704a3234ed19bdcab5389582a534c7c5290e90fdaa1fc4",
      "byte_length": 499
    },
    "source-api-config.json": {
      "sha256": "2c52b874ac483f9b2bba6c945b43aaaa49d143465f46e76b71ba3b5fbab96aa3",
      "byte_length": 624
    },
    "source-case.json": {
      "sha256": "eec48688ef0de35973bcf43ba8df7df6f67158233acf68a7726551f908237bec",
      "byte_length": 847
    },
    "source-semantics.json": {
      "sha256": "da8f2665eb37c59b3fa044fb1dbace3e7df149bc99d0c1a6f5efae832c62c733",
      "byte_length": 852
    }
  }
}
```
