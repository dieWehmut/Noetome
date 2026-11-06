# 有据流程表达预览

状态：passed；预览：rendered；知识状态：pending；发布：false。

科学质量：not_run；研究：not_requested；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "knowledge_flow",
  "scope": "active_flow_v1",
  "request_id": "17a7b584-3858-4ed0-a9c9-1bba8395004d",
  "binding": {
    "project_id": "3080d012-8501-43d3-985d-9099a8d7f550",
    "import_id": "a1c24b4d-2161-4c89-80db-f8bc2e4fe042",
    "source_id": "e655470e-e255-41e8-8b29-413cb3e4a0ae",
    "version_id": "20bff8a2-145c-468a-9607-5e8b2c2abf80",
    "content_sha256": "1a662ed525031a2b69b8c72db6e88965eea4c3b9c61bd1348dd8f87d654f9491",
    "normalized_sha256": "1a662ed525031a2b69b8c72db6e88965eea4c3b9c61bd1348dd8f87d654f9491",
    "parse_id": null,
    "manifest_sha256": null
  },
  "status": "passed",
  "preview_status": "rendered",
  "knowledge_status": "pending",
  "scientific_quality": "not_run",
  "research_status": "not_requested",
  "published": false,
  "network_calls": 0,
  "paid_calls": 0,
  "checks": {
    "source_review": "passed",
    "model_support": "passed",
    "note_plan": "passed",
    "rendering": "passed",
    "saved_output": "passed"
  },
  "issue_codes": [],
  "issues": [],
  "process_document": {
    "process": {
      "title": "样例签名分流",
      "entry": "s1",
      "goals": [
        "s2",
        "s3"
      ],
      "conditions": [
        "演示模式开启"
      ],
      "exceptions": [
        "资料不可读时暂停"
      ]
    },
    "steps": [
      {
        "id": "d1",
        "kind": "decision",
        "label": "检查签名",
        "description": "只判断签名是否完整",
        "criterion": "签名完整",
        "inputs": [
          "已接收卡"
        ],
        "outputs": [
          "签名判定结果"
        ],
        "conditions": [
          "卡片已接收"
        ],
        "exceptions": [
          "签名区域不可读时暂停判定"
        ]
      },
      {
        "id": "s1",
        "kind": "step",
        "label": "接收卡片",
        "description": "接收待判断的样例卡",
        "criterion": null,
        "inputs": [
          "待判断卡"
        ],
        "outputs": [
          "已接收卡"
        ],
        "conditions": [
          "卡片可读"
        ],
        "exceptions": [
          "卡片破损时暂停"
        ]
      },
      {
        "id": "s2",
        "kind": "step",
        "label": "进入归档队列",
        "description": "将真分支卡片加入归档队列",
        "criterion": null,
        "inputs": [
          "判据为真的卡片"
        ],
        "outputs": [
          "待归档卡"
        ],
        "conditions": [
          "签名完整"
        ],
        "exceptions": [
          "队列不可用时保留卡片"
        ]
      },
      {
        "id": "s3",
        "kind": "step",
        "label": "进入补签队列",
        "description": "将假分支卡片加入补签队列",
        "criterion": null,
        "inputs": [
          "判据为假的卡片"
        ],
        "outputs": [
          "待补签卡"
        ],
        "conditions": [
          "签名完整这一判据为假"
        ],
        "exceptions": [
          "队列不可用时保留卡片"
        ]
      }
    ],
    "relations": [
      {
        "id": "r1",
        "left": "s1",
        "right": "d1",
        "kind": "sequence",
        "branch_outcome": null,
        "conditions": [
          "接收完成"
        ],
        "exceptions": [
          "破损卡不进入判定"
        ]
      },
      {
        "id": "r2",
        "left": "d1",
        "right": "s2",
        "kind": "branch",
        "branch_outcome": true,
        "conditions": [
          "签名区域可读"
        ],
        "exceptions": [
          "归档队列不可用时暂停"
        ]
      },
      {
        "id": "r3",
        "left": "d1",
        "right": "s3",
        "kind": "branch",
        "branch_outcome": false,
        "conditions": [
          "签名区域可读"
        ],
        "exceptions": [
          "补签队列不可用时暂停"
        ]
      }
    ],
    "evidence": [
      {
        "object_id": "d1",
        "start_byte": 336,
        "end_byte": 577,
        "quote": "判定 d1：\"检查签名\"。判据：\"签名完整\"。说明：\"只判断签名是否完整\"。输入：[\"已接收卡\"]。输出：[\"签名判定结果\"]。条件：[\"卡片已接收\"]。例外：[\"签名区域不可读时暂停判定\"]。"
      },
      {
        "object_id": "process",
        "start_byte": 0,
        "end_byte": 143,
        "quote": "过程：\"样例签名分流\"。入口：s1。目标：[\"s2\",\"s3\"]。条件：[\"演示模式开启\"]。例外：[\"资料不可读时暂停\"]。"
      },
      {
        "object_id": "r1",
        "start_byte": 1046,
        "end_byte": 1150,
        "quote": "关系 r1：s1 完成后执行 d1。条件：[\"接收完成\"]。例外：[\"破损卡不进入判定\"]。"
      },
      {
        "object_id": "r2",
        "start_byte": 1151,
        "end_byte": 1276,
        "quote": "关系 r2：d1 的判据为真时执行 s2。条件：[\"签名区域可读\"]。例外：[\"归档队列不可用时暂停\"]。"
      },
      {
        "object_id": "r3",
        "start_byte": 1277,
        "end_byte": 1402,
        "quote": "关系 r3：d1 的判据为假时执行 s3。条件：[\"签名区域可读\"]。例外：[\"补签队列不可用时暂停\"]。"
      },
      {
        "object_id": "s1",
        "start_byte": 144,
        "end_byte": 335,
        "quote": "步骤 s1：\"接收卡片\"。说明：\"接收待判断的样例卡\"。输入：[\"待判断卡\"]。输出：[\"已接收卡\"]。条件：[\"卡片可读\"]。例外：[\"卡片破损时暂停\"]。"
      },
      {
        "object_id": "s2",
        "start_byte": 578,
        "end_byte": 802,
        "quote": "步骤 s2：\"进入归档队列\"。说明：\"将真分支卡片加入归档队列\"。输入：[\"判据为真的卡片\"]。输出：[\"待归档卡\"]。条件：[\"签名完整\"]。例外：[\"队列不可用时保留卡片\"]。"
      },
      {
        "object_id": "s3",
        "start_byte": 803,
        "end_byte": 1045,
        "quote": "步骤 s3：\"进入补签队列\"。说明：\"将假分支卡片加入补签队列\"。输入：[\"判据为假的卡片\"]。输出：[\"待补签卡\"]。条件：[\"签名完整这一判据为假\"]。例外：[\"队列不可用时保留卡片\"]。"
      }
    ]
  },
  "profile": {
    "schema_version": 2,
    "producer": "flow_render_profile",
    "profile_id": "obsidian_1_15_0_pele_flowchart",
    "app": "Obsidian",
    "app_version": "1.15.0",
    "mermaid_version": null,
    "renderer": {
      "name": "pele",
      "version": "1.8.0",
      "source_sha256": "9bf34b3a9a19351dc69d0d238e365560331a8ae392c41656df7f5090653253d0"
    },
    "flowchart_support": "supported",
    "label_encoding": "decimal_ascii_punctuation_v1",
    "html_labels": false
  },
  "representation": "flowchart",
  "decision": {
    "rule_id": "verified_directed_acyclic_flow_v1",
    "reason": "来源明确给出完整有向无环关系，且所选目标的当前图形及标签配置已有实测依据。",
    "fallback_reasons": []
  },
  "review_ir_fingerprint": {
    "sha256": "ff61f02a9ea602cd4a06daf3dfc56cd2f6e842668d42ec1f531d4f9b6ad0eef6",
    "byte_length": 9057
  },
  "plan_fingerprint": {
    "sha256": "b81f3c3bbd165d6579579fca70f06ea1dbdaceb578db2dfc34f21001f420b96b",
    "byte_length": 17058
  },
  "totals": {
    "branch_relations": 2,
    "causes_relations": 0,
    "content_mappings": 17,
    "correlates_relations": 0,
    "decision_steps": 1,
    "evidence_records": 8,
    "flow_edges": 3,
    "flow_nodes": 4,
    "inhibits_relations": 0,
    "links": 9,
    "node_conditions": 4,
    "node_exceptions": 4,
    "node_inputs": 4,
    "node_outputs": 4,
    "parsed_statements": 8,
    "process_conditions": 1,
    "process_exceptions": 1,
    "promotes_relations": 0,
    "relation_conditions": 3,
    "relation_exceptions": 3,
    "relations": 3,
    "rendered_notes": 2,
    "sequence_relations": 1,
    "source_statements": 8,
    "steps": 4,
    "unknown_relations": 0,
    "unparsed_statements": 0
  },
  "inputs": {
    ".case.json": {
      "sha256": "f95d0d22e1e6e2c588e89351512ac1d8001d9d2f120c2fbc5de0c0a551234019",
      "byte_length": 363
    },
    ".gold.json": {
      "sha256": "3a89a171a1113973694d785813fe40f63f294154294cdee5dc049721c326bfaa",
      "byte_length": 5149
    },
    ".profile.json": {
      "sha256": "6d2051f224d92b93fc25ddefa34f22ebd706d6a80027ec01cf09a99c21213469",
      "byte_length": 453
    },
    ".review-.case.json": {
      "sha256": "f95d0d22e1e6e2c588e89351512ac1d8001d9d2f120c2fbc5de0c0a551234019",
      "byte_length": 363
    },
    ".review-.gold.json": {
      "sha256": "3a89a171a1113973694d785813fe40f63f294154294cdee5dc049721c326bfaa",
      "byte_length": 5149
    },
    ".review-.owner.json": {
      "sha256": "99d93da26e8b2759395a41430cdb4947e627284afbbd8ff757cbebaaee449014",
      "byte_length": 170
    },
    ".review-.source.json": {
      "sha256": "db1283ef8ae4324a0a0be9acd00922e95460abd1a3cc9e9dabbe66b02274cad8",
      "byte_length": 2372
    },
    ".review-process-ir.json": {
      "sha256": "ff61f02a9ea602cd4a06daf3dfc56cd2f6e842668d42ec1f531d4f9b6ad0eef6",
      "byte_length": 9057
    },
    ".review-review.json": {
      "sha256": "ca21522a59e6af4c3d89c184d24d2228cb21217cb8fb595d1ad527c0a26c03aa",
      "byte_length": 9035
    },
    ".review-review.md": {
      "sha256": "df8321ae5dc2c6c2036765c321fb540add034eae314c93dd83118b945fdb0134",
      "byte_length": 9218
    },
    ".source.json": {
      "sha256": "db1283ef8ae4324a0a0be9acd00922e95460abd1a3cc9e9dabbe66b02274cad8",
      "byte_length": 2372
    }
  },
  "files": [
    {
      "relative_path": "流程总览.md",
      "note_id": "flow_overview",
      "plan_revision": 1,
      "content_sha256": "1c1a43f9eb1e95ff495575eb7f5875abad0d6a2e7179ac137ce5ffa42b36d44e",
      "byte_length": 981,
      "content_mappings": [
        {
          "section_id": "flow",
          "block_id": "process_title",
          "kind": "text",
          "item_id": null,
          "source_object_ids": [
            "process"
          ],
          "start_byte": 41,
          "end_byte": 74
        },
        {
          "section_id": "flow",
          "block_id": "reading_scope",
          "kind": "text",
          "item_id": null,
          "source_object_ids": [],
          "start_byte": 88,
          "end_byte": 349
        },
        {
          "section_id": "flow",
          "block_id": "process_diagram",
          "kind": "diagram_node",
          "item_id": "n0",
          "source_object_ids": [
            "d1"
          ],
          "start_byte": 379,
          "end_byte": 414
        },
        {
          "section_id": "flow",
          "block_id": "process_diagram",
          "kind": "diagram_node",
          "item_id": "n1",
          "source_object_ids": [
            "s1"
          ],
          "start_byte": 415,
          "end_byte": 435
        },
        {
          "section_id": "flow",
          "block_id": "process_diagram",
          "kind": "diagram_node",
          "item_id": "n2",
          "source_object_ids": [
            "s2"
          ],
          "start_byte": 436,
          "end_byte": 462
        },
        {
          "section_id": "flow",
          "block_id": "process_diagram",
          "kind": "diagram_node",
          "item_id": "n3",
          "source_object_ids": [
            "s3"
          ],
          "start_byte": 463,
          "end_byte": 489
        },
        {
          "section_id": "flow",
          "block_id": "process_diagram",
          "kind": "diagram_edge",
          "item_id": "e0",
          "source_object_ids": [
            "r1"
          ],
          "start_byte": 490,
          "end_byte": 511
        },
        {
          "section_id": "flow",
          "block_id": "process_diagram",
          "kind": "diagram_edge",
          "item_id": "e1",
          "source_object_ids": [
            "d1",
            "r2"
          ],
          "start_byte": 512,
          "end_byte": 554
        },
        {
          "section_id": "flow",
          "block_id": "process_diagram",
          "kind": "diagram_edge",
          "item_id": "e2",
          "source_object_ids": [
            "d1",
            "r3"
          ],
          "start_byte": 555,
          "end_byte": 597
        }
      ],
      "links": [
        {
          "section_id": "flow",
          "block_id": "detail_links",
          "item_id": null,
          "target_path": "过程明细.md",
          "target_heading": "过程说明",
          "display_text": "查看完整过程说明",
          "source_object_ids": [
            "process"
          ],
          "start_byte": 603,
          "end_byte": 657
        },
        {
          "section_id": "flow",
          "block_id": "detail_links",
          "item_id": null,
          "target_path": "过程明细.md",
          "target_heading": "步骤-d1",
          "display_text": "查看步骤明细",
          "source_object_ids": [
            "d1"
          ],
          "start_byte": 658,
          "end_byte": 703
        },
        {
          "section_id": "flow",
          "block_id": "detail_links",
          "item_id": null,
          "target_path": "过程明细.md",
          "target_heading": "步骤-s1",
          "display_text": "查看步骤明细",
          "source_object_ids": [
            "s1"
          ],
          "start_byte": 704,
          "end_byte": 749
        },
        {
          "section_id": "flow",
          "block_id": "detail_links",
          "item_id": null,
          "target_path": "过程明细.md",
          "target_heading": "步骤-s2",
          "display_text": "查看步骤明细",
          "source_object_ids": [
            "s2"
          ],
          "start_byte": 750,
          "end_byte": 795
        },
        {
          "section_id": "flow",
          "block_id": "detail_links",
          "item_id": null,
          "target_path": "过程明细.md",
          "target_heading": "步骤-s3",
          "display_text": "查看步骤明细",
          "source_object_ids": [
            "s3"
          ],
          "start_byte": 796,
          "end_byte": 841
        },
        {
          "section_id": "flow",
          "block_id": "detail_links",
          "item_id": null,
          "target_path": "过程明细.md",
          "target_heading": "关系-r1",
          "display_text": "查看关系明细",
          "source_object_ids": [
            "r1"
          ],
          "start_byte": 842,
          "end_byte": 887
        },
        {
          "section_id": "flow",
          "block_id": "detail_links",
          "item_id": null,
          "target_path": "过程明细.md",
          "target_heading": "关系-r2",
          "display_text": "查看关系明细",
          "source_object_ids": [
            "r2"
          ],
          "start_byte": 888,
          "end_byte": 933
        },
        {
          "section_id": "flow",
          "block_id": "detail_links",
          "item_id": null,
          "target_path": "过程明细.md",
          "target_heading": "关系-r3",
          "display_text": "查看关系明细",
          "source_object_ids": [
            "r3"
          ],
          "start_byte": 934,
          "end_byte": 979
        }
      ]
    },
    {
      "relative_path": "过程明细.md",
      "note_id": "process_details",
      "plan_revision": 1,
      "content_sha256": "914613316b45141927bec71f4168e7dd38e7abd8cd65b16867224b749f8a54b9",
      "byte_length": 2075,
      "content_mappings": [
        {
          "section_id": "process_detail",
          "block_id": "process",
          "kind": "text",
          "item_id": null,
          "source_object_ids": [
            "process"
          ],
          "start_byte": 41,
          "end_byte": 178
        },
        {
          "section_id": "step_d1",
          "block_id": "d1",
          "kind": "text",
          "item_id": null,
          "source_object_ids": [
            "d1"
          ],
          "start_byte": 206,
          "end_byte": 451
        },
        {
          "section_id": "step_s1",
          "block_id": "s1",
          "kind": "text",
          "item_id": null,
          "source_object_ids": [
            "s1"
          ],
          "start_byte": 479,
          "end_byte": 715
        },
        {
          "section_id": "step_s2",
          "block_id": "s2",
          "kind": "text",
          "item_id": null,
          "source_object_ids": [
            "s2"
          ],
          "start_byte": 743,
          "end_byte": 1012
        },
        {
          "section_id": "step_s3",
          "block_id": "s3",
          "kind": "text",
          "item_id": null,
          "source_object_ids": [
            "s3"
          ],
          "start_byte": 1040,
          "end_byte": 1327
        },
        {
          "section_id": "relation_r1",
          "block_id": "r1",
          "kind": "text",
          "item_id": null,
          "source_object_ids": [
            "d1",
            "r1",
            "s1"
          ],
          "start_byte": 1355,
          "end_byte": 1526
        },
        {
          "section_id": "relation_r2",
          "block_id": "r2",
          "kind": "text",
          "item_id": null,
          "source_object_ids": [
            "d1",
            "r2",
            "s2"
          ],
          "start_byte": 1554,
          "end_byte": 1764
        },
        {
          "section_id": "relation_r3",
          "block_id": "r3",
          "kind": "text",
          "item_id": null,
          "source_object_ids": [
            "d1",
            "r3",
            "s3"
          ],
          "start_byte": 1792,
          "end_byte": 2002
        }
      ],
      "links": [
        {
          "section_id": "return",
          "block_id": "return_link",
          "item_id": null,
          "target_path": "流程总览.md",
          "target_heading": "流程表达",
          "display_text": "返回流程总览",
          "source_object_ids": [
            "process"
          ],
          "start_byte": 2025,
          "end_byte": 2073
        }
      ]
    }
  ],
  "artifacts": {
    ".case.json": {
      "sha256": "f95d0d22e1e6e2c588e89351512ac1d8001d9d2f120c2fbc5de0c0a551234019",
      "byte_length": 363
    },
    ".gold.json": {
      "sha256": "3a89a171a1113973694d785813fe40f63f294154294cdee5dc049721c326bfaa",
      "byte_length": 5149
    },
    ".owner.json": {
      "sha256": "0ce7903dbb2c63dc8dc3fdb6807a83444f811a512c164223aad6b86d06109638",
      "byte_length": 146
    },
    ".profile.json": {
      "sha256": "6d2051f224d92b93fc25ddefa34f22ebd706d6a80027ec01cf09a99c21213469",
      "byte_length": 453
    },
    ".review-.case.json": {
      "sha256": "f95d0d22e1e6e2c588e89351512ac1d8001d9d2f120c2fbc5de0c0a551234019",
      "byte_length": 363
    },
    ".review-.gold.json": {
      "sha256": "3a89a171a1113973694d785813fe40f63f294154294cdee5dc049721c326bfaa",
      "byte_length": 5149
    },
    ".review-.owner.json": {
      "sha256": "99d93da26e8b2759395a41430cdb4947e627284afbbd8ff757cbebaaee449014",
      "byte_length": 170
    },
    ".review-.source.json": {
      "sha256": "db1283ef8ae4324a0a0be9acd00922e95460abd1a3cc9e9dabbe66b02274cad8",
      "byte_length": 2372
    },
    ".review-process-ir.json": {
      "sha256": "ff61f02a9ea602cd4a06daf3dfc56cd2f6e842668d42ec1f531d4f9b6ad0eef6",
      "byte_length": 9057
    },
    ".review-review.json": {
      "sha256": "ca21522a59e6af4c3d89c184d24d2228cb21217cb8fb595d1ad527c0a26c03aa",
      "byte_length": 9035
    },
    ".review-review.md": {
      "sha256": "df8321ae5dc2c6c2036765c321fb540add034eae314c93dd83118b945fdb0134",
      "byte_length": 9218
    },
    ".source.json": {
      "sha256": "db1283ef8ae4324a0a0be9acd00922e95460abd1a3cc9e9dabbe66b02274cad8",
      "byte_length": 2372
    },
    "plan.json": {
      "sha256": "b81f3c3bbd165d6579579fca70f06ea1dbdaceb578db2dfc34f21001f420b96b",
      "byte_length": 17058
    },
    "流程总览.md": {
      "sha256": "1c1a43f9eb1e95ff495575eb7f5875abad0d6a2e7179ac137ce5ffa42b36d44e",
      "byte_length": 981
    },
    "过程明细.md": {
      "sha256": "914613316b45141927bec71f4168e7dd38e7abd8cd65b16867224b749f8a54b9",
      "byte_length": 2075
    }
  }
}
```
