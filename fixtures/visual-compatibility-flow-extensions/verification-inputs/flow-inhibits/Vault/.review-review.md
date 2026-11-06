# 有界过程来源核对

状态：passed；知识状态：pending；发布：false。

科学质量：not_run；研究：not_requested；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "process_source_review",
  "scope": "bounded_process_text_v1",
  "request_id": "a3473294-fcc4-4f88-a28a-d4d2d9ef78c1",
  "binding": {
    "project_id": "c8f0d542-1bb6-4c24-82b6-ad20fc189af4",
    "import_id": "63f6f00c-2007-424a-9a1d-990aa0dc9726",
    "source_id": "fcd1028d-31fd-453e-b2b7-21afa69b3214",
    "version_id": "026cdb13-335a-48c7-8f8f-cb4e9927b193",
    "content_sha256": "fc79a71511c61385a98ffad4788a68a74f5edeee088f35a6dfdc770c71f0b242",
    "normalized_sha256": "fc79a71511c61385a98ffad4788a68a74f5edeee088f35a6dfdc770c71f0b242",
    "parse_id": null,
    "manifest_sha256": null
  },
  "selection": [
    {
      "kind": "text",
      "ordinal": 0
    }
  ],
  "status": "passed",
  "knowledge_status": "pending",
  "scientific_quality": "not_run",
  "research_status": "not_requested",
  "published": false,
  "network_calls": 0,
  "paid_calls": 0,
  "checks": {
    "source_parsing": "passed",
    "gold_match": "passed",
    "saved_output": "passed"
  },
  "issue_codes": [],
  "unparsed_statements": 0,
  "parsed_document": {
    "process": {
      "title": "演示鸣响抑制规则",
      "entry": "s1",
      "goals": [
        "s2"
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
        "id": "s1",
        "kind": "step",
        "label": "启用演示静音控制",
        "description": "把演示静音控制设为启用",
        "criterion": null,
        "inputs": [
          "静音命令"
        ],
        "outputs": [
          "已启用静音控制"
        ],
        "conditions": [
          "演示模式开启"
        ],
        "exceptions": [
          "控制器断开时不生效"
        ]
      },
      {
        "id": "s2",
        "kind": "step",
        "label": "蜂鸣器鸣响响应",
        "description": "表示可能受到抑制的鸣响响应，不断言响应已发生",
        "criterion": null,
        "inputs": [
          "鸣响请求"
        ],
        "outputs": [
          "可能的鸣响响应"
        ],
        "conditions": [
          "蜂鸣器可用"
        ],
        "exceptions": [
          "蜂鸣器断开时不可观察"
        ]
      }
    ],
    "relations": [
      {
        "id": "r1",
        "left": "s1",
        "right": "s2",
        "kind": "inhibits",
        "branch_outcome": null,
        "conditions": [
          "演示静音通道接通"
        ],
        "exceptions": [
          "旁路开启时该抑制不成立"
        ]
      }
    ],
    "evidence": [
      {
        "object_id": "process",
        "start_byte": 0,
        "end_byte": 144,
        "quote": "过程：\"演示鸣响抑制规则\"。入口：s1。目标：[\"s2\"]。条件：[\"演示模式开启\"]。例外：[\"资料不可读时暂停\"]。"
      },
      {
        "object_id": "r1",
        "start_byte": 637,
        "end_byte": 753,
        "quote": "关系 r1：s1 抑制 s2。条件：[\"演示静音通道接通\"]。例外：[\"旁路开启时该抑制不成立\"]。"
      },
      {
        "object_id": "s1",
        "start_byte": 145,
        "end_byte": 375,
        "quote": "步骤 s1：\"启用演示静音控制\"。说明：\"把演示静音控制设为启用\"。输入：[\"静音命令\"]。输出：[\"已启用静音控制\"]。条件：[\"演示模式开启\"]。例外：[\"控制器断开时不生效\"]。"
      },
      {
        "object_id": "s2",
        "start_byte": 376,
        "end_byte": 636,
        "quote": "步骤 s2：\"蜂鸣器鸣响响应\"。说明：\"表示可能受到抑制的鸣响响应，不断言响应已发生\"。输入：[\"鸣响请求\"]。输出：[\"可能的鸣响响应\"]。条件：[\"蜂鸣器可用\"]。例外：[\"蜂鸣器断开时不可观察\"]。"
      }
    ]
  },
  "totals": {
    "branch_relations": 0,
    "causes_relations": 0,
    "correlates_relations": 0,
    "decision_steps": 0,
    "evidence_records": 4,
    "inhibits_relations": 1,
    "node_conditions": 2,
    "node_exceptions": 2,
    "node_inputs": 2,
    "node_outputs": 2,
    "parsed_statements": 4,
    "process_conditions": 1,
    "process_exceptions": 1,
    "promotes_relations": 0,
    "relation_conditions": 1,
    "relation_exceptions": 1,
    "relations": 1,
    "sequence_relations": 0,
    "source_statements": 4,
    "steps": 2,
    "unknown_relations": 0,
    "unparsed_statements": 0
  },
  "expected_totals": {
    "branch_relations": 0,
    "causes_relations": 0,
    "correlates_relations": 0,
    "decision_steps": 0,
    "evidence_records": 4,
    "inhibits_relations": 1,
    "node_conditions": 2,
    "node_exceptions": 2,
    "node_inputs": 2,
    "node_outputs": 2,
    "parsed_statements": 4,
    "process_conditions": 1,
    "process_exceptions": 1,
    "promotes_relations": 0,
    "relation_conditions": 1,
    "relation_exceptions": 1,
    "relations": 1,
    "sequence_relations": 0,
    "source_statements": 4,
    "steps": 2,
    "unknown_relations": 0,
    "unparsed_statements": 0
  },
  "gold_sha256": "4ec5170931a64c95d819cacc8b33151e940c134316a57937c01bbb79355a9548",
  "ir_fingerprint": {
    "sha256": "0e860c0a3818de19d720ab05b74196928fa1f5329e77704a9564276b7a6cb53e",
    "byte_length": 5303
  },
  "inputs": {
    ".case.json": {
      "sha256": "71bcea656ee244de909418a8638265908115672cca5906988f8b74194161ecee",
      "byte_length": 363
    },
    ".gold.json": {
      "sha256": "4ec5170931a64c95d819cacc8b33151e940c134316a57937c01bbb79355a9548",
      "byte_length": 2775
    },
    ".source.json": {
      "sha256": "e5f1d128eb1f3f18b04039218b7248c2dd8bcf19ba1c0c5c0a4c9de6cbc786d2",
      "byte_length": 1682
    }
  },
  "artifacts": {
    ".case.json": {
      "sha256": "71bcea656ee244de909418a8638265908115672cca5906988f8b74194161ecee",
      "byte_length": 363
    },
    ".gold.json": {
      "sha256": "4ec5170931a64c95d819cacc8b33151e940c134316a57937c01bbb79355a9548",
      "byte_length": 2775
    },
    ".owner.json": {
      "sha256": "261271023876d8e3e788380c3f3b6285f9c22dca39c8ba4e1924647d5fb5bd97",
      "byte_length": 170
    },
    ".source.json": {
      "sha256": "e5f1d128eb1f3f18b04039218b7248c2dd8bcf19ba1c0c5c0a4c9de6cbc786d2",
      "byte_length": 1682
    },
    "process-ir.json": {
      "sha256": "0e860c0a3818de19d720ab05b74196928fa1f5329e77704a9564276b7a6cb53e",
      "byte_length": 5303
    }
  }
}
```
