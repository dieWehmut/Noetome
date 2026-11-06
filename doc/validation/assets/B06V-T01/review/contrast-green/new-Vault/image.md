# 选定图像保真预览报告

状态：passed；预览：rendered；知识：pending；科学质量：not_run；发布：false。

仅选定图像和手工结构声明工程核对；不执行自动图像理解；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "knowledge_image",
  "scope": "selected_image_fidelity_v1",
  "request_id": "6fd29fb0-14e2-42f4-83f7-a3d297cee049",
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
    "representation": "original_image",
    "fallback_reasons": [
      "IMAGE_COLOR_CONTRAST"
    ],
    "issue_codes": []
  },
  "plan_fingerprint": {
    "sha256": "c245efd23fba3a4217b19a82424d05df2b5d3d5095afb34bbec5df6d4961b122",
    "byte_length": 1680
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
      "content_sha256": "0e0d5a4f4f257304cb59c2029d2cd32b2b881e08fcc13fd0bdd5ac26d69809a3",
      "byte_length": 848,
      "mappings": [
        {
          "kind": "title",
          "source_field": "case.title",
          "start_byte": 35,
          "end_byte": 64
        },
        {
          "kind": "image_embed",
          "source_field": "case.image",
          "start_byte": 81,
          "end_byte": 98
        },
        {
          "kind": "fallback_reason",
          "source_field": "decision.fallback_reasons[0]",
          "start_byte": 108,
          "end_byte": 189
        },
        {
          "kind": "caption",
          "source_field": "semantics.caption",
          "start_byte": 214,
          "end_byte": 267
        },
        {
          "kind": "element_id",
          "source_field": "semantics.elements[0].id",
          "start_byte": 317,
          "end_byte": 324
        },
        {
          "kind": "element_kind",
          "source_field": "semantics.elements[0].kind",
          "start_byte": 349,
          "end_byte": 353
        },
        {
          "kind": "element_label",
          "source_field": "semantics.elements[0].label",
          "start_byte": 378,
          "end_byte": 385
        },
        {
          "kind": "element_unit",
          "source_field": "semantics.elements[0].unit",
          "start_byte": 410,
          "end_byte": 412
        },
        {
          "kind": "element_legend",
          "source_field": "semantics.elements[0].legend_id",
          "start_byte": 437,
          "end_byte": 441
        },
        {
          "kind": "element_geometry",
          "source_field": "semantics.elements[0].bbox",
          "start_byte": 472,
          "end_byte": 486
        },
        {
          "kind": "legend_id",
          "source_field": "semantics.legend[0].id",
          "start_byte": 564,
          "end_byte": 568
        },
        {
          "kind": "legend_label",
          "source_field": "semantics.legend[0].label",
          "start_byte": 593,
          "end_byte": 597
        },
        {
          "kind": "legend_meaning",
          "source_field": "semantics.legend[0].meaning",
          "start_byte": 622,
          "end_byte": 638
        },
        {
          "kind": "legend_color",
          "source_field": "semantics.legend[0].color",
          "start_byte": 663,
          "end_byte": 670
        },
        {
          "kind": "legend_geometry",
          "source_field": "semantics.legend[0].bbox",
          "start_byte": 701,
          "end_byte": 716
        },
        {
          "kind": "attachment_link",
          "source_field": "case.image",
          "start_byte": 819,
          "end_byte": 847
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "3923780b2cff6462db3f6d8292860f933adfa536e0c3010681de2e4c0d328bf6",
      "byte_length": 148
    },
    "original.png": {
      "sha256": "7ad9fbb707479a45fe75ff62a0c323588f6c3ad445aff6e3ce1c466ef5ecfcad",
      "byte_length": 4444
    },
    "plan.json": {
      "sha256": "c245efd23fba3a4217b19a82424d05df2b5d3d5095afb34bbec5df6d4961b122",
      "byte_length": 1680
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
    },
    "图像预览.md": {
      "sha256": "0e0d5a4f4f257304cb59c2029d2cd32b2b881e08fcc13fd0bdd5ac26d69809a3",
      "byte_length": 848
    }
  }
}
```
