# 选定图像保真预览报告

状态：passed；预览：rendered；知识：pending；科学质量：not_run；发布：false。

仅选定图像和手工结构声明工程核对；不执行自动图像理解；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "knowledge_image",
  "scope": "selected_image_fidelity_v1",
  "request_id": "b429021a-015d-4b2f-bf5e-4d4d553b48a6",
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
    "selection_id": "curve_no_data",
    "title": "图像保真样例 curve_no_data",
    "source_kind": "experimental_curve",
    "image": {
      "file": "source.png",
      "sha256": "a5b038e2beb6d648c4fdeae7043bafa72ff81b9d35e638f624b03cfe809c413c"
    },
    "semantics": {
      "file": "semantics.json",
      "sha256": "cd7cbc028393ee6332c37a4bb0ec4dd6c4091ec5473843c9dc56ef7da84fcfb6"
    },
    "dimensions": [
      960,
      520
    ],
    "readability": "clear"
  },
  "semantics": {
    "schema_version": 1,
    "producer": "author_image_declaration",
    "author": "independent_cc0_fixture_author",
    "complete": true,
    "data_available": false,
    "caption": "Illustrative experimental curve only: original sample values are unavailable.",
    "conditions": [
      "Only the selected schematic relationships are asserted."
    ],
    "exceptions": [
      "No scientific accuracy or measured data is claimed."
    ],
    "elements": [
      {
        "id": "time",
        "kind": "rect",
        "bbox": [
          650,
          350,
          240,
          45
        ],
        "label": "Time",
        "unit": "s",
        "legend_id": null
      },
      {
        "id": "response",
        "kind": "rect",
        "bbox": [
          40,
          40,
          240,
          45
        ],
        "label": "Response",
        "unit": "a.u.",
        "legend_id": null
      }
    ],
    "relations": [],
    "spatial_relations": [],
    "legend": []
  },
  "profile": {
    "schema_version": 1,
    "app": "Obsidian",
    "app_version": "1.15.0",
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
      "IMAGE_CURVE_NO_DATA"
    ],
    "issue_codes": []
  },
  "plan_fingerprint": {
    "sha256": "3e8941795a9de9bed94ea230e86a36aa33d270970816584f9153c02daff1de2c",
    "byte_length": 1858
  },
  "inputs": {
    "original.png": {
      "sha256": "a5b038e2beb6d648c4fdeae7043bafa72ff81b9d35e638f624b03cfe809c413c",
      "byte_length": 16698
    },
    "profile.json": {
      "sha256": "6f58c1a7fc6d4f914be5dae19cc39cb9f43c11c4252a8bc3419b38ebf9c1bf01",
      "byte_length": 143
    },
    "source-case.json": {
      "sha256": "5aa91bdaed4a1149ace2a82bcd2199c4a639f8a23094a522a9a335a08db0b2b4",
      "byte_length": 502
    },
    "source-semantics.json": {
      "sha256": "cd7cbc028393ee6332c37a4bb0ec4dd6c4091ec5473843c9dc56ef7da84fcfb6",
      "byte_length": 904
    }
  },
  "files": [
    {
      "relative_path": "图像预览.md",
      "content_sha256": "c659718ad60c0e1bfa1e6ab3ab19460bcd9cbab4cc2be44f26522ebb6ac0c515",
      "byte_length": 1007,
      "mappings": [
        {
          "kind": "title",
          "source_field": "case.title",
          "start_byte": 35,
          "end_byte": 67
        },
        {
          "kind": "image_embed",
          "source_field": "case.image",
          "start_byte": 84,
          "end_byte": 101
        },
        {
          "kind": "fallback_reason",
          "source_field": "decision.fallback_reasons[0]",
          "start_byte": 111,
          "end_byte": 192
        },
        {
          "kind": "caption",
          "source_field": "semantics.caption",
          "start_byte": 217,
          "end_byte": 294
        },
        {
          "kind": "element_id",
          "source_field": "semantics.elements[0].id",
          "start_byte": 344,
          "end_byte": 348
        },
        {
          "kind": "element_kind",
          "source_field": "semantics.elements[0].kind",
          "start_byte": 373,
          "end_byte": 377
        },
        {
          "kind": "element_label",
          "source_field": "semantics.elements[0].label",
          "start_byte": 402,
          "end_byte": 406
        },
        {
          "kind": "element_unit",
          "source_field": "semantics.elements[0].unit",
          "start_byte": 431,
          "end_byte": 432
        },
        {
          "kind": "element_legend",
          "source_field": "semantics.elements[0].legend_id",
          "start_byte": 457,
          "end_byte": 478
        },
        {
          "kind": "element_geometry",
          "source_field": "semantics.elements[0].bbox",
          "start_byte": 509,
          "end_byte": 525
        },
        {
          "kind": "element_id",
          "source_field": "semantics.elements[1].id",
          "start_byte": 564,
          "end_byte": 572
        },
        {
          "kind": "element_kind",
          "source_field": "semantics.elements[1].kind",
          "start_byte": 597,
          "end_byte": 601
        },
        {
          "kind": "element_label",
          "source_field": "semantics.elements[1].label",
          "start_byte": 626,
          "end_byte": 634
        },
        {
          "kind": "element_unit",
          "source_field": "semantics.elements[1].unit",
          "start_byte": 659,
          "end_byte": 663
        },
        {
          "kind": "element_legend",
          "source_field": "semantics.elements[1].legend_id",
          "start_byte": 688,
          "end_byte": 709
        },
        {
          "kind": "element_geometry",
          "source_field": "semantics.elements[1].bbox",
          "start_byte": 740,
          "end_byte": 754
        },
        {
          "kind": "conditions",
          "source_field": "semantics.conditions[0]",
          "start_byte": 824,
          "end_byte": 879
        },
        {
          "kind": "exceptions",
          "source_field": "semantics.exceptions[0]",
          "start_byte": 904,
          "end_byte": 955
        },
        {
          "kind": "attachment_link",
          "source_field": "case.image",
          "start_byte": 978,
          "end_byte": 1006
        }
      ]
    }
  ],
  "artifacts": {
    ".owner.json": {
      "sha256": "3724c37797d7afef0775b1c787578c9793da0f778e01e5ad1c4652429288a555",
      "byte_length": 148
    },
    "fake.csv": {
      "sha256": "ac743262b699efc4fa5e29f41f1b0d15b7cafdd94d51b3f572c18dac7eefdb40",
      "byte_length": 8
    },
    "original.png": {
      "sha256": "a5b038e2beb6d648c4fdeae7043bafa72ff81b9d35e638f624b03cfe809c413c",
      "byte_length": 16698
    },
    "plan.json": {
      "sha256": "3e8941795a9de9bed94ea230e86a36aa33d270970816584f9153c02daff1de2c",
      "byte_length": 1858
    },
    "profile.json": {
      "sha256": "6f58c1a7fc6d4f914be5dae19cc39cb9f43c11c4252a8bc3419b38ebf9c1bf01",
      "byte_length": 143
    },
    "source-case.json": {
      "sha256": "5aa91bdaed4a1149ace2a82bcd2199c4a639f8a23094a522a9a335a08db0b2b4",
      "byte_length": 502
    },
    "source-semantics.json": {
      "sha256": "cd7cbc028393ee6332c37a4bb0ec4dd6c4091ec5473843c9dc56ef7da84fcfb6",
      "byte_length": 904
    },
    "图像预览.md": {
      "sha256": "c659718ad60c0e1bfa1e6ab3ab19460bcd9cbab4cc2be44f26522ebb6ac0c515",
      "byte_length": 1007
    }
  }
}
```
