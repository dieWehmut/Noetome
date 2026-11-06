# 选定公式保真预览报告

状态：passed；预览：rendered；知识：pending；科学质量：not_run；发布：false。

仅选定公式/原区域工程核对；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "knowledge_formula",
  "scope": "selected_formula_fidelity_v1",
  "request_id": "318a333b-22de-4087-bd13-e2795a9acc0a",
  "status": "passed",
  "preview_status": "rendered",
  "knowledge_status": "pending",
  "scientific_quality": "not_run",
  "published": false,
  "network_calls": 0,
  "paid_calls": 0,
  "case": {
    "schema_version": 1,
    "purpose": "formula_fidelity",
    "selection_id": "native",
    "title": "公式保真样例 native",
    "source_kind": "tex",
    "tex": {
      "file": "original.tex",
      "sha256": "4fbab010e8618ed51cea0a2caac1aa469f2c06cbcbdc774f4f5f9bf12294cfdb"
    },
    "semantics": {
      "file": "semantics.json",
      "sha256": "31e8cfa93a4209e625717daf9aab98249f3b89c4f114101af10a68c946c0cacf"
    },
    "region": {
      "file": "region.png",
      "sha256": "ee03e4bb36f52dc0c4eb604b31b751c282fc84539a805562bbc0e4c542ad2e59",
      "page": 1,
      "canvas": [
        1000,
        600
      ],
      "bounds": [
        100,
        120,
        800,
        240
      ],
      "legibility": "clear"
    },
    "data_available": false
  },
  "semantics": {
    "schema_version": 1,
    "expression": "C_i(t) = -\\frac{a}{b} t^{2}",
    "symbols": [
      {
        "symbol": "C",
        "definition": "test concentration",
        "unit": "mmol/L"
      },
      {
        "symbol": "i",
        "definition": "selected integer sample index",
        "unit": null
      },
      {
        "symbol": "t",
        "definition": "elapsed time",
        "unit": "s"
      },
      {
        "symbol": "a",
        "definition": "positive scaling constant",
        "unit": "mmol/L"
      },
      {
        "symbol": "b",
        "definition": "positive nonzero divisor",
        "unit": "s^2"
      }
    ],
    "conditions": [
      "a > 0 and b > 0",
      "This is a fictional engineering example, not a scientific claim."
    ],
    "ranges": [
      "i = 1,2,3",
      "0 <= t <= 10 s"
    ],
    "exceptions": [
      "Do not apply outside the selected time interval."
    ]
  },
  "profile": {
    "schema_version": 1,
    "app": "Obsidian",
    "app_version": "1.15.0",
    "mathjax_version": "4.1.3",
    "native_math": true,
    "compatibility_dfrac": true,
    "local_images": true,
    "verified": true
  },
  "checks": {
    "source_preservation": "passed",
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
  "tokens": [
    "C",
    "_",
    "i",
    "(",
    "t",
    ")",
    "=",
    "-",
    "\\frac",
    "{",
    "a",
    "}",
    "{",
    "b",
    "}",
    "t",
    "^",
    "{",
    "2",
    "}"
  ],
  "plan_fingerprint": {
    "sha256": "827a446edeed6eed4786547c1009ce2239e4e6924713499645764ce06b500742",
    "byte_length": 2293
  },
  "inputs": {
    "original.png": {
      "sha256": "ee03e4bb36f52dc0c4eb604b31b751c282fc84539a805562bbc0e4c542ad2e59",
      "byte_length": 6656
    },
    "original.tex": {
      "sha256": "4fbab010e8618ed51cea0a2caac1aa469f2c06cbcbdc774f4f5f9bf12294cfdb",
      "byte_length": 29
    },
    "profile.json": {
      "sha256": "1be77b79b3d7dfb08c93a44e0cf5d26647cba13344e8df0a4dbc866aa5735fde",
      "byte_length": 202
    },
    "source-case.json": {
      "sha256": "8590f88a83d7d09de05c1c0365656ec021f8e980edca9c57ad709aa7529e1c27",
      "byte_length": 715
    },
    "source-semantics.json": {
      "sha256": "31e8cfa93a4209e625717daf9aab98249f3b89c4f114101af10a68c946c0cacf",
      "byte_length": 846
    }
  },
  "files": [
    {
      "relative_path": "公式预览.md",
      "content_sha256": "a1734a214a7354e4a800832773379c641972a348bcf9baf4b977360a3d691712",
      "byte_length": 1064,
      "mappings": [
        {
          "kind": "title",
          "source_field": "case.title",
          "start_byte": 35,
          "end_byte": 60
        },
        {
          "kind": "expression",
          "source_field": "tex",
          "start_byte": 80,
          "end_byte": 107
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[0].symbol",
          "start_byte": 162,
          "end_byte": 163
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[0].definition",
          "start_byte": 188,
          "end_byte": 206
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[0].unit",
          "start_byte": 231,
          "end_byte": 237
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[1].symbol",
          "start_byte": 276,
          "end_byte": 277
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[1].definition",
          "start_byte": 302,
          "end_byte": 331
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[1].unit",
          "start_byte": 356,
          "end_byte": 377
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[2].symbol",
          "start_byte": 416,
          "end_byte": 417
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[2].definition",
          "start_byte": 442,
          "end_byte": 454
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[2].unit",
          "start_byte": 479,
          "end_byte": 480
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[3].symbol",
          "start_byte": 519,
          "end_byte": 520
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[3].definition",
          "start_byte": 545,
          "end_byte": 570
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[3].unit",
          "start_byte": 595,
          "end_byte": 601
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[4].symbol",
          "start_byte": 640,
          "end_byte": 641
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[4].definition",
          "start_byte": 666,
          "end_byte": 690
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[4].unit",
          "start_byte": 715,
          "end_byte": 718
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[0]",
          "start_byte": 749,
          "end_byte": 764
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[1]",
          "start_byte": 778,
          "end_byte": 842
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[0]",
          "start_byte": 867,
          "end_byte": 876
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[1]",
          "start_byte": 890,
          "end_byte": 904
        },
        {
          "kind": "exception",
          "source_field": "semantics.exceptions[0]",
          "start_byte": 929,
          "end_byte": 977
        },
        {
          "kind": "attachment_link",
          "source_field": "tex",
          "start_byte": 1000,
          "end_byte": 1026
        },
        {
          "kind": "attachment_link",
          "source_field": "region",
          "start_byte": 1028,
          "end_byte": 1062
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "0f66d52d70cdff6f3ee77044fea49bfd315859a8290ebd0903e0f676345734d2",
      "byte_length": 152
    },
    "compiled.tex": {
      "sha256": "ba13467e89656ca236d818e075c4c09dd5dca26dd6398ea03a7ea67dbf618733",
      "byte_length": 27
    },
    "original.png": {
      "sha256": "ee03e4bb36f52dc0c4eb604b31b751c282fc84539a805562bbc0e4c542ad2e59",
      "byte_length": 6656
    },
    "original.tex": {
      "sha256": "4fbab010e8618ed51cea0a2caac1aa469f2c06cbcbdc774f4f5f9bf12294cfdb",
      "byte_length": 29
    },
    "plan.json": {
      "sha256": "827a446edeed6eed4786547c1009ce2239e4e6924713499645764ce06b500742",
      "byte_length": 2293
    },
    "profile.json": {
      "sha256": "1be77b79b3d7dfb08c93a44e0cf5d26647cba13344e8df0a4dbc866aa5735fde",
      "byte_length": 202
    },
    "source-case.json": {
      "sha256": "8590f88a83d7d09de05c1c0365656ec021f8e980edca9c57ad709aa7529e1c27",
      "byte_length": 715
    },
    "source-semantics.json": {
      "sha256": "31e8cfa93a4209e625717daf9aab98249f3b89c4f114101af10a68c946c0cacf",
      "byte_length": 846
    },
    "公式预览.md": {
      "sha256": "a1734a214a7354e4a800832773379c641972a348bcf9baf4b977360a3d691712",
      "byte_length": 1064
    }
  }
}
```
