# 选定扫描公式重建预览报告

状态：preview_ready；预览：saved；知识：pending；科学质量：not_run；发布：false。

识别模式：local_http_test；实际网络尝试：1。价格、Token tokenizer 和账单未知；原始 TeX 未知。

选定原扫描区域、单请求正文、实际 HTTP 原响应、已核实用量与语义核对分开保存；本地人工 HTTP 服务只验证工程契约。

```json
{
  "schema_version": 1,
  "producer": "knowledge_scan_formula",
  "scope": "selected_scan_formula_v1",
  "request_id": "4f96ca29-4345-4958-805c-b9e928b056d3",
  "status": "preview_ready",
  "preview_status": "saved",
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
    "app_version": "999.0.0",
    "mathjax_version": "3.2.2",
    "native_math": true,
    "compatibility_dfrac": true,
    "local_images": true,
    "verified": false
  },
  "checks": {
    "source_preservation": "passed",
    "region_match": "passed",
    "recognition": "passed",
    "expression_match": "passed",
    "symbol_definitions": "passed",
    "semantic_review": "passed",
    "note_plan": "passed",
    "rendering": "passed",
    "saved_output": "passed"
  },
  "decision": {
    "representation": "formula_image",
    "fallback_reasons": [
      "SCAN_NATIVE_UNAVAILABLE"
    ],
    "compatibility_rewrites": [],
    "issue_codes": []
  },
  "recognition": {
    "schema_version": 1,
    "recognized_tex": "x_{i} = -\\frac{a_{i}}{b_{i}}",
    "uncertainties": []
  },
  "api": {
    "actual_model": "noetome-test-artificial-returned-model",
    "confidence": null,
    "configured_model": "noetome-test-independent-scan",
    "endpoint": "http://127.0.0.1:37517/v1/chat/completions",
    "errors": [],
    "finish_reason": "stop",
    "http_request_id": null,
    "http_status": 200,
    "provider": "openai_chat_completions",
    "request_strategy": "tangmao-vision-scan-v1",
    "response_complete": true,
    "response_id": "noetome-test-independent-response",
    "response_receipt": {
      "byte_length": 421,
      "omission_reason": null,
      "sha256": "35850546b2cc6c99568933827f22b065be8fd3ff45572e4328afe96d254ae72f"
    },
    "service_tier": null,
    "transport_mode": "local_http_test",
    "usage": {
      "input_reserved_tokens": 8193,
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
    "sha256": "35850546b2cc6c99568933827f22b065be8fd3ff45572e4328afe96d254ae72f",
    "byte_length": 421
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
  "plan_fingerprint": {
    "sha256": "80ec5c9643fa69161421e1f60bd4c1ce803c8b32b5accbd3ebf7faf0f9603e3c",
    "byte_length": 2316
  },
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
      "sha256": "ffa9d07fd8301f49edb43e6597b291c7d944d373ef0515057f5c85653f6c104c",
      "byte_length": 204
    },
    "source-api-config.json": {
      "sha256": "4d7941d1c98755e1c6f5c8255b4bb00739a6baff5a7ac1cb5676fc425a0c323d",
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
  "files": [
    {
      "relative_path": "扫描公式预览.md",
      "content_sha256": "36ad3401d126ba4d11ab70ed73d5d0999b6297603e2a5f07468d3fef8bf35280",
      "byte_length": 1092,
      "mappings": [
        {
          "kind": "title",
          "source_field": "case.title",
          "start_byte": 41,
          "end_byte": 65
        },
        {
          "kind": "image_embed",
          "source_field": "case.region",
          "start_byte": 82,
          "end_byte": 99
        },
        {
          "kind": "recognized_expression",
          "source_field": "recognition.recognized_tex",
          "start_byte": 226,
          "end_byte": 254
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[0].symbol",
          "start_byte": 310,
          "end_byte": 311
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[0].definition",
          "start_byte": 336,
          "end_byte": 358
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[0].unit",
          "start_byte": 383,
          "end_byte": 385
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[1].symbol",
          "start_byte": 424,
          "end_byte": 425
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[1].definition",
          "start_byte": 450,
          "end_byte": 463
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[1].unit",
          "start_byte": 488,
          "end_byte": 490
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[2].symbol",
          "start_byte": 529,
          "end_byte": 530
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[2].definition",
          "start_byte": 555,
          "end_byte": 570
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[2].unit",
          "start_byte": 595,
          "end_byte": 608
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[3].symbol",
          "start_byte": 647,
          "end_byte": 648
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[3].definition",
          "start_byte": 673,
          "end_byte": 686
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[3].unit",
          "start_byte": 711,
          "end_byte": 724
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[0]",
          "start_byte": 755,
          "end_byte": 764
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[1]",
          "start_byte": 778,
          "end_byte": 786
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[0]",
          "start_byte": 811,
          "end_byte": 823
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[1]",
          "start_byte": 837,
          "end_byte": 846
        },
        {
          "kind": "exception",
          "source_field": "semantics.exceptions[0]",
          "start_byte": 871,
          "end_byte": 911
        },
        {
          "kind": "attachment_link",
          "source_field": "case.region",
          "start_byte": 934,
          "end_byte": 968
        },
        {
          "kind": "attachment_link",
          "source_field": "case.page",
          "start_byte": 970,
          "end_byte": 997
        },
        {
          "kind": "attachment_link",
          "source_field": "recognition",
          "start_byte": 999,
          "end_byte": 1030
        },
        {
          "kind": "attachment_link",
          "source_field": "recognition.recognized_tex",
          "start_byte": 1032,
          "end_byte": 1062
        },
        {
          "kind": "attachment_link",
          "source_field": "case.pdf",
          "start_byte": 1064,
          "end_byte": 1090
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "f8ec48733207561018551349fb0b1bb26e908d99bd08d8eba2e239647b4a26c0",
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
    "plan.json": {
      "sha256": "80ec5c9643fa69161421e1f60bd4c1ce803c8b32b5accbd3ebf7faf0f9603e3c",
      "byte_length": 2316
    },
    "profile.json": {
      "sha256": "ffa9d07fd8301f49edb43e6597b291c7d944d373ef0515057f5c85653f6c104c",
      "byte_length": 204
    },
    "recognition.json": {
      "sha256": "393cd45e64ca88d57d38c7ec5acd653566c7b6a013a2ce4fb07bf383dc2e5861",
      "byte_length": 101
    },
    "recognized.tex": {
      "sha256": "a9a989236eaab9e3d991cac135c4575f20060f0c8a5bd8f51d7475d61ebcc172",
      "byte_length": 28
    },
    "request-body.json": {
      "sha256": "8d1abc470bcd4973b087073958b005beeb07ec0a585c6f50b1af99af6b4454a6",
      "byte_length": 7169
    },
    "request.json": {
      "sha256": "dfb0631986c9a544406c3fb6dbfdc4ed2e89e59638f2263906733d9fad42a195",
      "byte_length": 1518
    },
    "response.bin": {
      "sha256": "35850546b2cc6c99568933827f22b065be8fd3ff45572e4328afe96d254ae72f",
      "byte_length": 421
    },
    "source-api-config.json": {
      "sha256": "4d7941d1c98755e1c6f5c8255b4bb00739a6baff5a7ac1cb5676fc425a0c323d",
      "byte_length": 624
    },
    "source-case.json": {
      "sha256": "0ca6d59df807c4b9811b2c17e69bc5b4be7a245b9df586c15a95c1299e3c9e83",
      "byte_length": 821
    },
    "source-semantics.json": {
      "sha256": "83be529ba9430eb748ae185553718607e0c61e0d417519b3c66697c8d44e8547",
      "byte_length": 725
    },
    "扫描公式预览.md": {
      "sha256": "36ad3401d126ba4d11ab70ed73d5d0999b6297603e2a5f07468d3fef8bf35280",
      "byte_length": 1092
    }
  }
}
```
