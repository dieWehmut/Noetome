# 有界来源知识核对

状态：passed；知识状态：pending；发布：false。

科学质量：not_run；研究：not_requested；网络与付费调用：0。

规则：controlled_measurement_zh_v1；复核主体：automated_source_review；解释时间："after_model_response"。

来源范围、对象、问题码、逐类别检查与分母见以下完整事实快照，与 verification.json 一致。

```json
{
  "schema_version": 1,
  "report_kind": "source_knowledge_verification",
  "producer": "source_review",
  "request_id": "4bed77a5-3409-4f72-9709-fa35ba0b752f",
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
      "project_id": "2c9dd41b-e63a-4858-bc56-18039ca25f33",
      "import_id": "9f0abe91-99c5-44d0-8e15-2e9ea48da551",
      "source_id": "50cf1923-ffea-432e-b37c-199a5223a141",
      "version_id": "d944ab9b-0eaa-4d17-a9df-53bb4a8fba1b",
      "content_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
      "normalized_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
      "parse_id": null,
      "manifest_sha256": null
    },
    "units": [
      {
        "unit_id": "unit-0",
        "locator": {
          "kind": "text",
          "span_id": "ec3a69c4-0221-49d9-a4e6-e885c4072a85",
          "ordinal": 0,
          "resource_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a"
        },
        "source_status": "parsed",
        "start_byte": 0,
        "end_byte": 140,
        "text": "在本夹具的样例 A 中，速度为 10 m/s。条件是温度 20 ℃、仅适用样例 A。此句是作者编写的工程测试数据。\n"
      }
    ],
    "expected_atoms": 1
  },
  "binding": {
    "project_id": "2c9dd41b-e63a-4858-bc56-18039ca25f33",
    "import_id": "9f0abe91-99c5-44d0-8e15-2e9ea48da551",
    "source_id": "50cf1923-ffea-432e-b37c-199a5223a141",
    "version_id": "d944ab9b-0eaa-4d17-a9df-53bb4a8fba1b",
    "content_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
    "normalized_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
    "parse_id": null,
    "manifest_sha256": null
  },
  "origin": {
    "producer": "api",
    "transport_mode": "live",
    "reviewer": "automated_source_review",
    "interpretation_timing": "after_model_response",
    "input_fingerprints": {
      "candidates.json": {
        "sha256": "92fed9123fa6f711ce76df524e4ff79a36aada79a7c4f5fb72f64f68027915fa",
        "byte_length": 3503
      },
      "decision.json": {
        "sha256": "aa6a5c97b8a558278cf7791c231b772c298f1deeef8a8ce1b57ecbb8216db15a",
        "byte_length": 140
      },
      "gold.json": {
        "sha256": "7e6decaf0a137f089be668d251ace83fb7ea0f65019f5544cca26d6b1f4c489d",
        "byte_length": 566
      },
      "review.json": {
        "sha256": "1b0f0d584ca6f84831f6629550dd4fcff64cd81d0ae775e59a649fa69611132e",
        "byte_length": 542
      }
    },
    "gold_sha256": "7e6decaf0a137f089be668d251ace83fb7ea0f65019f5544cca26d6b1f4c489d"
  },
  "rules": {
    "grammar": "controlled_measurement_zh_v1",
    "subject": "样例 A",
    "property": "速度",
    "allowed_contexts": [
      "在本夹具的样例 A 中"
    ],
    "reviewer": "automated_source_review",
    "interpretation_timing": "after_model_response"
  },
  "expected_results": [
    {
      "object_id": "expected-0",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "issue_codes": []
    }
  ],
  "candidate_results": [
    {
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
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
      "end_byte": 140,
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
      "end_byte": 140,
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
      "end_byte": 140,
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
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": {
        "atom_count": 1,
        "unit_id": "unit-0"
      },
      "actual": {
        "candidate_ids": [
          "atom-1"
        ],
        "decision": {
          "atom_ids": [
            "atom-1"
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
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": {
        "binding": {
          "content_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
          "import_id": "9f0abe91-99c5-44d0-8e15-2e9ea48da551",
          "manifest_sha256": null,
          "normalized_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
          "parse_id": null,
          "project_id": "2c9dd41b-e63a-4858-bc56-18039ca25f33",
          "source_id": "50cf1923-ffea-432e-b37c-199a5223a141",
          "version_id": "d944ab9b-0eaa-4d17-a9df-53bb4a8fba1b"
        },
        "request_id": "4bed77a5-3409-4f72-9709-fa35ba0b752f"
      },
      "actual": {
        "binding": {
          "content_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
          "import_id": "9f0abe91-99c5-44d0-8e15-2e9ea48da551",
          "manifest_sha256": null,
          "normalized_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
          "parse_id": null,
          "project_id": "2c9dd41b-e63a-4858-bc56-18039ca25f33",
          "source_id": "50cf1923-ffea-432e-b37c-199a5223a141",
          "version_id": "d944ab9b-0eaa-4d17-a9df-53bb4a8fba1b"
        },
        "request_id": "4bed77a5-3409-4f72-9709-fa35ba0b752f"
      }
    },
    {
      "category": "duplicate",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": "unique_semantic_atom",
      "actual": "在本夹具的样例 A 中，速度为 10 m/s；条件为温度 20 ℃，且仅适用样例 A。"
    },
    {
      "category": "evidence_identity",
      "object_id": "ev-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": {
        "end_byte": 140,
        "start_byte": 0,
        "unit_id": "unit-0"
      },
      "actual": {
        "evidence_id": "ev-1",
        "excerpt_end_byte": 140,
        "excerpt_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
        "excerpt_start_byte": 0,
        "quote": "在本夹具的样例 A 中，速度为 10 m/s。条件是温度 20 ℃、仅适用样例 A。此句是作者编写的工程测试数据。\n",
        "unit_id": "unit-0"
      }
    },
    {
      "category": "evidence",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": {
        "end_byte": 94,
        "quote": "在本夹具的样例 A 中，速度为 10 m/s。条件是温度 20 ℃、仅适用样例 A。",
        "start_byte": 0
      },
      "actual": [
        {
          "evidence_id": "ev-1",
          "excerpt_end_byte": 140,
          "excerpt_sha256": "35b0a7f6589dacf32a149a947986dff17369ee165607fdf6a301c398519f860a",
          "excerpt_start_byte": 0,
          "quote": "在本夹具的样例 A 中，速度为 10 m/s。条件是温度 20 ℃、仅适用样例 A。此句是作者编写的工程测试数据。\n",
          "unit_id": "unit-0"
        }
      ]
    },
    {
      "category": "concept",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": {
        "components": [
          "样例 A",
          "速度"
        ],
        "compound": "样例 A 速度"
      },
      "actual": {
        "additional_source_concepts": 2,
        "linked": [
          {
            "aliases": [],
            "canonical_name": "夹具",
            "concept_id": "concept-fixture",
            "needs_full_explanation": false,
            "revision": 1
          },
          {
            "aliases": [],
            "canonical_name": "样例 A",
            "concept_id": "concept-sample-a",
            "needs_full_explanation": false,
            "revision": 1
          },
          {
            "aliases": [],
            "canonical_name": "速度",
            "concept_id": "concept-speed",
            "needs_full_explanation": false,
            "revision": 1
          },
          {
            "aliases": [],
            "canonical_name": "温度",
            "concept_id": "concept-temperature",
            "needs_full_explanation": false,
            "revision": 1
          }
        ]
      }
    },
    {
      "category": "content",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": "样例 A 的速度为 10 m/s，条件为温度 20 ℃且仅适用样例 A。",
      "actual": "在本夹具的样例 A 中，速度为 10 m/s；条件为温度 20 ℃，且仅适用样例 A。"
    },
    {
      "category": "fact",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": {
        "value": "10",
        "value_status": "known"
      },
      "actual": {
        "conditions": [
          "在本夹具的样例 A 中",
          "温度 20 ℃",
          "仅适用样例 A"
        ],
        "unit": "m/s",
        "value": "10",
        "value_status": "known"
      }
    },
    {
      "category": "unit",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": "m/s",
      "actual": {
        "measurement": "m/s",
        "qualifiers": [
          "在本夹具的样例 A 中",
          "温度 20 ℃",
          "仅适用样例 A",
          "m/s"
        ]
      }
    },
    {
      "category": "condition",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": "温度 20 ℃",
      "actual": {
        "body": [
          "温度20℃",
          "仅适用样例A"
        ],
        "measurement": [
          "在本夹具的样例 A 中",
          "温度 20 ℃",
          "仅适用样例 A"
        ],
        "qualifiers": [
          "在本夹具的样例 A 中",
          "温度 20 ℃",
          "仅适用样例 A",
          "m/s"
        ]
      }
    },
    {
      "category": "condition",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": "仅适用样例 A",
      "actual": {
        "body": [
          "温度20℃",
          "仅适用样例A"
        ],
        "measurement": [
          "在本夹具的样例 A 中",
          "温度 20 ℃",
          "仅适用样例 A"
        ],
        "qualifiers": [
          "在本夹具的样例 A 中",
          "温度 20 ℃",
          "仅适用样例 A",
          "m/s"
        ]
      }
    },
    {
      "category": "condition_set",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": {
        "allowed_contexts": [
          "在本夹具的样例 A 中"
        ],
        "required": [
          "温度 20 ℃",
          "仅适用样例 A"
        ]
      },
      "actual": {
        "body": [
          "温度20℃",
          "仅适用样例A"
        ],
        "measurement": [
          "在本夹具的样例 A 中",
          "温度 20 ℃",
          "仅适用样例 A"
        ]
      }
    },
    {
      "category": "exception_set",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": [],
      "actual": []
    },
    {
      "category": "qualifiers",
      "object_id": "atom-1",
      "unit_id": "unit-0",
      "start_byte": 0,
      "end_byte": 140,
      "status": "passed",
      "code": null,
      "expected": {
        "conditions": [
          "温度 20 ℃",
          "仅适用样例 A"
        ],
        "contexts": [
          "在本夹具的样例 A 中"
        ],
        "exceptions": [],
        "unit": "m/s"
      },
      "actual": [
        "在本夹具的样例 A 中",
        "温度 20 ℃",
        "仅适用样例 A",
        "m/s"
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
    "additional_source_concepts": 2
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
      "expected": 0,
      "passed": 0,
      "failed": 0,
      "needs_review": 0,
      "unprocessed": 0,
      "status": "not_applicable"
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
      "sha256": "93188bccb773dd38434d51356141dc6c6ca09a9ade7ce08ae709b178d655edf1",
      "byte_length": 154
    },
    "candidates-original.json": {
      "sha256": "92fed9123fa6f711ce76df524e4ff79a36aada79a7c4f5fb72f64f68027915fa",
      "byte_length": 3503
    },
    "decision-original.json": {
      "sha256": "aa6a5c97b8a558278cf7791c231b772c298f1deeef8a8ce1b57ecbb8216db15a",
      "byte_length": 140
    },
    "gold-original.json": {
      "sha256": "7e6decaf0a137f089be668d251ace83fb7ea0f65019f5544cca26d6b1f4c489d",
      "byte_length": 566
    },
    "knowledge-ir.json": {
      "sha256": "65e918e36d58dade6bce621fd6734783f74aadf097b8cc0c12fb4edcc06576df",
      "byte_length": 12735
    },
    "review-original.json": {
      "sha256": "1b0f0d584ca6f84831f6629550dd4fcff64cd81d0ae775e59a649fa69611132e",
      "byte_length": 542
    },
    "source-snapshot.json": {
      "sha256": "05bfca17d678e8b498c316bce86ab4cbaa97b272e461c03a88b58bc6f92b3f3a",
      "byte_length": 1029
    }
  }
}
```
