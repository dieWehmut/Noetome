# 有据流程表达预览

状态：needs_review；预览：not_run；知识状态：pending；发布：false。

科学质量：not_run；研究：not_requested；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "knowledge_flow",
  "scope": "active_flow_v1",
  "request_id": "b49045ff-de7b-4e15-aa57-62cb604c9e89",
  "binding": {
    "project_id": "c2e81ca1-ac80-4ceb-a0cc-ee88be5f724d",
    "import_id": "58ba1bc8-92cc-4f3f-9536-12ec7d046eb1",
    "source_id": "548e0bb4-e80d-4572-85ed-7e39badef5c1",
    "version_id": "90e254ad-509e-4f16-a322-04893412ea19",
    "content_sha256": "ed70d6507618841afffdedd6bd28a0ab0df4e8ce91f5a30006fbec45d09d41cd",
    "normalized_sha256": "ed70d6507618841afffdedd6bd28a0ab0df4e8ce91f5a30006fbec45d09d41cd",
    "parse_id": null,
    "manifest_sha256": null
  },
  "status": "needs_review",
  "preview_status": "not_run",
  "knowledge_status": "pending",
  "scientific_quality": "not_run",
  "research_status": "not_requested",
  "published": false,
  "network_calls": 0,
  "paid_calls": 0,
  "checks": {
    "source_review": "passed",
    "model_support": "needs_review",
    "note_plan": "not_run",
    "rendering": "not_run",
    "saved_output": "passed"
  },
  "issue_codes": [
    "FLOW_UNKNOWN_CRITERION"
  ],
  "issues": [
    {
      "code": "FLOW_UNKNOWN_CRITERION",
      "object_ids": [
        "d1"
      ],
      "message": "判定缺少明确判据，不能生成真假分支表达。"
    }
  ],
  "process_document": {
    "process": {
      "title": "未知判据的分流说明",
      "entry": "d1",
      "goals": [
        "s1",
        "s2"
      ],
      "conditions": [
        "演示模式开启"
      ],
      "exceptions": []
    },
    "steps": [
      {
        "id": "d1",
        "kind": "decision",
        "label": "执行待明确的判定",
        "description": "来源未列明实际判据",
        "criterion": null,
        "inputs": [
          "样例卡"
        ],
        "outputs": [
          "未知"
        ],
        "conditions": [
          "卡片可读"
        ],
        "exceptions": [
          "判据未确认前不能执行"
        ]
      },
      {
        "id": "s1",
        "kind": "step",
        "label": "真支处理",
        "description": "只记录真支去向",
        "criterion": null,
        "inputs": [
          "真支卡片"
        ],
        "outputs": [
          "真支结果"
        ],
        "conditions": [],
        "exceptions": []
      },
      {
        "id": "s2",
        "kind": "step",
        "label": "假支处理",
        "description": "只记录假支去向",
        "criterion": null,
        "inputs": [
          "假支卡片"
        ],
        "outputs": [
          "假支结果"
        ],
        "conditions": [],
        "exceptions": []
      }
    ],
    "relations": [
      {
        "id": "r1",
        "left": "d1",
        "right": "s1",
        "kind": "branch",
        "branch_outcome": true,
        "conditions": [],
        "exceptions": []
      },
      {
        "id": "r2",
        "left": "d1",
        "right": "s2",
        "kind": "branch",
        "branch_outcome": false,
        "conditions": [],
        "exceptions": []
      }
    ],
    "evidence": [
      {
        "object_id": "d1",
        "start_byte": 127,
        "end_byte": 346,
        "quote": "判定 d1：\"执行待明确的判定\"。判据：null。说明：\"来源未列明实际判据\"。输入：[\"样例卡\"]。输出：[\"未知\"]。条件：[\"卡片可读\"]。例外：[\"判据未确认前不能执行\"]。"
      },
      {
        "object_id": "process",
        "start_byte": 0,
        "end_byte": 126,
        "quote": "过程：\"未知判据的分流说明\"。入口：d1。目标：[\"s1\",\"s2\"]。条件：[\"演示模式开启\"]。例外：[]。"
      },
      {
        "object_id": "r1",
        "start_byte": 645,
        "end_byte": 718,
        "quote": "关系 r1：d1 的判据为真时执行 s1。条件：[]。例外：[]。"
      },
      {
        "object_id": "r2",
        "start_byte": 719,
        "end_byte": 792,
        "quote": "关系 r2：d1 的判据为假时执行 s2。条件：[]。例外：[]。"
      },
      {
        "object_id": "s1",
        "start_byte": 347,
        "end_byte": 495,
        "quote": "步骤 s1：\"真支处理\"。说明：\"只记录真支去向\"。输入：[\"真支卡片\"]。输出：[\"真支结果\"]。条件：[]。例外：[]。"
      },
      {
        "object_id": "s2",
        "start_byte": 496,
        "end_byte": 644,
        "quote": "步骤 s2：\"假支处理\"。说明：\"只记录假支去向\"。输入：[\"假支卡片\"]。输出：[\"假支结果\"]。条件：[]。例外：[]。"
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
  "representation": null,
  "decision": null,
  "review_ir_fingerprint": {
    "sha256": "b24a345e15696c4faf12635d5496b893b9321194cf6b035c1813db59cde76329",
    "byte_length": 6089
  },
  "plan_fingerprint": null,
  "totals": {
    "branch_relations": 2,
    "causes_relations": 0,
    "content_mappings": 0,
    "correlates_relations": 0,
    "decision_steps": 1,
    "evidence_records": 6,
    "flow_edges": 0,
    "flow_nodes": 0,
    "inhibits_relations": 0,
    "links": 0,
    "node_conditions": 1,
    "node_exceptions": 1,
    "node_inputs": 3,
    "node_outputs": 3,
    "parsed_statements": 6,
    "process_conditions": 1,
    "process_exceptions": 0,
    "promotes_relations": 0,
    "relation_conditions": 0,
    "relation_exceptions": 0,
    "relations": 2,
    "rendered_notes": 0,
    "sequence_relations": 0,
    "source_statements": 6,
    "steps": 3,
    "unknown_relations": 0,
    "unparsed_statements": 0
  },
  "inputs": {
    ".case.json": {
      "sha256": "691072e7705ab878edd78add87208d5eeba070d84bca0fc1693dd9e482c43b6d",
      "byte_length": 363
    },
    ".gold.json": {
      "sha256": "a09214fe74630677143c32788eb135e2a9d7ca04bb41a78a8d69bcfb739aae65",
      "byte_length": 3212
    },
    ".profile.json": {
      "sha256": "6d2051f224d92b93fc25ddefa34f22ebd706d6a80027ec01cf09a99c21213469",
      "byte_length": 453
    },
    ".review-.case.json": {
      "sha256": "691072e7705ab878edd78add87208d5eeba070d84bca0fc1693dd9e482c43b6d",
      "byte_length": 363
    },
    ".review-.gold.json": {
      "sha256": "a09214fe74630677143c32788eb135e2a9d7ca04bb41a78a8d69bcfb739aae65",
      "byte_length": 3212
    },
    ".review-.owner.json": {
      "sha256": "bbe4dcef3e69cafb21782dec3fe08f604ca615c9c0c71b8ab044a284f2a9d045",
      "byte_length": 170
    },
    ".review-.source.json": {
      "sha256": "2381ace48e103dd84777123cf2b25ca13a75e93317b78c41902dc67d8c661519",
      "byte_length": 1723
    },
    ".review-process-ir.json": {
      "sha256": "b24a345e15696c4faf12635d5496b893b9321194cf6b035c1813db59cde76329",
      "byte_length": 6089
    },
    ".review-review.json": {
      "sha256": "62f323824747434e2b92adf4e1953857be30681b96f4300a67ca5156c69782b9",
      "byte_length": 6974
    },
    ".review-review.md": {
      "sha256": "1a07bc025251dfc71d327e4997774cae76ac60c8f6b9f23254c77d10bc63b446",
      "byte_length": 7157
    },
    ".source.json": {
      "sha256": "2381ace48e103dd84777123cf2b25ca13a75e93317b78c41902dc67d8c661519",
      "byte_length": 1723
    }
  },
  "files": [],
  "artifacts": {
    ".case.json": {
      "sha256": "691072e7705ab878edd78add87208d5eeba070d84bca0fc1693dd9e482c43b6d",
      "byte_length": 363
    },
    ".gold.json": {
      "sha256": "a09214fe74630677143c32788eb135e2a9d7ca04bb41a78a8d69bcfb739aae65",
      "byte_length": 3212
    },
    ".owner.json": {
      "sha256": "dab9b44260bc380ee7ca4f54e9c624813a24e3d39aec2d42e55091614c0f5cf4",
      "byte_length": 146
    },
    ".profile.json": {
      "sha256": "6d2051f224d92b93fc25ddefa34f22ebd706d6a80027ec01cf09a99c21213469",
      "byte_length": 453
    },
    ".review-.case.json": {
      "sha256": "691072e7705ab878edd78add87208d5eeba070d84bca0fc1693dd9e482c43b6d",
      "byte_length": 363
    },
    ".review-.gold.json": {
      "sha256": "a09214fe74630677143c32788eb135e2a9d7ca04bb41a78a8d69bcfb739aae65",
      "byte_length": 3212
    },
    ".review-.owner.json": {
      "sha256": "bbe4dcef3e69cafb21782dec3fe08f604ca615c9c0c71b8ab044a284f2a9d045",
      "byte_length": 170
    },
    ".review-.source.json": {
      "sha256": "2381ace48e103dd84777123cf2b25ca13a75e93317b78c41902dc67d8c661519",
      "byte_length": 1723
    },
    ".review-process-ir.json": {
      "sha256": "b24a345e15696c4faf12635d5496b893b9321194cf6b035c1813db59cde76329",
      "byte_length": 6089
    },
    ".review-review.json": {
      "sha256": "62f323824747434e2b92adf4e1953857be30681b96f4300a67ca5156c69782b9",
      "byte_length": 6974
    },
    ".review-review.md": {
      "sha256": "1a07bc025251dfc71d327e4997774cae76ac60c8f6b9f23254c77d10bc63b446",
      "byte_length": 7157
    },
    ".source.json": {
      "sha256": "2381ace48e103dd84777123cf2b25ca13a75e93317b78c41902dc67d8c661519",
      "byte_length": 1723
    }
  }
}
```
