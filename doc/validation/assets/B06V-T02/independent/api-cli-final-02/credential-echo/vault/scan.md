# 选定扫描公式重建预览报告

状态：needs_review；预览：blocked；知识：pending；科学质量：not_run；发布：false。

识别模式：local_http_test；实际网络尝试：1。价格、Token tokenizer 和账单未知；原始 TeX 未知。

选定原扫描区域、单请求正文、实际 HTTP 原响应、已核实用量与语义核对分开保存；本地人工 HTTP 服务只验证工程契约。

```json
{
  "schema_version": 1,
  "producer": "knowledge_scan_formula",
  "scope": "selected_scan_formula_v1",
  "request_id": "dcf505ce-35c4-4ca0-9a52-cf6dff077558",
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
    "selection_id": "scan_scalar_clear",
    "title": "单页扫描公式清晰",
    "source_kind": "scanned_page",
    "pdf": {
      "file": "page.pdf",
      "sha256": "147a454d1814793df52120417cca2172daa8dd81bfb6e242cbb64935ad911803"
    },
    "page": {
      "file": "page.png",
      "sha256": "b416f4d3aea278bb3a1d05682183b7ad56a4c9bfb6bbb7bde61b9185343028c5",
      "page": 1,
      "canvas": [
        800,
        1000
      ]
    },
    "region": {
      "file": "region.png",
      "sha256": "8b84103383f6d4a6fc4054f2221c3ff0e41bf355b4db09a3b085e0af0ec2d4bc",
      "bounds": [
        120,
        175,
        560,
        200
      ],
      "legibility": "clear"
    },
    "semantics": {
      "file": "semantics.json",
      "sha256": "83be529ba9430eb748ae185553718607e0c61e0d417519b3c66697c8d44e8547"
    }
  },
  "semantics": {
    "schema_version": 1,
    "producer": "author_scan_declaration",
    "expression": "x_i = -\\frac{a_i}{b_i}",
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
    "uncertainties": []
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
    "symbol_definitions": "passed",
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
      "SCAN_API_CREDENTIAL_ECHO"
    ]
  },
  "recognition": null,
  "api": {
    "actual_model": null,
    "confidence": null,
    "configured_model": "noetome-test-independent-scan",
    "endpoint": "http://127.0.0.1:36949/v1/chat/completions",
    "errors": [
      {
        "code": "SCAN_API_CREDENTIAL_ECHO",
        "message": "SCAN_API_CREDENTIAL_ECHO：credential echo detected; original response and parsed fields omitted"
      }
    ],
    "finish_reason": null,
    "http_request_id": null,
    "http_status": 200,
    "provider": "openai_chat_completions",
    "request_strategy": "tangmao-vision-scan-v1",
    "response_complete": true,
    "response_id": null,
    "response_receipt": {
      "byte_length": 40,
      "omission_reason": "omitted_credential_echo",
      "sha256": "ddb6dd36bd8f779ddb9fa51296463525ed6fc00580e23b03fcc7e726d8aba1a5"
    },
    "service_tier": null,
    "transport_mode": "local_http_test",
    "usage": {
      "input_reserved_tokens": 8193,
      "model_tokenizer": null,
      "model_usage": null,
      "model_usage_status": "not_validated",
      "network_attempts": 1,
      "paid_calls": 0,
      "price_usd": null,
      "raw_model_usage": null,
      "real_model_calls": 0,
      "request_utf8_bytes": 7169,
      "reservation_policy": "request_utf8_bytes_plus_1024_v1",
      "reserved_tokens": 16385
    }
  },
  "recognition_request_fingerprint": {
    "sha256": "8d1abc470bcd4973b087073958b005beeb07ec0a585c6f50b1af99af6b4454a6",
    "byte_length": 7169
  },
  "recognition_response_fingerprint": {
    "sha256": "ddb6dd36bd8f779ddb9fa51296463525ed6fc00580e23b03fcc7e726d8aba1a5",
    "byte_length": 40
  },
  "tokens": [
    "x",
    "_",
    "i",
    "=",
    "-",
    "\\frac",
    "{",
    "a",
    "_",
    "i",
    "}",
    "{",
    "b",
    "_",
    "i",
    "}"
  ],
  "plan_fingerprint": null,
  "inputs": {
    "original.pdf": {
      "sha256": "147a454d1814793df52120417cca2172daa8dd81bfb6e242cbb64935ad911803",
      "byte_length": 37603
    },
    "original.png": {
      "sha256": "8b84103383f6d4a6fc4054f2221c3ff0e41bf355b4db09a3b085e0af0ec2d4bc",
      "byte_length": 4292
    },
    "page.png": {
      "sha256": "b416f4d3aea278bb3a1d05682183b7ad56a4c9bfb6bbb7bde61b9185343028c5",
      "byte_length": 45594
    },
    "profile.json": {
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
    },
    "source-api-config.json": {
      "sha256": "6ab8276c5848b01c6c9ca7848be92859253068c64166743ce5f6b2843016f3cb",
      "byte_length": 624
    },
    "source-case.json": {
      "sha256": "0ca6d59df807c4b9811b2c17e69bc5b4be7a245b9df586c15a95c1299e3c9e83",
      "byte_length": 821
    },
    "source-semantics.json": {
      "sha256": "83be529ba9430eb748ae185553718607e0c61e0d417519b3c66697c8d44e8547",
      "byte_length": 725
    }
  },
  "files": [],
  "artifacts": {
    ".owner.json": {
      "sha256": "ca56f0ca1d8d64f55d25f79d6d72707128307050fbc5e6abbc0113e68ffc9b97",
      "byte_length": 162
    },
    "original.pdf": {
      "sha256": "147a454d1814793df52120417cca2172daa8dd81bfb6e242cbb64935ad911803",
      "byte_length": 37603
    },
    "original.png": {
      "sha256": "8b84103383f6d4a6fc4054f2221c3ff0e41bf355b4db09a3b085e0af0ec2d4bc",
      "byte_length": 4292
    },
    "page.png": {
      "sha256": "b416f4d3aea278bb3a1d05682183b7ad56a4c9bfb6bbb7bde61b9185343028c5",
      "byte_length": 45594
    },
    "profile.json": {
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
    },
    "request-body.json": {
      "sha256": "8d1abc470bcd4973b087073958b005beeb07ec0a585c6f50b1af99af6b4454a6",
      "byte_length": 7169
    },
    "request.json": {
      "sha256": "2ef23d01f19aa89547eebf3abe82af59400b8c2dccbbd7e00427541a53431607",
      "byte_length": 1520
    },
    "source-api-config.json": {
      "sha256": "6ab8276c5848b01c6c9ca7848be92859253068c64166743ce5f6b2843016f3cb",
      "byte_length": 624
    },
    "source-case.json": {
      "sha256": "0ca6d59df807c4b9811b2c17e69bc5b4be7a245b9df586c15a95c1299e3c9e83",
      "byte_length": 821
    },
    "source-semantics.json": {
      "sha256": "83be529ba9430eb748ae185553718607e0c61e0d417519b3c66697c8d44e8547",
      "byte_length": 725
    }
  }
}
```
