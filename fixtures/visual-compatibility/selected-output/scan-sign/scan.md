# 选定扫描公式重建预览报告

状态：needs_review；预览：blocked；知识：pending；科学质量：not_run；发布：false。

识别模式：live；实际网络尝试：1。价格、Token tokenizer 和账单未知；原始 TeX 未知。

选定原扫描区域、单请求正文、实际 HTTP 原响应、已核实用量与语义核对分开保存；本地人工 HTTP 服务只验证工程契约。

```json
{
  "schema_version": 1,
  "producer": "knowledge_scan_formula",
  "scope": "selected_scan_formula_v1",
  "request_id": "1f17e6d4-94eb-4fdd-b1ad-bc9a47569a8c",
  "status": "needs_review",
  "preview_status": "blocked",
  "knowledge_status": "pending",
  "scientific_quality": "not_run",
  "published": false,
  "network_calls": 1,
  "paid_calls": null,
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
        "kind": "symbol",
        "description": "The symbol or coefficient between the equals sign and the fraction is obscured by a gray crossed-out box."
      }
    ]
  },
  "api": {
    "actual_model": "deepseek-flash",
    "confidence": null,
    "configured_model": "deepseek-flash",
    "endpoint": "https://ai.1343263.xyz/v1/chat/completions",
    "errors": [],
    "finish_reason": "stop",
    "http_request_id": "8e928552-bbef-40f4-970a-7512ec4a70bf",
    "http_status": 200,
    "provider": "openai_chat_completions",
    "request_strategy": "tangmao-vision-scan-v1",
    "response_complete": true,
    "response_id": "chatcmpl-1793960829",
    "response_receipt": {
      "byte_length": 1775,
      "omission_reason": null,
      "sha256": "792519e0ff4fb82852d658c95ab9e84eb1931f95277e5be60397769051d3a31d"
    },
    "service_tier": null,
    "transport_mode": "live",
    "usage": {
      "input_reserved_tokens": 8490,
      "model_tokenizer": null,
      "model_usage": {
        "input_tokens": 444,
        "input_tokens_details": {
          "cached_tokens": 0
        },
        "output_tokens": 332,
        "total_tokens": 776
      },
      "model_usage_status": "validated",
      "network_attempts": 1,
      "paid_calls": null,
      "price_usd": null,
      "raw_model_usage": {
        "completion_tokens": 332,
        "prompt_tokens": 444,
        "prompt_tokens_details": {
          "cached_tokens": 0
        },
        "total_tokens": 776
      },
      "real_model_calls": 1,
      "request_utf8_bytes": 7466,
      "reservation_policy": "request_utf8_bytes_plus_1024_v1",
      "reserved_tokens": 16682
    }
  },
  "recognition_request_fingerprint": {
    "sha256": "7a36e53ad64e56068eb747d0d36303c0633be395101e3988dcbee1655fc77689",
    "byte_length": 7466
  },
  "recognition_response_fingerprint": {
    "sha256": "792519e0ff4fb82852d658c95ab9e84eb1931f95277e5be60397769051d3a31d",
    "byte_length": 1775
  },
  "tokens": null,
  "plan_fingerprint": null,
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
      "sha256": "ac225b10685fbd6afb80fe4298787b6c44a11078c874697473becc8f3c54ed3a",
      "byte_length": 590
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
  "files": [],
  "artifacts": {
    ".owner.json": {
      "sha256": "a62c1d12006a1433848ef38b3c3dfa2fc7751010892285e47d4c6055df7f6380",
      "byte_length": 162
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
    "profile.json": {
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
    },
    "recognition.json": {
      "sha256": "0ec4ecdc69b4b38b7b08712d2f390886c72defae984ffc2a6dec0157d0121ad6",
      "byte_length": 242
    },
    "request-body.json": {
      "sha256": "7a36e53ad64e56068eb747d0d36303c0633be395101e3988dcbee1655fc77689",
      "byte_length": 7466
    },
    "request.json": {
      "sha256": "7f9d43dc2fff9532f3d52326efbe74923364d290a52126f7e6e41cb0ced85d21",
      "byte_length": 1445
    },
    "response.bin": {
      "sha256": "792519e0ff4fb82852d658c95ab9e84eb1931f95277e5be60397769051d3a31d",
      "byte_length": 1775
    },
    "source-api-config.json": {
      "sha256": "ac225b10685fbd6afb80fe4298787b6c44a11078c874697473becc8f3c54ed3a",
      "byte_length": 590
    },
    "source-case.json": {
      "sha256": "d4afeedc1da24fa292c204e9b40b8a0271a91f426e1389e5e890af11f2b908aa",
      "byte_length": 842
    },
    "source-semantics.json": {
      "sha256": "3c9ec361aa7fb88a84d108483db2f503b394d137888b595b4eaf83a660196443",
      "byte_length": 845
    }
  }
}
```
