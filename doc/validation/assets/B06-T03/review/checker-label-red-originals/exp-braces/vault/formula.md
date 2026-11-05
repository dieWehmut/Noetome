# 选定公式保真预览报告

状态：passed；预览：rendered；知识：pending；科学质量：not_run；发布：false。

仅选定公式/原区域工程核对；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "knowledge_formula",
  "scope": "selected_formula_fidelity_v1",
  "request_id": "8e43ad8a-6483-45c2-854b-ca1b37687946",
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
    "selection_id": "native_exp",
    "title": "公式保真样例 native_exp",
    "source_kind": "tex",
    "tex": {
      "file": "original.tex",
      "sha256": "658aed26eb6d71d92775ec0e85a16cb16d377e06e62c4c3f97bb5b0f842712ce"
    },
    "semantics": {
      "file": "semantics.json",
      "sha256": "841c0935473be2ee124ebdd41de8f53f396e59fe391751847d6ef5e7224c4efa"
    },
    "region": {
      "file": "region.png",
      "sha256": "a30de55f74c6dccda0e3dabcc064236002d11a772429cf77e69d0da0c67a6a47",
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
    "expression": "C_i(t)=C_0\\exp{-k t}",
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
        "symbol": "k",
        "definition": "positive decay coefficient",
        "unit": "1/s"
      }
    ],
    "conditions": [
      "k > 0"
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
    "C",
    "_",
    "0",
    "\\exp",
    "{",
    "-",
    "k",
    "t",
    "}"
  ],
  "plan_fingerprint": {
    "sha256": "bb2d0ae5c0ca567c4cb79fb6f32d13a6f464e6afa32028876826298de20e8c41",
    "byte_length": 2052
  },
  "inputs": {
    "original.png": {
      "sha256": "a30de55f74c6dccda0e3dabcc064236002d11a772429cf77e69d0da0c67a6a47",
      "byte_length": 7885
    },
    "original.tex": {
      "sha256": "658aed26eb6d71d92775ec0e85a16cb16d377e06e62c4c3f97bb5b0f842712ce",
      "byte_length": 21
    },
    "profile.json": {
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
    },
    "source-case.json": {
      "sha256": "24ac9891f51663ecf9bf8a506f0bf4ab975ff0d28f12422882ca3adb5a2efb12",
      "byte_length": 723
    },
    "source-semantics.json": {
      "sha256": "841c0935473be2ee124ebdd41de8f53f396e59fe391751847d6ef5e7224c4efa",
      "byte_length": 653
    }
  },
  "files": [
    {
      "relative_path": "公式预览.md",
      "content_sha256": "11202a7aeb77e978960f2b27e3f415908b533816c42caf8748bd69a17c30061f",
      "byte_length": 854,
      "mappings": [
        {
          "kind": "title",
          "source_field": "case.title",
          "start_byte": 35,
          "end_byte": 64
        },
        {
          "kind": "expression",
          "source_field": "tex",
          "start_byte": 84,
          "end_byte": 104
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[0].symbol",
          "start_byte": 159,
          "end_byte": 160
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[0].definition",
          "start_byte": 185,
          "end_byte": 203
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[0].unit",
          "start_byte": 228,
          "end_byte": 234
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[1].symbol",
          "start_byte": 273,
          "end_byte": 274
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[1].definition",
          "start_byte": 299,
          "end_byte": 328
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[1].unit",
          "start_byte": 353,
          "end_byte": 374
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[2].symbol",
          "start_byte": 413,
          "end_byte": 414
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[2].definition",
          "start_byte": 439,
          "end_byte": 451
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[2].unit",
          "start_byte": 476,
          "end_byte": 477
        },
        {
          "kind": "symbol",
          "source_field": "semantics.symbols[3].symbol",
          "start_byte": 516,
          "end_byte": 517
        },
        {
          "kind": "definition",
          "source_field": "semantics.symbols[3].definition",
          "start_byte": 542,
          "end_byte": 568
        },
        {
          "kind": "unit",
          "source_field": "semantics.symbols[3].unit",
          "start_byte": 593,
          "end_byte": 596
        },
        {
          "kind": "condition",
          "source_field": "semantics.conditions[0]",
          "start_byte": 627,
          "end_byte": 632
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[0]",
          "start_byte": 657,
          "end_byte": 666
        },
        {
          "kind": "range",
          "source_field": "semantics.ranges[1]",
          "start_byte": 680,
          "end_byte": 694
        },
        {
          "kind": "exception",
          "source_field": "semantics.exceptions[0]",
          "start_byte": 719,
          "end_byte": 767
        },
        {
          "kind": "attachment_link",
          "source_field": "tex",
          "start_byte": 790,
          "end_byte": 816
        },
        {
          "kind": "attachment_link",
          "source_field": "region",
          "start_byte": 818,
          "end_byte": 852
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "8f5a13d4a63c9a4db75ae524e5350b0d0c92e1222447a498405fdd9cba0111dd",
      "byte_length": 152
    },
    "compiled.tex": {
      "sha256": "5a4df9bb35a65d7dee6096a51016d3350b8f07dfcfbc31784fe0b6c174fc1f30",
      "byte_length": 20
    },
    "original.png": {
      "sha256": "a30de55f74c6dccda0e3dabcc064236002d11a772429cf77e69d0da0c67a6a47",
      "byte_length": 7885
    },
    "original.tex": {
      "sha256": "658aed26eb6d71d92775ec0e85a16cb16d377e06e62c4c3f97bb5b0f842712ce",
      "byte_length": 21
    },
    "plan.json": {
      "sha256": "bb2d0ae5c0ca567c4cb79fb6f32d13a6f464e6afa32028876826298de20e8c41",
      "byte_length": 2052
    },
    "profile.json": {
      "sha256": "2025dadc2bd5d6a3ab508ff3b3d10e62330d3f1ad0e5571841ae29e86c00b553",
      "byte_length": 201
    },
    "source-case.json": {
      "sha256": "24ac9891f51663ecf9bf8a506f0bf4ab975ff0d28f12422882ca3adb5a2efb12",
      "byte_length": 723
    },
    "source-semantics.json": {
      "sha256": "841c0935473be2ee124ebdd41de8f53f396e59fe391751847d6ef5e7224c4efa",
      "byte_length": 653
    },
    "公式预览.md": {
      "sha256": "11202a7aeb77e978960f2b27e3f415908b533816c42caf8748bd69a17c30061f",
      "byte_length": 854
    }
  }
}
```
