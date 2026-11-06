# 选定扫描公式重建预览报告

状态：preview_ready；预览：saved；知识：pending；科学质量：not_run；发布：false。

识别模式：local_http_test；实际网络尝试：1。价格、Token tokenizer 和账单未知；原始 TeX 未知。

选定原扫描区域、单请求正文、实际 HTTP 原响应、已核实用量与语义核对分开保存；本地人工 HTTP 服务只验证工程契约。

```json
{
  "schema_version": 1,
  "producer": "knowledge_scan_formula",
  "scope": "selected_scan_formula_v1",
  "request_id": "8baed070-6517-4526-bc37-654da57877a8",
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
    "selection_id": "scan_scalar_unreadable_sign",
    "title": "单页扫描公式符号缺损",
    "source_kind": "scanned_page",
    "pdf": {
      "file": "page.pdf",
      "sha256": "ee33b3e6bf16cb972ae438c4c310757de8a85aaba4bfa206964a98a056aa3c00"
    },
    "page": {
      "file": "page.png",
      "sha256": "eb34efd27cfa1bc70bd9d274c0d9bf92eb91bf9e56997bfeb188041cddcb8097",
      "page": 1,
      "canvas": [
        800,
        1000
      ]
    },
    "region": {
      "file": "region.png",
      "sha256": "7a22e31bb437a46687f26d5264d5e31befe837fee962b6198f73db2886a8d81f",
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
      "sha256": "3c9ec361aa7fb88a84d108483db2f503b394d137888b595b4eaf83a660196443"
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
        "kind": "sign",
        "description": "Missing scan material covers the sign before the fraction; that sign is unknown."
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
    "compatibility_rewrites": [
      "SCAN_SINGLE_SCRIPT_GROUP_COMPATIBILITY"
    ],
    "issue_codes": []
  },
  "recognition": {
    "schema_version": 1,
    "recognized_tex": null,
    "uncertainties": [
      {
        "kind": "sign",
        "description": "The selected critical mark is obscured; cannot transcribe it."
      }
    ]
  },
  "api": {
    "actual_model": "noetome-test-artificial-returned-model",
    "confidence": null,
    "configured_model": "noetome-test-independent-scan",
    "endpoint": "http://127.0.0.1:37205/v1/chat/completions",
    "errors": [],
    "finish_reason": "stop",
    "http_request_id": null,
    "http_status": 200,
    "provider": "openai_chat_completions",
    "request_strategy": "tangmao-vision-scan-v1",
    "response_complete": true,
    "response_id": "noetome-test-independent-response",
    "response_receipt": {
      "byte_length": 494,
      "omission_reason": null,
      "sha256": "2a9ad397ee8be440f54f1fdb977c0a4e25221bdedeed048ca22d890fa1805f75"
    },
    "service_tier": null,
    "transport_mode": "local_http_test",
    "usage": {
      "input_reserved_tokens": 8505,
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
      "request_utf8_bytes": 7481,
      "reservation_policy": "request_utf8_bytes_plus_1024_v1",
      "reserved_tokens": 16697
    }
  },
  "recognition_request_fingerprint": {
    "sha256": "c80c05bd5f940dd742746bcdf5fe609bda43a1b132d423c1b066fcdc11112821",
    "byte_length": 7481
  },
  "recognition_response_fingerprint": {
    "sha256": "2a9ad397ee8be440f54f1fdb977c0a4e25221bdedeed048ca22d890fa1805f75",
    "byte_length": 494
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
    "sha256": "52997132d0c51212cf6a2c2dea92aecccd3c1302278ffbeda41634573413f04a",
    "byte_length": 2500
  },
  "inputs": {
    "original.pdf": {
      "sha256": "ee33b3e6bf16cb972ae438c4c310757de8a85aaba4bfa206964a98a056aa3c00",
      "byte_length": 41073
    },
    "original.png": {
      "sha256": "7a22e31bb437a46687f26d5264d5e31befe837fee962b6198f73db2886a8d81f",
      "byte_length": 4525
    },
    "page.png": {
      "sha256": "eb34efd27cfa1bc70bd9d274c0d9bf92eb91bf9e56997bfeb188041cddcb8097",
      "byte_length": 49763
    },
    "profile.json": {
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
    },
    "source-api-config.json": {
      "sha256": "d9dcb067aab9c004c5f059d2d451dbe0d57dc038323bb9fc6653630d09343e04",
      "byte_length": 624
    },
    "source-case.json": {
      "sha256": "d4afeedc1da24fa292c204e9b40b8a0271a91f426e1389e5e890af11f2b908aa",
      "byte_length": 842
    },
    "source-semantics.json": {
      "sha256": "3c9ec361aa7fb88a84d108483db2f503b394d137888b595b4eaf83a660196443",
      "byte_length": 845
    }
  },
  "files": [
    {
      "relative_path": "扫描公式预览.md",
      "content_sha256": "c2d56816231c99427fdc8dc2d59dccae14669ac3f7444275ee99f606fea7d279",
      "byte_length": 1101,
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
          "end_byte": 263
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[0].symbol",
          "start_byte": 319,
          "end_byte": 320
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[0].definition",
          "start_byte": 345,
          "end_byte": 367
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[0].unit",
          "start_byte": 392,
          "end_byte": 394
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[1].symbol",
          "start_byte": 433,
          "end_byte": 434
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[1].definition",
          "start_byte": 459,
          "end_byte": 472
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[1].unit",
          "start_byte": 497,
          "end_byte": 499
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[2].symbol",
          "start_byte": 538,
          "end_byte": 539
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[2].definition",
          "start_byte": 564,
          "end_byte": 579
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[2].unit",
          "start_byte": 604,
          "end_byte": 617
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[3].symbol",
          "start_byte": 656,
          "end_byte": 657
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[3].definition",
          "start_byte": 682,
          "end_byte": 695
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[3].unit",
          "start_byte": 720,
          "end_byte": 733
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[0]",
          "start_byte": 764,
          "end_byte": 773
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[1]",
          "start_byte": 787,
          "end_byte": 795
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[0]",
          "start_byte": 820,
          "end_byte": 832
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[1]",
          "start_byte": 846,
          "end_byte": 855
        },
        {
          "kind": "exception",
          "source_field": "semantics.exceptions[0]",
          "start_byte": 880,
          "end_byte": 920
        },
        {
          "kind": "attachment_link",
          "source_field": "case.region",
          "start_byte": 943,
          "end_byte": 977
        },
        {
          "kind": "attachment_link",
          "source_field": "case.page",
          "start_byte": 979,
          "end_byte": 1006
        },
        {
          "kind": "attachment_link",
          "source_field": "recognition",
          "start_byte": 1008,
          "end_byte": 1039
        },
        {
          "kind": "attachment_link",
          "source_field": "recognition.recognized_tex",
          "start_byte": 1041,
          "end_byte": 1071
        },
        {
          "kind": "attachment_link",
          "source_field": "case.pdf",
          "start_byte": 1073,
          "end_byte": 1099
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "e30b8f091289caac982c976c475a64b88978a6960b295c8b34faf36fdaf175c5",
      "byte_length": 162
    },
    "compiled.tex": {
      "sha256": "10d4289783c1ae5b281008f169ccad7a8c961d7f178b47c38d09e4279bb2abf2",
      "byte_length": 20
    },
    "original.pdf": {
      "sha256": "ee33b3e6bf16cb972ae438c4c310757de8a85aaba4bfa206964a98a056aa3c00",
      "byte_length": 41073
    },
    "original.png": {
      "sha256": "7a22e31bb437a46687f26d5264d5e31befe837fee962b6198f73db2886a8d81f",
      "byte_length": 4525
    },
    "page.png": {
      "sha256": "eb34efd27cfa1bc70bd9d274c0d9bf92eb91bf9e56997bfeb188041cddcb8097",
      "byte_length": 49763
    },
    "plan.json": {
      "sha256": "52997132d0c51212cf6a2c2dea92aecccd3c1302278ffbeda41634573413f04a",
      "byte_length": 2500
    },
    "profile.json": {
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
    },
    "recognition.json": {
      "sha256": "7dbbd8030d4836b003eaa06ebc2771f68b21d55ab69d89c2f81cb75b2f1ed069",
      "byte_length": 196
    },
    "request-body.json": {
      "sha256": "c80c05bd5f940dd742746bcdf5fe609bda43a1b132d423c1b066fcdc11112821",
      "byte_length": 7481
    },
    "request.json": {
      "sha256": "e94ed75c2df74ee6f65121a1f1e55c19556d2eabfa2e7392e39b06cb9b6c365d",
      "byte_length": 1567
    },
    "response.bin": {
      "sha256": "2a9ad397ee8be440f54f1fdb977c0a4e25221bdedeed048ca22d890fa1805f75",
      "byte_length": 494
    },
    "source-api-config.json": {
      "sha256": "d9dcb067aab9c004c5f059d2d451dbe0d57dc038323bb9fc6653630d09343e04",
      "byte_length": 624
    },
    "source-case.json": {
      "sha256": "d4afeedc1da24fa292c204e9b40b8a0271a91f426e1389e5e890af11f2b908aa",
      "byte_length": 842
    },
    "source-semantics.json": {
      "sha256": "3c9ec361aa7fb88a84d108483db2f503b394d137888b595b4eaf83a660196443",
      "byte_length": 845
    },
    "扫描公式预览.md": {
      "sha256": "c2d56816231c99427fdc8dc2d59dccae14669ac3f7444275ee99f606fea7d279",
      "byte_length": 1101
    }
  }
}

```
