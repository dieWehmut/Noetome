# 有界过程来源核对

状态：passed；知识状态：pending；发布：false。

科学质量：not_run；研究：not_requested；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "process_source_review",
  "scope": "bounded_process_text_v1",
  "request_id": "125fb35b-29f0-44d8-b140-d0b735cd7f62",
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
  "totals": {
    "branch_relations": 2,
    "causes_relations": 0,
    "correlates_relations": 0,
    "decision_steps": 1,
    "evidence_records": 8,
    "inhibits_relations": 0,
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
    "sequence_relations": 1,
    "source_statements": 8,
    "steps": 4,
    "unknown_relations": 0,
    "unparsed_statements": 0
  },
  "expected_totals": {
    "branch_relations": 2,
    "causes_relations": 0,
    "correlates_relations": 0,
    "decision_steps": 1,
    "evidence_records": 8,
    "inhibits_relations": 0,
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
    "sequence_relations": 1,
    "source_statements": 8,
    "steps": 4,
    "unknown_relations": 0,
    "unparsed_statements": 0
  },
  "gold_sha256": "3a89a171a1113973694d785813fe40f63f294154294cdee5dc049721c326bfaa",
  "ir_fingerprint": {
    "sha256": "ff61f02a9ea602cd4a06daf3dfc56cd2f6e842668d42ec1f531d4f9b6ad0eef6",
    "byte_length": 9057
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
    ".source.json": {
      "sha256": "db1283ef8ae4324a0a0be9acd00922e95460abd1a3cc9e9dabbe66b02274cad8",
      "byte_length": 2372
    }
  },
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
      "sha256": "99d93da26e8b2759395a41430cdb4947e627284afbbd8ff757cbebaaee449014",
      "byte_length": 170
    },
    ".source.json": {
      "sha256": "db1283ef8ae4324a0a0be9acd00922e95460abd1a3cc9e9dabbe66b02274cad8",
      "byte_length": 2372
    },
    "process-ir.json": {
      "sha256": "ff61f02a9ea602cd4a06daf3dfc56cd2f6e842668d42ec1f531d4f9b6ad0eef6",
      "byte_length": 9057
    }
  }
}
```
