# 选定扫描公式重建预览报告

状态：preview_ready；预览：saved；知识：pending；科学质量：not_run；发布：false。

识别模式：local_http_test；实际网络尝试：1。价格、Token tokenizer 和账单未知；原始 TeX 未知。

选定原扫描区域、单请求正文、实际 HTTP 原响应、已核实用量与语义核对分开保存；本地人工 HTTP 服务只验证工程契约。

```json
{
  "schema_version": 1,
  "producer": "knowledge_scan_formula",
  "scope": "selected_scan_formula_v1",
  "request_id": "ae76cc46-c073-4160-89da-51ca27a2462f",
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
    "app_version": "1.15.0",
    "mathjax_version": "4.1.3",
    "native_math": false,
    "compatibility_dfrac": true,
    "local_images": true,
    "verified": true
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
    "recognized_tex": "x_i = -\\frac{a_i}{b_i}",
    "uncertainties": []
  },
  "api": {
    "actual_model": "deepseek-flash",
    "confidence": null,
    "configured_model": "noetome-test-independent-scan",
    "endpoint": "http://127.0.0.1:38013/v1/chat/completions",
    "errors": [],
    "finish_reason": "stop",
    "http_request_id": null,
    "http_status": 200,
    "provider": "openai_chat_completions",
    "request_strategy": "tangmao-vision-scan-v1",
    "response_complete": true,
    "response_id": "chatcmpl-1793960996",
    "response_receipt": {
      "byte_length": 824,
      "omission_reason": null,
      "sha256": "db02969fb52a1dde15000e6183fc66fd6d148e6b30f1f11414ec1649d1a96948"
    },
    "service_tier": null,
    "transport_mode": "local_http_test",
    "usage": {
      "input_reserved_tokens": 8193,
      "model_tokenizer": null,
      "model_usage": {
        "input_tokens": 444,
        "input_tokens_details": {
          "cached_tokens": 0
        },
        "output_tokens": 144,
        "total_tokens": 588
      },
      "model_usage_status": "validated",
      "network_attempts": 1,
      "paid_calls": 0,
      "price_usd": null,
      "raw_model_usage": {
        "completion_tokens": 144,
        "prompt_tokens": 444,
        "prompt_tokens_details": {
          "cached_tokens": 0
        },
        "total_tokens": 588
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
    "sha256": "db02969fb52a1dde15000e6183fc66fd6d148e6b30f1f11414ec1649d1a96948",
    "byte_length": 824
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
    "sha256": "028b2f7856c7b04b23cab86a322e0ee282b60b9fb9e6bed01e24efe73625dff1",
    "byte_length": 2310
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
      "sha256": "610391d2345993ed80f93ded368c507415f4cf2d2e381d4f2b09d9f62437d39b",
      "byte_length": 203
    },
    "source-api-config.json": {
      "sha256": "efdeac5ca08eef0d016f8902c3a713e6842624f3ca94b4d7a02a537169f5daa8",
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
      "content_sha256": "28afd0db261f02df83db1bc98f6eab15d135dc31bee900604cbb351e39b69d7a",
      "byte_length": 1086,
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
          "end_byte": 248
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[0].symbol",
          "start_byte": 304,
          "end_byte": 305
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[0].definition",
          "start_byte": 330,
          "end_byte": 352
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[0].unit",
          "start_byte": 377,
          "end_byte": 379
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[1].symbol",
          "start_byte": 418,
          "end_byte": 419
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[1].definition",
          "start_byte": 444,
          "end_byte": 457
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[1].unit",
          "start_byte": 482,
          "end_byte": 484
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[2].symbol",
          "start_byte": 523,
          "end_byte": 524
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[2].definition",
          "start_byte": 549,
          "end_byte": 564
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[2].unit",
          "start_byte": 589,
          "end_byte": 602
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[3].symbol",
          "start_byte": 641,
          "end_byte": 642
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[3].definition",
          "start_byte": 667,
          "end_byte": 680
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[3].unit",
          "start_byte": 705,
          "end_byte": 718
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[0]",
          "start_byte": 749,
          "end_byte": 758
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[1]",
          "start_byte": 772,
          "end_byte": 780
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[0]",
          "start_byte": 805,
          "end_byte": 817
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[1]",
          "start_byte": 831,
          "end_byte": 840
        },
        {
          "kind": "exception",
          "source_field": "semantics.exceptions[0]",
          "start_byte": 865,
          "end_byte": 905
        },
        {
          "kind": "attachment_link",
          "source_field": "case.region",
          "start_byte": 928,
          "end_byte": 962
        },
        {
          "kind": "attachment_link",
          "source_field": "case.page",
          "start_byte": 964,
          "end_byte": 991
        },
        {
          "kind": "attachment_link",
          "source_field": "recognition",
          "start_byte": 993,
          "end_byte": 1024
        },
        {
          "kind": "attachment_link",
          "source_field": "recognition.recognized_tex",
          "start_byte": 1026,
          "end_byte": 1056
        },
        {
          "kind": "attachment_link",
          "source_field": "case.pdf",
          "start_byte": 1058,
          "end_byte": 1084
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "7f2ed692c4404744855498c80a1df2342502bda3b1974b088d2ccd475027ef97",
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
      "sha256": "028b2f7856c7b04b23cab86a322e0ee282b60b9fb9e6bed01e24efe73625dff1",
      "byte_length": 2310
    },
    "profile.json": {
      "sha256": "610391d2345993ed80f93ded368c507415f4cf2d2e381d4f2b09d9f62437d39b",
      "byte_length": 203
    },
    "recognition.json": {
      "sha256": "85e5843f23f0b34487898aeeef56390f6989c4b65950b7da574a3a42455f69ef",
      "byte_length": 95
    },
    "recognized.tex": {
      "sha256": "02184791e0b11cff2797c0772f014919fe392fe862b82c295cd77b3c451e537e",
      "byte_length": 22
    },
    "request-body.json": {
      "sha256": "8d1abc470bcd4973b087073958b005beeb07ec0a585c6f50b1af99af6b4454a6",
      "byte_length": 7169
    },
    "request.json": {
      "sha256": "87b1aec21e937a26056a87aad176824b12183324f6d7695c047aac6f1dec1cbe",
      "byte_length": 1552
    },
    "response.bin": {
      "sha256": "db02969fb52a1dde15000e6183fc66fd6d148e6b30f1f11414ec1649d1a96948",
      "byte_length": 824
    },
    "source-api-config.json": {
      "sha256": "efdeac5ca08eef0d016f8902c3a713e6842624f3ca94b4d7a02a537169f5daa8",
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
      "sha256": "28afd0db261f02df83db1bc98f6eab15d135dc31bee900604cbb351e39b69d7a",
      "byte_length": 1086
    }
  }
}
```
