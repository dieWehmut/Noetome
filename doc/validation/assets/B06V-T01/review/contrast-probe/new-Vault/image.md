# 选定图像保真预览报告

状态：passed；预览：rendered；知识：pending；科学质量：not_run；发布：false。

仅选定图像和手工结构声明工程核对；不执行自动图像理解；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "knowledge_image",
  "scope": "selected_image_fidelity_v1",
  "request_id": "256c0700-a9ea-496b-b657-0f0f36a2a88f",
  "status": "passed",
  "preview_status": "rendered",
  "knowledge_status": "pending",
  "scientific_quality": "not_run",
  "published": false,
  "network_calls": 0,
  "paid_calls": 0,
  "case": {
    "schema_version": 1,
    "purpose": "image_fidelity",
    "selection_id": "contrast_probe",
    "title": "Fresh contrast counterexample",
    "source_kind": "diagram",
    "image": {
      "file": "source.png",
      "sha256": "7ad9fbb707479a45fe75ff62a0c323588f6c3ad445aff6e3ce1c466ef5ecfcad"
    },
    "semantics": {
      "file": "semantics.json",
      "sha256": "b3bfe628c851e6e48de826f811e16f5c852a90fe024e504d941c65532e84f100"
    },
    "dimensions": [
      400,
      250
    ],
    "readability": "clear"
  },
  "semantics": {
    "schema_version": 1,
    "producer": "author_image_declaration",
    "author": "fresh_review_self_authored",
    "complete": true,
    "data_available": false,
    "caption": "Readable black source boxes with white source labels.",
    "conditions": [],
    "exceptions": [],
    "elements": [
      {
        "id": "payload",
        "kind": "rect",
        "bbox": [
          20,
          20,
          180,
          80
        ],
        "label": "Payload",
        "unit": "mg",
        "legend_id": "dark"
      }
    ],
    "relations": [],
    "spatial_relations": [],
    "legend": [
      {
        "id": "dark",
        "label": "Dark",
        "meaning": "Selected payload",
        "color": "#000000",
        "bbox": [
          20,
          140,
          300,
          80
        ]
      }
    ]
  },
  "profile": {
    "schema_version": 1,
    "app": "Obsidian",
    "app_version": "1.6.7",
    "svg_supported": true,
    "local_images": true,
    "verified": true
  },
  "checks": {
    "source_preservation": "passed",
    "author_declaration": "passed",
    "geometry_review": "passed",
    "semantic_review": "passed",
    "note_plan": "passed",
    "rendering": "passed",
    "saved_output": "passed"
  },
  "decision": {
    "representation": "svg_redraw",
    "fallback_reasons": [],
    "issue_codes": []
  },
  "plan_fingerprint": {
    "sha256": "839b34d91128018fdace80a92abe68251858e7ee19681eadf9a0984c61756871",
    "byte_length": 1642
  },
  "inputs": {
    "original.png": {
      "sha256": "7ad9fbb707479a45fe75ff62a0c323588f6c3ad445aff6e3ce1c466ef5ecfcad",
      "byte_length": 4444
    },
    "profile.json": {
      "sha256": "ca6ddf79660ab307434f7b6add58fcc8371b2127b057798b3b39034c408ee9ce",
      "byte_length": 142
    },
    "source-case.json": {
      "sha256": "6c670e4a3486e4537db8e31c624759d70fe4a8ac7b4299c8f05a1d3428aa41a5",
      "byte_length": 488
    },
    "source-semantics.json": {
      "sha256": "b3bfe628c851e6e48de826f811e16f5c852a90fe024e504d941c65532e84f100",
      "byte_length": 745
    }
  },
  "files": [
    {
      "relative_path": "图像预览.md",
      "content_sha256": "47623278f3a2691631250dc65c769c0058271b5a13a2fa664fe34aaa2ee15d42",
      "byte_length": 752,
      "mappings": [
        {
          "kind": "title",
          "source_field": "case.title",
          "start_byte": 35,
          "end_byte": 64
        },
        {
          "kind": "image_embed",
          "source_field": "semantics",
          "start_byte": 81,
          "end_byte": 97
        },
        {
          "kind": "caption",
          "source_field": "semantics.caption",
          "start_byte": 118,
          "end_byte": 171
        },
        {
          "kind": "element_id",
          "source_field": "semantics.elements[0].id",
          "start_byte": 221,
          "end_byte": 228
        },
        {
          "kind": "element_kind",
          "source_field": "semantics.elements[0].kind",
          "start_byte": 253,
          "end_byte": 257
        },
        {
          "kind": "element_label",
          "source_field": "semantics.elements[0].label",
          "start_byte": 282,
          "end_byte": 289
        },
        {
          "kind": "element_unit",
          "source_field": "semantics.elements[0].unit",
          "start_byte": 314,
          "end_byte": 316
        },
        {
          "kind": "element_legend",
          "source_field": "semantics.elements[0].legend_id",
          "start_byte": 341,
          "end_byte": 345
        },
        {
          "kind": "element_geometry",
          "source_field": "semantics.elements[0].bbox",
          "start_byte": 376,
          "end_byte": 390
        },
        {
          "kind": "legend_id",
          "source_field": "semantics.legend[0].id",
          "start_byte": 468,
          "end_byte": 472
        },
        {
          "kind": "legend_label",
          "source_field": "semantics.legend[0].label",
          "start_byte": 497,
          "end_byte": 501
        },
        {
          "kind": "legend_meaning",
          "source_field": "semantics.legend[0].meaning",
          "start_byte": 526,
          "end_byte": 542
        },
        {
          "kind": "legend_color",
          "source_field": "semantics.legend[0].color",
          "start_byte": 567,
          "end_byte": 574
        },
        {
          "kind": "legend_geometry",
          "source_field": "semantics.legend[0].bbox",
          "start_byte": 605,
          "end_byte": 620
        },
        {
          "kind": "attachment_link",
          "source_field": "case.image",
          "start_byte": 723,
          "end_byte": 751
        }
      ]
    },
    {
      "relative_path": "redrawn.svg",
      "content_sha256": "a8d565a009836d9066b8137cc60ad871254cae32203d95d4d97d6f3e88ae0fc5",
      "byte_length": 986,
      "mappings": [
        {
          "kind": "canvas_background",
          "source_field": "case.dimensions",
          "start_byte": 88,
          "end_byte": 165
        },
        {
          "kind": "element_shape",
          "source_field": "semantics.elements[0].bbox",
          "start_byte": 166,
          "end_byte": 281
        },
        {
          "kind": "element_label",
          "source_field": "semantics.elements[0].label",
          "start_byte": 282,
          "end_byte": 428
        },
        {
          "kind": "element_unit",
          "source_field": "semantics.elements[0].unit",
          "start_byte": 429,
          "end_byte": 569
        },
        {
          "kind": "legend_swatch",
          "source_field": "semantics.legend[0].bbox",
          "start_byte": 570,
          "end_byte": 682
        },
        {
          "kind": "legend_label",
          "source_field": "semantics.legend[0].label",
          "start_byte": 683,
          "end_byte": 823
        },
        {
          "kind": "legend_meaning",
          "source_field": "semantics.legend[0].meaning",
          "start_byte": 824,
          "end_byte": 978
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "898c2f700ca6fc42f560584fd27f5b45e1048f0c505e2f615e79c69ef2f473be",
      "byte_length": 148
    },
    "original.png": {
      "sha256": "7ad9fbb707479a45fe75ff62a0c323588f6c3ad445aff6e3ce1c466ef5ecfcad",
      "byte_length": 4444
    },
    "plan.json": {
      "sha256": "839b34d91128018fdace80a92abe68251858e7ee19681eadf9a0984c61756871",
      "byte_length": 1642
    },
    "profile.json": {
      "sha256": "ca6ddf79660ab307434f7b6add58fcc8371b2127b057798b3b39034c408ee9ce",
      "byte_length": 142
    },
    "redrawn.svg": {
      "sha256": "a8d565a009836d9066b8137cc60ad871254cae32203d95d4d97d6f3e88ae0fc5",
      "byte_length": 986
    },
    "source-case.json": {
      "sha256": "6c670e4a3486e4537db8e31c624759d70fe4a8ac7b4299c8f05a1d3428aa41a5",
      "byte_length": 488
    },
    "source-semantics.json": {
      "sha256": "b3bfe628c851e6e48de826f811e16f5c852a90fe024e504d941c65532e84f100",
      "byte_length": 745
    },
    "图像预览.md": {
      "sha256": "47623278f3a2691631250dc65c769c0058271b5a13a2fa664fe34aaa2ee15d42",
      "byte_length": 752
    }
  }
}
```
