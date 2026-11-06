# 选定扫描公式重建预览报告

状态：preview_ready；预览：saved；知识：pending；科学质量：not_run；发布：false。

识别模式：local_http_test；实际网络尝试：1。价格、Token tokenizer 和账单未知；原始 TeX 未知。

选定原扫描区域、单请求正文、实际 HTTP 原响应、已核实用量与语义核对分开保存；本地人工 HTTP 服务只验证工程契约。

```json
{
  "schema_version": 1,
  "producer": "knowledge_scan_formula",
  "scope": "selected_scan_formula_v1",
  "request_id": "cdede24a-fff0-4de2-87e7-c678e592f66e",
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
    "recognition": "passed",
    "expression_match": "passed",
    "symbol_definitions": "passed",
    "semantic_review": "passed",
    "note_plan": "passed",
    "rendering": "passed",
    "saved_output": "passed"
  },
  "decision": {
    "representation": "native_math",
    "fallback_reasons": [],
    "compatibility_rewrites": [],
    "issue_codes": []
  },
  "recognition": {
    "schema_version": 1,
    "recognized_tex": "x_i = -\\frac{a_i}{b_i}",
    "uncertainties": []
  },
  "api": {
    "actual_model": "noetome-test-\\n-\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\u0061\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\u0062\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\u0063\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\u0064\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\u0065\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\u0066",
    "confidence": null,
    "configured_model": "noetome-test-mixed-key",
    "endpoint": "http://127.0.0.1:37807/v1/chat/completions",
    "errors": [],
    "finish_reason": "stop",
    "http_request_id": null,
    "http_status": 200,
    "provider": "openai_chat_completions",
    "request_strategy": "tangmao-vision-scan-v1",
    "response_complete": true,
    "response_id": "synthetic-mixed-credential-only",
    "response_receipt": {
      "byte_length": 610,
      "omission_reason": null,
      "sha256": "829c6410fc1bebc8326b4c536dfa65a3a505b7cfdf6296c8d3bfc386bcf3b52b"
    },
    "service_tier": null,
    "transport_mode": "local_http_test",
    "usage": {
      "input_reserved_tokens": 8186,
      "model_tokenizer": null,
      "model_usage": {
        "input_tokens": 12,
        "output_tokens": 8,
        "total_tokens": 20
      },
      "model_usage_status": "validated",
      "network_attempts": 1,
      "paid_calls": 0,
      "price_usd": null,
      "raw_model_usage": {
        "completion_tokens": 8,
        "prompt_tokens": 12,
        "total_tokens": 20
      },
      "real_model_calls": 0,
      "request_utf8_bytes": 7162,
      "reservation_policy": "request_utf8_bytes_plus_1024_v1",
      "reserved_tokens": 16378
    }
  },
  "recognition_request_fingerprint": {
    "sha256": "dbcec9851bc5ee7fd248b95c2438d04175b959db6a0e1d83f2710dde4682f9db",
    "byte_length": 7162
  },
  "recognition_response_fingerprint": {
    "sha256": "829c6410fc1bebc8326b4c536dfa65a3a505b7cfdf6296c8d3bfc386bcf3b52b",
    "byte_length": 610
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
    "sha256": "0875255a8c45ad8e07cacf626f8b3dfcb11f6c4555833deeb6f5c3337997557d",
    "byte_length": 2290
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
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
    },
    "source-api-config.json": {
      "sha256": "8eaef055e229c471137262d648116bb12ac653f3a38ee9ec6fd31bb7b3be1e5d",
      "byte_length": 614
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
      "content_sha256": "c341037053a1f88fe3678ea1bec78efd93e47917808de683b8174d84b5dd9b50",
      "byte_length": 1095,
      "mappings": [
        {
          "kind": "title",
          "source_field": "case.title",
          "start_byte": 41,
          "end_byte": 65
        },
        {
          "kind": "expression",
          "source_field": "compiled_expression",
          "start_byte": 85,
          "end_byte": 105
        },
        {
          "kind": "recognized_expression",
          "source_field": "recognition.recognized_tex",
          "start_byte": 235,
          "end_byte": 257
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[0].symbol",
          "start_byte": 313,
          "end_byte": 314
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[0].definition",
          "start_byte": 339,
          "end_byte": 361
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[0].unit",
          "start_byte": 386,
          "end_byte": 388
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[1].symbol",
          "start_byte": 427,
          "end_byte": 428
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[1].definition",
          "start_byte": 453,
          "end_byte": 466
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[1].unit",
          "start_byte": 491,
          "end_byte": 493
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[2].symbol",
          "start_byte": 532,
          "end_byte": 533
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[2].definition",
          "start_byte": 558,
          "end_byte": 573
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[2].unit",
          "start_byte": 598,
          "end_byte": 611
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[3].symbol",
          "start_byte": 650,
          "end_byte": 651
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[3].definition",
          "start_byte": 676,
          "end_byte": 689
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[3].unit",
          "start_byte": 714,
          "end_byte": 727
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[0]",
          "start_byte": 758,
          "end_byte": 767
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[1]",
          "start_byte": 781,
          "end_byte": 789
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[0]",
          "start_byte": 814,
          "end_byte": 826
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[1]",
          "start_byte": 840,
          "end_byte": 849
        },
        {
          "kind": "exception",
          "source_field": "semantics.exceptions[0]",
          "start_byte": 874,
          "end_byte": 914
        },
        {
          "kind": "attachment_link",
          "source_field": "case.region",
          "start_byte": 937,
          "end_byte": 971
        },
        {
          "kind": "attachment_link",
          "source_field": "case.page",
          "start_byte": 973,
          "end_byte": 1000
        },
        {
          "kind": "attachment_link",
          "source_field": "recognition",
          "start_byte": 1002,
          "end_byte": 1033
        },
        {
          "kind": "attachment_link",
          "source_field": "recognition.recognized_tex",
          "start_byte": 1035,
          "end_byte": 1065
        },
        {
          "kind": "attachment_link",
          "source_field": "case.pdf",
          "start_byte": 1067,
          "end_byte": 1093
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "bae11a3322a95ab1985b70887cd56f5cd2b03bdd4ee40275c2d199cfaedf25a4",
      "byte_length": 162
    },
    "compiled.tex": {
      "sha256": "10d4289783c1ae5b281008f169ccad7a8c961d7f178b47c38d09e4279bb2abf2",
      "byte_length": 20
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
      "sha256": "0875255a8c45ad8e07cacf626f8b3dfcb11f6c4555833deeb6f5c3337997557d",
      "byte_length": 2290
    },
    "profile.json": {
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
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
      "sha256": "dbcec9851bc5ee7fd248b95c2438d04175b959db6a0e1d83f2710dde4682f9db",
      "byte_length": 7162
    },
    "request.json": {
      "sha256": "2038cff053248232ab76f21658ccf47a24f1f7336738ecba5d0954183588f7ec",
      "byte_length": 1507
    },
    "response.bin": {
      "sha256": "829c6410fc1bebc8326b4c536dfa65a3a505b7cfdf6296c8d3bfc386bcf3b52b",
      "byte_length": 610
    },
    "source-api-config.json": {
      "sha256": "8eaef055e229c471137262d648116bb12ac653f3a38ee9ec6fd31bb7b3be1e5d",
      "byte_length": 614
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
      "sha256": "c341037053a1f88fe3678ea1bec78efd93e47917808de683b8174d84b5dd9b50",
      "byte_length": 1095
    }
  }
}
```
