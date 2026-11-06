# 有界过程来源核对

状态：passed；知识状态：pending；发布：false。

科学质量：not_run；研究：not_requested；网络与付费调用：0。

```json
{
  "schema_version": 1,
  "producer": "process_source_review",
  "scope": "bounded_process_text_v1",
  "request_id": "a4eaab1a-fa21-41c2-ba66-a5a219cb8303",
  "binding": {
    "project_id": "1b6729da-2c47-40ab-bd5e-9803b51fde04",
    "import_id": "63c9c26c-2ddb-4aca-9dbb-b158576daace",
    "source_id": "9e2e7268-fec1-4705-aa22-98baccaf13f3",
    "version_id": "650b31ab-0de4-4cc8-89e8-9c45f4aec8c3",
    "content_sha256": "07ffaee58e8b3f349addf15fd2eb7df5a49357fa808c65fc5728bb2c1bd7d076",
    "normalized_sha256": "07ffaee58e8b3f349addf15fd2eb7df5a49357fa808c65fc5728bb2c1bd7d076",
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
      "title": "多行标签的保真流程",
      "entry": "s1",
      "goals": [
        "s2"
      ],
      "conditions": [],
      "exceptions": [
        "不可读资料需复核"
      ]
    },
    "steps": [
      {
        "id": "s1",
        "kind": "step",
        "label": "检查\n卡片\t[α]",
        "description": "完整保留来源标签中的换行和制表符",
        "criterion": null,
        "inputs": [
          "样例卡"
        ],
        "outputs": [
          "检查结果"
        ],
        "conditions": [],
        "exceptions": [
          "卡片缺失时停止"
        ]
      },
      {
        "id": "s2",
        "kind": "step",
        "label": "记录检查结果",
        "description": "记录来源明示的检查结果",
        "criterion": null,
        "inputs": [
          "检查结果"
        ],
        "outputs": [
          "检查记录"
        ],
        "conditions": [
          "检查完成"
        ],
        "exceptions": []
      }
    ],
    "relations": [
      {
        "id": "r1",
        "left": "s1",
        "right": "s2",
        "kind": "sequence",
        "branch_outcome": null,
        "conditions": [],
        "exceptions": [
          "检查未完成时不得登记"
        ]
      }
    ],
    "evidence": [
      {
        "object_id": "process",
        "start_byte": 0,
        "end_byte": 127,
        "quote": "过程：\"多行标签的保真流程\"。入口：s1。目标：[\"s2\"]。条件：[]。例外：[\"不可读资料需复核\"]。"
      },
      {
        "object_id": "r1",
        "start_byte": 513,
        "end_byte": 609,
        "quote": "关系 r1：s1 完成后执行 s2。条件：[]。例外：[\"检查未完成时不得登记\"]。"
      },
      {
        "object_id": "s1",
        "start_byte": 128,
        "end_byte": 331,
        "quote": "步骤 s1：\"检查\\n卡片\\t[α]\"。说明：\"完整保留来源标签中的换行和制表符\"。输入：[\"样例卡\"]。输出：[\"检查结果\"]。条件：[]。例外：[\"卡片缺失时停止\"]。"
      },
      {
        "object_id": "s2",
        "start_byte": 332,
        "end_byte": 512,
        "quote": "步骤 s2：\"记录检查结果\"。说明：\"记录来源明示的检查结果\"。输入：[\"检查结果\"]。输出：[\"检查记录\"]。条件：[\"检查完成\"]。例外：[]。"
      }
    ]
  },
  "totals": {
    "branch_relations": 0,
    "causes_relations": 0,
    "correlates_relations": 0,
    "decision_steps": 0,
    "evidence_records": 4,
    "inhibits_relations": 0,
    "node_conditions": 1,
    "node_exceptions": 1,
    "node_inputs": 2,
    "node_outputs": 2,
    "parsed_statements": 4,
    "process_conditions": 0,
    "process_exceptions": 1,
    "promotes_relations": 0,
    "relation_conditions": 0,
    "relation_exceptions": 1,
    "relations": 1,
    "sequence_relations": 1,
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
    "inhibits_relations": 0,
    "node_conditions": 1,
    "node_exceptions": 1,
    "node_inputs": 2,
    "node_outputs": 2,
    "parsed_statements": 4,
    "process_conditions": 0,
    "process_exceptions": 1,
    "promotes_relations": 0,
    "relation_conditions": 0,
    "relation_exceptions": 1,
    "relations": 1,
    "sequence_relations": 1,
    "source_statements": 4,
    "steps": 2,
    "unknown_relations": 0,
    "unparsed_statements": 0
  },
  "gold_sha256": "79999b21da68b9e80f4313cfce72821163a965540950acc631a9b9ba98d288c4",
  "ir_fingerprint": {
    "sha256": "8aa5f79722f6fe5d3ad35db0bbd005579429adc8ab8ca6ce9249a399408fdd37",
    "byte_length": 4773
  },
  "inputs": {
    ".case.json": {
      "sha256": "6c7d6ac8240be7f390a0148304eee70e5657cfb8203e5c337e795dd612f60adb",
      "byte_length": 363
    },
    ".gold.json": {
      "sha256": "79999b21da68b9e80f4313cfce72821163a965540950acc631a9b9ba98d288c4",
      "byte_length": 2412
    },
    ".source.json": {
      "sha256": "a55730a94b7674d64e1b6176ccce45dc584791214f08f0001154838f63f7db33",
      "byte_length": 1532
    }
  },
  "artifacts": {
    ".case.json": {
      "sha256": "6c7d6ac8240be7f390a0148304eee70e5657cfb8203e5c337e795dd612f60adb",
      "byte_length": 363
    },
    ".gold.json": {
      "sha256": "79999b21da68b9e80f4313cfce72821163a965540950acc631a9b9ba98d288c4",
      "byte_length": 2412
    },
    ".owner.json": {
      "sha256": "80fee289fabd7b9d7efe4c87d62f07e670951691f8385ab90e49f895abea199e",
      "byte_length": 170
    },
    ".source.json": {
      "sha256": "a55730a94b7674d64e1b6176ccce45dc584791214f08f0001154838f63f7db33",
      "byte_length": 1532
    },
    "process-ir.json": {
      "sha256": "8aa5f79722f6fe5d3ad35db0bbd005579429adc8ab8ca6ce9249a399408fdd37",
      "byte_length": 4773
    }
  }
}
```
