# 选定公式保真预览报告

状态：passed；预览：rendered；知识：pending；科学质量：not_run；发布：false。

仅选定公式/原区域工程核对；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "knowledge_formula",
  "scope": "selected_formula_fidelity_v1",
  "request_id": "18624b2a-647f-4e04-85d2-79119dc9d562",
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
    "selection_id": "image_fallback",
    "title": "公式保真样例 image_fallback",
    "source_kind": "tex",
    "tex": {
      "file": "original.tex",
      "sha256": "97c540cf4af7ab32e9791faa662ce3f4e0923b523f16976c516ff156583cbe59"
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
    "app_version": "1.6.7",
    "mathjax_version": "3.2.2",
    "native_math": false,
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
    "sha256": "6739fda93880ae45d71da7fcede831972a083b4cbee3b7631d144de207996cf4",
    "byte_length": 2325
  },
  "inputs": {
    "original.png": {
      "sha256": "ee03e4bb36f52dc0c4eb604b31b751c282fc84539a805562bbc0e4c542ad2e59",
      "byte_length": 6656
    },
    "original.tex": {
      "sha256": "97c540cf4af7ab32e9791faa662ce3f4e0923b523f16976c516ff156583cbe59",
      "byte_length": 28
    },
    "profile.json": {
      "sha256": "d989a8831fd8992e888c2e2744510fe8e69791946937cffaf07b7475dd988345",
      "byte_length": 202
    },
    "source-case.json": {
      "sha256": "d5735a8679ea9292c08e6d3844ae62e52a79ce2fc8ecde7ce49610d86b70bb3a",
      "byte_length": 731
    },
    "source-semantics.json": {
      "sha256": "31e8cfa93a4209e625717daf9aab98249f3b89c4f114101af10a68c946c0cacf",
      "byte_length": 846
    }
  },
  "files": [
    {
      "relative_path": "公式预览.md",
      "content_sha256": "9e4cc624304f2ebcb3bba61af536201e5840ef1c4c6be98d7028671b7b8b134a",
      "byte_length": 1056,
      "mappings": [
        {
          "kind": "title",
          "source_field": "case.title",
          "start_byte": 35,
          "end_byte": 68
        },
        {
          "kind": "image_embed",
          "source_field": "region",
          "start_byte": 85,
          "end_byte": 102
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[0].symbol",
          "start_byte": 154,
          "end_byte": 155
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[0].definition",
          "start_byte": 180,
          "end_byte": 198
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[0].unit",
          "start_byte": 223,
          "end_byte": 229
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[1].symbol",
          "start_byte": 268,
          "end_byte": 269
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[1].definition",
          "start_byte": 294,
          "end_byte": 323
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[1].unit",
          "start_byte": 348,
          "end_byte": 369
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[2].symbol",
          "start_byte": 408,
          "end_byte": 409
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[2].definition",
          "start_byte": 434,
          "end_byte": 446
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[2].unit",
          "start_byte": 471,
          "end_byte": 472
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[3].symbol",
          "start_byte": 511,
          "end_byte": 512
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[3].definition",
          "start_byte": 537,
          "end_byte": 562
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[3].unit",
          "start_byte": 587,
          "end_byte": 593
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[4].symbol",
          "start_byte": 632,
          "end_byte": 633
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[4].definition",
          "start_byte": 658,
          "end_byte": 682
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[4].unit",
          "start_byte": 707,
          "end_byte": 710
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[0]",
          "start_byte": 741,
          "end_byte": 756
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[1]",
          "start_byte": 770,
          "end_byte": 834
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[0]",
          "start_byte": 859,
          "end_byte": 868
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[1]",
          "start_byte": 882,
          "end_byte": 896
        },
        {
          "kind": "exception",
          "source_field": "semantics.exceptions[0]",
          "start_byte": 921,
          "end_byte": 969
        },
        {
          "kind": "attachment_link",
          "source_field": "tex",
          "start_byte": 992,
          "end_byte": 1018
        },
        {
          "kind": "attachment_link",
          "source_field": "region",
          "start_byte": 1020,
          "end_byte": 1054
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "9c8bba6d3eddfd46208aaae55261df2d71aa84e142298ef7b505aa8f606cae27",
      "byte_length": 152
    },
    "original.png": {
      "sha256": "ee03e4bb36f52dc0c4eb604b31b751c282fc84539a805562bbc0e4c542ad2e59",
      "byte_length": 6656
    },
    "original.tex": {
      "sha256": "97c540cf4af7ab32e9791faa662ce3f4e0923b523f16976c516ff156583cbe59",
      "byte_length": 28
    },
    "plan.json": {
      "sha256": "6739fda93880ae45d71da7fcede831972a083b4cbee3b7631d144de207996cf4",
      "byte_length": 2325
    },
    "profile.json": {
      "sha256": "d989a8831fd8992e888c2e2744510fe8e69791946937cffaf07b7475dd988345",
      "byte_length": 202
    },
    "source-case.json": {
      "sha256": "d5735a8679ea9292c08e6d3844ae62e52a79ce2fc8ecde7ce49610d86b70bb3a",
      "byte_length": 731
    },
    "source-semantics.json": {
      "sha256": "31e8cfa93a4209e625717daf9aab98249f3b89c4f114101af10a68c946c0cacf",
      "byte_length": 846
    },
    "公式预览.md": {
      "sha256": "9e4cc624304f2ebcb3bba61af536201e5840ef1c4c6be98d7028671b7b8b134a",
      "byte_length": 1056
    }
  }
}
```
