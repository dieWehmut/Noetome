# 选定公式保真预览报告

状态：passed；预览：rendered；知识：pending；科学质量：not_run；发布：false。

仅选定公式/原区域工程核对；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "knowledge_formula",
  "scope": "selected_formula_fidelity_v1",
  "request_id": "92e1a481-4b5e-4f73-bb86-726fc02295c5",
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
    "selection_id": "native_relational",
    "title": "公式保真样例 native_relational",
    "source_kind": "tex",
    "tex": {
      "file": "original.tex",
      "sha256": "55b84085a1bea5fee1a82ce64ea445a6e06c9a96e93c017d1318338924281823"
    },
    "semantics": {
      "file": "semantics.json",
      "sha256": "433a352a60fb55564a99308a1626706723aaf5a81ecf0b8cf36fd2df9fa6b118"
    },
    "region": {
      "file": "region.png",
      "sha256": "d9661ee17a8985a627c1906093273ac4de8be5322d9c6df97889152c1aff0919",
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
    "expression": "x_{i}^{2} \\le \\frac{a}{b}",
    "symbols": [
      {
        "symbol": "x",
        "definition": "test coordinate",
        "unit": "mm"
      },
      {
        "symbol": "i",
        "definition": "selected integer sample index",
        "unit": null
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
    "x",
    "_",
    "{",
    "i",
    "}",
    "^",
    "{",
    "2",
    "}",
    "\\le",
    "\\frac",
    "{",
    "a",
    "}",
    "{",
    "b",
    "}"
  ],
  "plan_fingerprint": {
    "sha256": "014fe2674bfcab1cea75266e30dccb7156c471356f99839238c813510d190c18",
    "byte_length": 2184
  },
  "inputs": {
    "original.png": {
      "sha256": "d9661ee17a8985a627c1906093273ac4de8be5322d9c6df97889152c1aff0919",
      "byte_length": 5274
    },
    "original.tex": {
      "sha256": "55b84085a1bea5fee1a82ce64ea445a6e06c9a96e93c017d1318338924281823",
      "byte_length": 26
    },
    "profile.json": {
      "sha256": "1be77b79b3d7dfb08c93a44e0cf5d26647cba13344e8df0a4dbc866aa5735fde",
      "byte_length": 202
    },
    "source-case.json": {
      "sha256": "43aaacdbb4545eb0c4a34f3037fc2ff3850f4555f1c97c9db94ecb722061b8ac",
      "byte_length": 737
    },
    "source-semantics.json": {
      "sha256": "433a352a60fb55564a99308a1626706723aaf5a81ecf0b8cf36fd2df9fa6b118",
      "byte_length": 750
    }
  },
  "files": [
    {
      "relative_path": "公式预览.md",
      "content_sha256": "4f899d186f26629ac210d219d684414143208bd563ac167365a300e827747a87",
      "byte_length": 963,
      "mappings": [
        {
          "kind": "title",
          "source_field": "case.title",
          "start_byte": 35,
          "end_byte": 71
        },
        {
          "kind": "expression",
          "source_field": "tex",
          "start_byte": 91,
          "end_byte": 116
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[0].symbol",
          "start_byte": 171,
          "end_byte": 172
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[0].definition",
          "start_byte": 197,
          "end_byte": 212
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[0].unit",
          "start_byte": 237,
          "end_byte": 239
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[1].symbol",
          "start_byte": 278,
          "end_byte": 279
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[1].definition",
          "start_byte": 304,
          "end_byte": 333
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[1].unit",
          "start_byte": 358,
          "end_byte": 379
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[2].symbol",
          "start_byte": 418,
          "end_byte": 419
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[2].definition",
          "start_byte": 444,
          "end_byte": 469
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[2].unit",
          "start_byte": 494,
          "end_byte": 500
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[3].symbol",
          "start_byte": 539,
          "end_byte": 540
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[3].definition",
          "start_byte": 565,
          "end_byte": 589
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[3].unit",
          "start_byte": 614,
          "end_byte": 617
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[0]",
          "start_byte": 648,
          "end_byte": 663
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[1]",
          "start_byte": 677,
          "end_byte": 741
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[0]",
          "start_byte": 766,
          "end_byte": 775
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[1]",
          "start_byte": 789,
          "end_byte": 803
        },
        {
          "kind": "exception",
          "source_field": "semantics.exceptions[0]",
          "start_byte": 828,
          "end_byte": 876
        },
        {
          "kind": "attachment_link",
          "source_field": "tex",
          "start_byte": 899,
          "end_byte": 925
        },
        {
          "kind": "attachment_link",
          "source_field": "region",
          "start_byte": 927,
          "end_byte": 961
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "937a3f9cfd303cf8a684861f9ee54e6358edcde3f6d649648f7b0a818718b838",
      "byte_length": 152
    },
    "compiled.tex": {
      "sha256": "f4e11d77de293cbf8106a47ee4b94d72d58cdd933c577024e67dff1b19439af1",
      "byte_length": 25
    },
    "original.png": {
      "sha256": "d9661ee17a8985a627c1906093273ac4de8be5322d9c6df97889152c1aff0919",
      "byte_length": 5274
    },
    "original.tex": {
      "sha256": "55b84085a1bea5fee1a82ce64ea445a6e06c9a96e93c017d1318338924281823",
      "byte_length": 26
    },
    "plan.json": {
      "sha256": "014fe2674bfcab1cea75266e30dccb7156c471356f99839238c813510d190c18",
      "byte_length": 2184
    },
    "profile.json": {
      "sha256": "1be77b79b3d7dfb08c93a44e0cf5d26647cba13344e8df0a4dbc866aa5735fde",
      "byte_length": 202
    },
    "source-case.json": {
      "sha256": "43aaacdbb4545eb0c4a34f3037fc2ff3850f4555f1c97c9db94ecb722061b8ac",
      "byte_length": 737
    },
    "source-semantics.json": {
      "sha256": "433a352a60fb55564a99308a1626706723aaf5a81ecf0b8cf36fd2df9fa6b118",
      "byte_length": 750
    },
    "公式预览.md": {
      "sha256": "4f899d186f26629ac210d219d684414143208bd563ac167365a300e827747a87",
      "byte_length": 963
    }
  }
}
```
