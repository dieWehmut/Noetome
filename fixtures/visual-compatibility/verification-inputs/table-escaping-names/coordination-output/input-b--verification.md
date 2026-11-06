# 有界来源知识核对

状态：passed；知识状态：pending；发布：false。

科学质量：not_run；研究：not_requested；网络与付费调用：0。

规则：controlled_measurement_zh_v1；复核主体：automated_source_review；解释时间："before_synthetic_response"。

来源范围、对象、问题码、逐类别检查与分母见以下完整事实快照，与 verification.json 一致。

```json
{
  "schema_version": 1,
  "report_kind": "source_knowledge_verification",
  "producer": "source_review",
  "request_id": "synthetic-coordination-b",
  "status": "passed",
  "knowledge_status": "pending",
  "published": false,
  "scientific_quality": "not_run",
  "research_status": "not_requested",
  "network_calls": 0,
  "paid_calls": 0,
  "scope": {
    "purpose": "single_measurement_source_review",
    "selection": [
      {
        "kind": "text",
        "ordinal": 0
      }
    ],
    "binding": {
      "project_id": "d10dd42e-e2b2-4d2a-9316-137464a42a87",
      "import_id": "aa99adb4-720d-4988-825a-fe957dd0096e",
      "source_id": "8aeea69f-044d-4126-bfa6-4ce3059fa0ca",
      "version_id": "c95284c5-6816-4cf5-aa12-7c5be32f9b99",
      "content_sha256": "4f1961d4828189ea71b8fa887bb7ca12dde10e7ec3e43c87e8fe8df4ec10bc72",
      "normalized_sha256": "4f1961d4828189ea71b8fa887bb7ca12dde10e7ec3e43c87e8fe8df4ec10bc72",
      "parse_id": null,
      "manifest_sha256": null
    },
    "units": [
      {
        "unit_id": "unit-0",
        "locator": {
          "kind": "text",
          "span_id": "8b423bac-4f96-45c2-9f85-e5fe6c3c3b5c",
          "ordinal": 0,
          "resource_sha256": "4f1961d4828189ea71b8fa887bb7ca12dde10e7ec3e43c87e8fe8df4ec10bc72"
        },
        "source_status": "parsed",
        "start_byte": 0,
        "end_byte": 180,
        "text": "适用条件：已列明；观点时间：2020年。\n乙容器（β） 的速率 | 测量为 12 m/s，条件为x < 2且计数范围 [1,2]；例外为读数 * 异常时不适用。\n"
      }
    ],
    "expected_atoms": 1
  },
  "binding": {
    "project_id": "d10dd42e-e2b2-4d2a-9316-137464a42a87",
    "import_id": "aa99adb4-720d-4988-825a-fe957dd0096e",
    "source_id": "8aeea69f-044d-4126-bfa6-4ce3059fa0ca",
    "version_id": "c95284c5-6816-4cf5-aa12-7c5be32f9b99",
    "content_sha256": "4f1961d4828189ea71b8fa887bb7ca12dde10e7ec3e43c87e8fe8df4ec10bc72",
    "normalized_sha256": "4f1961d4828189ea71b8fa887bb7ca12dde10e7ec3e43c87e8fe8df4ec10bc72",
    "parse_id": null,
    "manifest_sha256": null
  },
  "origin": {
    "producer": "api",
    "transport_mode": "local_http_test",
    "reviewer": "automated_source_review",
    "interpretation_timing": "before_synthetic_response",
    "input_fingerprints": {
      "candidates.json": {
        "sha256": "b4c35331c42fb5ae5dd8f0b63dd1678efc934be504bd7ea40268660c77b38aa2",
        "byte_length": 3086
      },
      "decision.json": {
        "sha256": "5cff0a8cad3a2d60ea5905aff1f5f7184fd9c28f6f02215b4c159a8458e87239",
        "byte_length": 141
      },
      "gold.json": {
        "sha256": "e7f47d8049459854f8614f1381000ca6d2fe1612b5e924b7d642818ad8db7b96",
        "byte_length": 789
      },
      "review.json": {
        "sha256": "f077301cc5939e58f506c4c633ae681690dd3ad48ae250006c1d2e2b3b3dc7e2",
        "byte_length": 528
      }
    },
    "gold_sha256": "e7f47d8049459854f8614f1381000ca6d2fe1612b5e924b7d642818ad8db7b96"
  },
  "rules": {
    "grammar": "controlled_measurement_zh_v1",
    "subject": "乙容器（β）",
    "property": "速率 | 测量",
    "allowed_contexts": [],
    "reviewer": "automated_source_review",
    "interpretation_timing": "before_synthetic_response"
  },
  "expected_results": [
    {
      "object_id": "expected-0",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "issue_codes": []
    }
  ],
  "candidate_results": [
    {
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "issue_codes": []
    }
  ],
  "checks": [
    {
      "category": "schema",
      "object_id": "expected-0",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": "strict_schema_1",
      "actual": null
    },
    {
      "category": "source",
      "object_id": "expected-0",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": "parsed",
      "actual": "parsed"
    },
    {
      "category": "exclusion",
      "object_id": "expected-0",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": "included",
      "actual": "included"
    },
    {
      "category": "coverage",
      "object_id": "expected-0",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": {
        "atom_count": 1,
        "unit_id": "unit-0"
      },
      "actual": {
        "candidate_ids": [
          "b-speed"
        ],
        "decision": {
          "atom_ids": [
            "b-speed"
          ],
          "disposition": "included",
          "exclusion_reason": null,
          "schema_version": 1,
          "unit_id": "unit-0"
        }
      }
    },
    {
      "category": "binding",
      "object_id": "expected-0",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": {
        "binding": {
          "content_sha256": "4f1961d4828189ea71b8fa887bb7ca12dde10e7ec3e43c87e8fe8df4ec10bc72",
          "import_id": "aa99adb4-720d-4988-825a-fe957dd0096e",
          "manifest_sha256": null,
          "normalized_sha256": "4f1961d4828189ea71b8fa887bb7ca12dde10e7ec3e43c87e8fe8df4ec10bc72",
          "parse_id": null,
          "project_id": "d10dd42e-e2b2-4d2a-9316-137464a42a87",
          "source_id": "8aeea69f-044d-4126-bfa6-4ce3059fa0ca",
          "version_id": "c95284c5-6816-4cf5-aa12-7c5be32f9b99"
        },
        "request_id": "synthetic-coordination-b"
      },
      "actual": {
        "binding": {
          "content_sha256": "4f1961d4828189ea71b8fa887bb7ca12dde10e7ec3e43c87e8fe8df4ec10bc72",
          "import_id": "aa99adb4-720d-4988-825a-fe957dd0096e",
          "manifest_sha256": null,
          "normalized_sha256": "4f1961d4828189ea71b8fa887bb7ca12dde10e7ec3e43c87e8fe8df4ec10bc72",
          "parse_id": null,
          "project_id": "d10dd42e-e2b2-4d2a-9316-137464a42a87",
          "source_id": "8aeea69f-044d-4126-bfa6-4ce3059fa0ca",
          "version_id": "c95284c5-6816-4cf5-aa12-7c5be32f9b99"
        },
        "request_id": "synthetic-coordination-b"
      }
    },
    {
      "category": "duplicate",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": "unique_semantic_atom",
      "actual": "乙容器（β） 的速率 | 测量为 12 m/s，条件为x < 2且计数范围 [1,2]；例外为读数 * 异常时不适用。"
    },
    {
      "category": "evidence_identity",
      "object_id": "b-support",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 179,
      "status": "passed",
      "code": null,
      "expected": {
        "end_byte": 180,
        "start_byte": 0,
        "unit_id": "unit-0"
      },
      "actual": {
        "evidence_id": "b-support",
        "excerpt_end_byte": 179,
        "excerpt_sha256": "b3b6dd354ea7c76f48b89b2d4e8ea5706ef9c146d12563edb8027ed961c356a8",
        "excerpt_start_byte": 0,
        "quote": "适用条件：已列明；观点时间：2020年。\n乙容器（β） 的速率 | 测量为 12 m/s，条件为x < 2且计数范围 [1,2]；例外为读数 * 异常时不适用。",
        "unit_id": "unit-0"
      }
    },
    {
      "category": "evidence",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": {
        "end_byte": 179,
        "quote": "乙容器（β） 的速率 | 测量为 12 m/s，条件为x < 2且计数范围 [1,2]；例外为读数 * 异常时不适用。",
        "start_byte": 53
      },
      "actual": [
        {
          "evidence_id": "b-support",
          "excerpt_end_byte": 179,
          "excerpt_sha256": "b3b6dd354ea7c76f48b89b2d4e8ea5706ef9c146d12563edb8027ed961c356a8",
          "excerpt_start_byte": 0,
          "quote": "适用条件：已列明；观点时间：2020年。\n乙容器（β） 的速率 | 测量为 12 m/s，条件为x < 2且计数范围 [1,2]；例外为读数 * 异常时不适用。",
          "unit_id": "unit-0"
        }
      ]
    },
    {
      "category": "concept",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": {
        "components": [
          "乙容器（β）",
          "速率 | 测量"
        ],
        "compound": "乙容器（β） 速率 | 测量"
      },
      "actual": {
        "additional_source_concepts": 0,
        "linked": [
          {
            "aliases": [],
            "canonical_name": "乙容器（β）",
            "concept_id": "b-subject",
            "needs_full_explanation": true,
            "revision": 1
          },
          {
            "aliases": [],
            "canonical_name": "速率 | 测量",
            "concept_id": "b-property",
            "needs_full_explanation": true,
            "revision": 1
          }
        ]
      }
    },
    {
      "category": "content",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": "乙容器（β） 的速率 | 测量为 12 m/s，条件为x < 2且计数范围 [1,2]；例外为读数 * 异常时不适用。",
      "actual": "乙容器（β） 的速率 | 测量为 12 m/s，条件为x < 2且计数范围 [1,2]；例外为读数 * 异常时不适用。"
    },
    {
      "category": "fact",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": {
        "value": "12",
        "value_status": "known"
      },
      "actual": {
        "conditions": [
          "x < 2",
          "计数范围 [1,2]"
        ],
        "unit": "m/s",
        "value": "12",
        "value_status": "known"
      }
    },
    {
      "category": "unit",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": "m/s",
      "actual": {
        "measurement": "m/s",
        "qualifiers": [
          "m/s",
          "x < 2",
          "计数范围 [1,2]",
          "读数 * 异常时不适用"
        ]
      }
    },
    {
      "category": "condition",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": "x < 2",
      "actual": {
        "body": [
          "x<2",
          "计数范围[1,2]"
        ],
        "measurement": [
          "x < 2",
          "计数范围 [1,2]"
        ],
        "qualifiers": [
          "m/s",
          "x < 2",
          "计数范围 [1,2]",
          "读数 * 异常时不适用"
        ]
      }
    },
    {
      "category": "condition",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": "计数范围 [1,2]",
      "actual": {
        "body": [
          "x<2",
          "计数范围[1,2]"
        ],
        "measurement": [
          "x < 2",
          "计数范围 [1,2]"
        ],
        "qualifiers": [
          "m/s",
          "x < 2",
          "计数范围 [1,2]",
          "读数 * 异常时不适用"
        ]
      }
    },
    {
      "category": "condition_set",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": {
        "allowed_contexts": [],
        "required": [
          "x < 2",
          "计数范围 [1,2]"
        ]
      },
      "actual": {
        "body": [
          "x<2",
          "计数范围[1,2]"
        ],
        "measurement": [
          "x < 2",
          "计数范围 [1,2]"
        ]
      }
    },
    {
      "category": "exception",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": "读数 * 异常时不适用",
      "actual": {
        "body": [
          "读数*异常时不适用"
        ],
        "qualifiers": [
          "m/s",
          "x < 2",
          "计数范围 [1,2]",
          "读数 * 异常时不适用"
        ]
      }
    },
    {
      "category": "exception_set",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": [
        "读数 * 异常时不适用"
      ],
      "actual": [
        "读数*异常时不适用"
      ]
    },
    {
      "category": "qualifiers",
      "object_id": "b-speed",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 180,
      "status": "passed",
      "code": null,
      "expected": {
        "conditions": [
          "x < 2",
          "计数范围 [1,2]"
        ],
        "contexts": [],
        "exceptions": [
          "读数 * 异常时不适用"
        ],
        "unit": "m/s"
      },
      "actual": [
        "m/s",
        "x < 2",
        "计数范围 [1,2]",
        "读数 * 异常时不适用"
      ]
    }
  ],
  "totals": {
    "expected_atoms": 1,
    "passed": 1,
    "failed": 0,
    "needs_review": 0,
    "unprocessed": 0,
    "candidate_atoms": 1,
    "unexpected_atoms": 0,
    "duplicate_atoms": 0,
    "required_concept_groups": 1,
    "additional_source_concepts": 0
  },
  "quality": {
    "binding": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "concept": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "condition": {
      "expected": 2,
      "passed": 2,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "content": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "coverage": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "evidence": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "exception": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "exclusion": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "fact": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "schema": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "source": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    },
    "unit": {
      "expected": 1,
      "passed": 1,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "passed"
    }
  },
  "artifacts": {
    ".owner.json": {
      "sha256": "88883829cc26da9d4966260b2ee12c11b389ff6eef4a6f2022c46eb65db1fd48",
      "byte_length": 142
    },
    "candidates-original.json": {
      "sha256": "b4c35331c42fb5ae5dd8f0b63dd1678efc934be504bd7ea40268660c77b38aa2",
      "byte_length": 3086
    },
    "decision-original.json": {
      "sha256": "5cff0a8cad3a2d60ea5905aff1f5f7184fd9c28f6f02215b4c159a8458e87239",
      "byte_length": 141
    },
    "gold-original.json": {
      "sha256": "e7f47d8049459854f8614f1381000ca6d2fe1612b5e924b7d642818ad8db7b96",
      "byte_length": 789
    },
    "knowledge-ir.json": {
      "sha256": "02daa1af9130b95e135430e4b1192d1f30a82d5e0d0b504c43aeab92c8dff821",
      "byte_length": 12452
    },
    "review-original.json": {
      "sha256": "f077301cc5939e58f506c4c633ae681690dd3ad48ae250006c1d2e2b3b3dc7e2",
      "byte_length": 528
    },
    "source-snapshot.json": {
      "sha256": "a65bf8fb6f6d7f4ddfb86877ab8f31c2a5923c0ab8869501daa2d6338b6cbae0",
      "byte_length": 1070
    }
  }
}
```
