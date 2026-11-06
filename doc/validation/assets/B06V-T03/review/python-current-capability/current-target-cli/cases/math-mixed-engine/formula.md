# 选定公式保真预览报告

状态：passed；预览：rendered；知识：pending；科学质量：not_run；发布：false。

仅选定公式/原区域工程核对；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "knowledge_formula",
  "scope": "selected_formula_fidelity_v1",
  "request_id": "489d76c4-2332-418f-817a-61034bed636d",
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
    "mathjax_version": "3.2.2",
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
    "representation": "formula_image",
    "fallback_reasons": [
      "FORMULA_NATIVE_UNAVAILABLE"
    ],
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
    "sha256": "d777d521d28bfb73a0a90858b651ad6e0676e894f0ad9b3e1cad3c96ce21a21b",
    "byte_length": 2309
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
      "sha256": "649cd750cd6ddc7ae81097df47c12209bb6f3b11846b04c6251922695bb5fa53",
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
      "content_sha256": "755d1d69ee84adae1390e5e9a49276214de9769ba674a26e69332767d4d76e8a",
      "byte_length": 1048,
      "mappings": [
        {
          "kind": "title",
          "source_field": "case.title",
          "start_byte": 35,
          "end_byte": 60
        },
        {
          "kind": "image_embed",
          "source_field": "region",
          "start_byte": 77,
          "end_byte": 94
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[0].symbol",
          "start_byte": 146,
          "end_byte": 147
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[0].definition",
          "start_byte": 172,
          "end_byte": 190
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[0].unit",
          "start_byte": 215,
          "end_byte": 221
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[1].symbol",
          "start_byte": 260,
          "end_byte": 261
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[1].definition",
          "start_byte": 286,
          "end_byte": 315
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[1].unit",
          "start_byte": 340,
          "end_byte": 361
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[2].symbol",
          "start_byte": 400,
          "end_byte": 401
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[2].definition",
          "start_byte": 426,
          "end_byte": 438
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[2].unit",
          "start_byte": 463,
          "end_byte": 464
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[3].symbol",
          "start_byte": 503,
          "end_byte": 504
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[3].definition",
          "start_byte": 529,
          "end_byte": 554
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[3].unit",
          "start_byte": 579,
          "end_byte": 585
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[4].symbol",
          "start_byte": 624,
          "end_byte": 625
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[4].definition",
          "start_byte": 650,
          "end_byte": 674
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[4].unit",
          "start_byte": 699,
          "end_byte": 702
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[0]",
          "start_byte": 733,
          "end_byte": 748
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[1]",
          "start_byte": 762,
          "end_byte": 826
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[0]",
          "start_byte": 851,
          "end_byte": 860
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[1]",
          "start_byte": 874,
          "end_byte": 888
        },
        {
          "kind": "exception",
          "source_field": "semantics.exceptions[0]",
          "start_byte": 913,
          "end_byte": 961
        },
        {
          "kind": "attachment_link",
          "source_field": "tex",
          "start_byte": 984,
          "end_byte": 1010
        },
        {
          "kind": "attachment_link",
          "source_field": "region",
          "start_byte": 1012,
          "end_byte": 1046
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "e9f5d5ca0baf77ebb1fcf8c15eb7c2f01e32b0e6196c09566040eb84c6a85c89",
      "byte_length": 152
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
      "sha256": "d777d521d28bfb73a0a90858b651ad6e0676e894f0ad9b3e1cad3c96ce21a21b",
      "byte_length": 2309
    },
    "profile.json": {
      "sha256": "649cd750cd6ddc7ae81097df47c12209bb6f3b11846b04c6251922695bb5fa53",
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
      "sha256": "755d1d69ee84adae1390e5e9a49276214de9769ba674a26e69332767d4d76e8a",
      "byte_length": 1048
    }
  }
}
```
