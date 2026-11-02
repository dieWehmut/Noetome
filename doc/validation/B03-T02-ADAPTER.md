# B03-T02 adapter 实际红绿与集成修复

本文阅读版仅清理展示转录的行尾空白和末尾空行；[初次原件](assets/B03-T02-adapter-initial.log)、[空页wire原件](assets/B03-T02-adapter-empty-wire.log)、[最终三项修复原件](assets/B03-T02-adapter-review-fixes.log)逐字节保留，原始SHA见证据索引。

下列初次交付、空页wire修复和最终三项审查修复的原始记录均保留。当前冻结 adapter 源码 SHA 为 `5e0a8c25c531085c5d04d55b094904fb83a79c3b3f2b3cd979e163c5bfc179b8`（31,814 字节），测试源码 SHA 为 `638e3b767e8148de37098fbf2eae27b22ae09aee18e4a9d3a78a1222486ae278`（43,192 字节）；旧指纹只代表历史阶段。最后31方法通过，后续完整Python43项输出见[全回归日志](assets/B03-T02-python-tests.log)。最终修复记录原件SHA为 `0b1fc339d085574eac43c54cb646d82d05e173fd4a8b2d4a932aed7bc1f81d6c`。

# B03-T02 Python adapter verification

This record concerns only the real Python adapter and its direct subprocess tests. Rust/Core integration, persistent sandbox reports, and the task ledger are maintained by the parent agent.

Runtime verified before implementation: Windows x64, Python 3.12.14; pdfplumber 0.11.9, pdfminer.six 20251230, pypdfium2 5.14.0, Pillow 12.3.0, charset-normalizer 3.5.2, cryptography 50.0.2, cffi 2.1.1, pycparser 3.11.

## Initial RED — no adapter implementation

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -v
```

Actual exit code: 1. 19 test methods; 19 assertion failures including boundary subtests; no unittest errors. Valid PDF response assertions fail with `PDF adapter is not implemented` from an actual `-I -B -X utf8 -c` child. The initial missing-entrypoint fallback is test-only and is never embedded in production. Failure-only assertions that already pass at this stage do not prove their policies implemented.

Actual output:

```text
test_actual_small_byte_count_and_runtime_limits_never_claim_success (test_pdf_adapter.PdfAdapterTests.test_actual_small_byte_count_and_runtime_limits_never_claim_success) ... ok
test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit (test_pdf_adapter.PdfAdapterTests.test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit) ...
  test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit (test_pdf_adapter.PdfAdapterTests.test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit) (filename='damaged.pdf') ... FAIL
  test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit (test_pdf_adapter.PdfAdapterTests.test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit) (filename='encrypted.pdf') ... FAIL
test_duplicate_json_and_extra_requests_and_invalid_utf8_are_rejected (test_pdf_adapter.PdfAdapterTests.test_duplicate_json_and_extra_requests_and_invalid_utf8_are_rejected) ... ok
test_empty_document_gets_failed_document_unit (test_pdf_adapter.PdfAdapterTests.test_empty_document_gets_failed_document_unit) ... FAIL
test_existing_resource_collision_never_overwrites_human_bytes (test_pdf_adapter.PdfAdapterTests.test_existing_resource_collision_never_overwrites_human_bytes) ... ok
test_glyph_policy_overflow_does_not_publish_truncated_inventory (test_pdf_adapter.PdfAdapterTests.test_glyph_policy_overflow_does_not_publish_truncated_inventory) ... FAIL
test_partial_page_failure_retains_confirmed_page_and_true_total (test_pdf_adapter.PdfAdapterTests.test_partial_page_failure_retains_confirmed_page_and_true_total) ... FAIL
test_pdf_page_policy_overflow_does_not_truncate_page_manifest (test_pdf_adapter.PdfAdapterTests.test_pdf_page_policy_overflow_does_not_truncate_page_manifest) ... FAIL
test_real_cropbox_offset_matches_visible_origin (test_pdf_adapter.PdfAdapterTests.test_real_cropbox_offset_matches_visible_origin) ... FAIL
test_real_multiple_pages_preserves_every_physical_page (test_pdf_adapter.PdfAdapterTests.test_real_multiple_pages_preserves_every_physical_page) ... FAIL
test_real_single_page_matches_full_authoring_text_and_utf8_ranges (test_pdf_adapter.PdfAdapterTests.test_real_single_page_matches_full_authoring_text_and_utf8_ranges) ... FAIL
test_rotations_and_userunit_keep_true_transforms_and_review_status (test_pdf_adapter.PdfAdapterTests.test_rotations_and_userunit_keep_true_transforms_and_review_status) ... FAIL
test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import (test_pdf_adapter.PdfAdapterTests.test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import) ... FAIL
test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) ...
  test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) (filename='scanned-page.pdf') ... FAIL
  test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) (filename='blank-page.pdf') ... FAIL
test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) ...
  test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) (filename='table-page.pdf') ... FAIL
  test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) (filename='formula-page.pdf') ... FAIL
  test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) (filename='image-page.pdf') ... FAIL
  test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) (filename='columns-page.pdf') ... FAIL
test_unknown_and_missing_fields_and_bad_enums_are_rejected (test_pdf_adapter.PdfAdapterTests.test_unknown_and_missing_fields_and_bad_enums_are_rejected) ... ok
test_unknown_format_preserves_complete_raw_without_inventing_pages (test_pdf_adapter.PdfAdapterTests.test_unknown_format_preserves_complete_raw_without_inventing_pages) ... FAIL
test_visual_reading_order_preserves_actual_drawing_inventory (test_pdf_adapter.PdfAdapterTests.test_visual_reading_order_preserves_actual_drawing_inventory) ... FAIL
test_wrong_input_fingerprint_is_rejected_before_creating_resources (test_pdf_adapter.PdfAdapterTests.test_wrong_input_fingerprint_is_rejected_before_creating_resources) ... ok

======================================================================
FAIL: test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit (test_pdf_adapter.PdfAdapterTests.test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit) (filename='damaged.pdf')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 398, in test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit
    case, response, manifest, resources = self.run_fixture(filename)
                                          ^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit (test_pdf_adapter.PdfAdapterTests.test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit) (filename='encrypted.pdf')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 398, in test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit
    case, response, manifest, resources = self.run_fixture(filename)
                                          ^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_empty_document_gets_failed_document_unit (test_pdf_adapter.PdfAdapterTests.test_empty_document_gets_failed_document_unit)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 419, in test_empty_document_gets_failed_document_unit
    response, manifest, _ = self.valid_response(result, workspace, request)
                            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_glyph_policy_overflow_does_not_publish_truncated_inventory (test_pdf_adapter.PdfAdapterTests.test_glyph_policy_overflow_does_not_publish_truncated_inventory)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 514, in test_glyph_policy_overflow_does_not_publish_truncated_inventory
    self.assertIn(b"50000", result.stderr)
AssertionError: b'50000' not found in b'PDF adapter is not implemented\r\n'

======================================================================
FAIL: test_partial_page_failure_retains_confirmed_page_and_true_total (test_pdf_adapter.PdfAdapterTests.test_partial_page_failure_retains_confirmed_page_and_true_total)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 347, in test_partial_page_failure_retains_confirmed_page_and_true_total
    case, _, manifest, resources = self.run_fixture("partial-page-failure.pdf")
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_pdf_page_policy_overflow_does_not_truncate_page_manifest (test_pdf_adapter.PdfAdapterTests.test_pdf_page_policy_overflow_does_not_truncate_page_manifest)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 508, in test_pdf_page_policy_overflow_does_not_truncate_page_manifest
    self.assertIn(b"128", result.stderr)
AssertionError: b'128' not found in b'PDF adapter is not implemented\r\n'

======================================================================
FAIL: test_real_cropbox_offset_matches_visible_origin (test_pdf_adapter.PdfAdapterTests.test_real_cropbox_offset_matches_visible_origin)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 331, in test_real_cropbox_offset_matches_visible_origin
    case, _, manifest, resources = self.run_fixture("crop-offset.pdf")
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_real_multiple_pages_preserves_every_physical_page (test_pdf_adapter.PdfAdapterTests.test_real_multiple_pages_preserves_every_physical_page)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 326, in test_real_multiple_pages_preserves_every_physical_page
    case, _, manifest, resources = self.run_fixture("multiple-pages.pdf")
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_real_single_page_matches_full_authoring_text_and_utf8_ranges (test_pdf_adapter.PdfAdapterTests.test_real_single_page_matches_full_authoring_text_and_utf8_ranges)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 318, in test_real_single_page_matches_full_authoring_text_and_utf8_ranges
    case, response, manifest, resources = self.run_fixture("single-page.pdf")
                                          ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_rotations_and_userunit_keep_true_transforms_and_review_status (test_pdf_adapter.PdfAdapterTests.test_rotations_and_userunit_keep_true_transforms_and_review_status)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 375, in test_rotations_and_userunit_keep_true_transforms_and_review_status
    case, _, manifest, resources = self.run_fixture("rotations.pdf")
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import (test_pdf_adapter.PdfAdapterTests.test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 526, in test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import
    self.assertIn(b"pdfplumber", result.stderr)
AssertionError: b'pdfplumber' not found in b'PDF adapter is not implemented\r\n'

======================================================================
FAIL: test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) (filename='scanned-page.pdf')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 359, in test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues
    case, _, manifest, resources = self.run_fixture(filename)
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) (filename='blank-page.pdf')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 359, in test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues
    case, _, manifest, resources = self.run_fixture(filename)
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) (filename='table-page.pdf')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 367, in test_table_formula_image_and_columns_remain_honestly_needs_review
    case, _, manifest, resources = self.run_fixture(filename)
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) (filename='formula-page.pdf')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 367, in test_table_formula_image_and_columns_remain_honestly_needs_review
    case, _, manifest, resources = self.run_fixture(filename)
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) (filename='image-page.pdf')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 367, in test_table_formula_image_and_columns_remain_honestly_needs_review
    case, _, manifest, resources = self.run_fixture(filename)
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) (filename='columns-page.pdf')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 367, in test_table_formula_image_and_columns_remain_honestly_needs_review
    case, _, manifest, resources = self.run_fixture(filename)
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_unknown_format_preserves_complete_raw_without_inventing_pages (test_pdf_adapter.PdfAdapterTests.test_unknown_format_preserves_complete_raw_without_inventing_pages)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 410, in test_unknown_format_preserves_complete_raw_without_inventing_pages
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


======================================================================
FAIL: test_visual_reading_order_preserves_actual_drawing_inventory (test_pdf_adapter.PdfAdapterTests.test_visual_reading_order_preserves_actual_drawing_inventory)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 338, in test_visual_reading_order_preserves_actual_drawing_inventory
    case, _, manifest, resources = self.run_fixture("layout-order.pdf")
                                   ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 139, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in valid_response
    self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", "replace"))
AssertionError: 1 != 0 : PDF adapter is not implemented


----------------------------------------------------------------------
Ran 19 tests in 1.565s

FAILED (failures=19)
```

## First implementation — native Windows argv failure

The first implementation did not start: actual source length was 33,495 characters and Python's native argv encoding was 34,712 characters. The same direct test command returned exit 1: `Ran 19 tests in 0.257s`, `FAILED (errors=73)`, with the actual cause `FileNotFoundError: [WinError 206] 文件名或扩展名太长。`. No PDF parse success was inferred from that run. Readable message/structure reduction and normal single-quote formatting allowed the direct native subprocess to start. The parent chose a generated compressed payload and short trusted `-c` bootstrap for the final Core integration; this agent does not own that payload/build tool.

## First adapter GREEN and subsequent integration corrections

The first isolated adapter run returned `Ran 19 tests in 9.722s`, `OK`, exit 0. A prior intermediate run had two failures: a test-only metadata prelude preceding the future import, and the authoring glyph oracle incorrectly using visual order for `layout-order.pdf`. The prelude now follows the future declaration. The parent corrected only the authoring oracle from the original drawing commands; PDF input bytes were unchanged. These early adapter tests had an incorrect assumption that a region included LF, so they did not establish Core integration acceptance.

The parent then reported the first real compiled Core failure as `PDF_TEXT`: a region end offset included its appended LF (reported probe range `[0,40)` for 39 line bytes). The regression below directly compares the authoring line's exact UTF8 bytes, without `strip` or `rstrip`. The final region excludes LF, while the complete page artifact still appends LF and advances the next region start by `end_byte + 1`.

## Diagnostic cap regression — RED / GREEN

RED command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -k 'small_explicit_byte_cap' -v
```

Actual RED exit 1:

```text
test_invalid_request_diagnostic_respects_small_explicit_byte_cap (test_pdf_adapter.PdfAdapterTests.test_invalid_request_diagnostic_respects_small_explicit_byte_cap) ... FAIL

======================================================================
FAIL: test_invalid_request_diagnostic_respects_small_explicit_byte_cap (test_pdf_adapter.PdfAdapterTests.test_invalid_request_diagnostic_respects_small_explicit_byte_cap)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 551, in test_invalid_request_diagnostic_respects_small_explicit_byte_cap
    self.assert_rejected(result, request)
  File "D:\Noetome\tests\test_pdf_adapter.py", line 324, in assert_rejected
    self.assertLessEqual(len(result.stderr), limit)
AssertionError: 42 not less than or equal to 7

----------------------------------------------------------------------
Ran 1 test in 0.097s

FAILED (failures=1)
```

After extracting a valid explicit diagnostic cap before the remaining request validation, the same command returned exit 0: `Ran 1 test in 0.115s`, `OK`. Unknown input fields with the requested 7-byte cap now return no stdout and at most 7 opaque diagnostic bytes.

## Finite coordinate underflow regression — RED / GREEN

A temporary, independently authored decimal PDF has finite positive raw box dimensions and UserUnit whose product underflows to 0 pt. A zero displayed area must be a failed page with null geometry, not an unknown blank review page.

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -k 'underflowed_display_geometry' -v
```

Actual RED exit 1:

```text
test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review (test_pdf_adapter.PdfAdapterTests.test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review) ... FAIL

======================================================================
FAIL: test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review (test_pdf_adapter.PdfAdapterTests.test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 398, in test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review
    self.assertEqual(manifest["status"], "failed")
AssertionError: 'needs_review' != 'failed'
- needs_review
+ failed


----------------------------------------------------------------------
Ran 1 test in 0.199s

FAILED (failures=1)
```

Actual GREEN exit 0:

```text
test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review (test_pdf_adapter.PdfAdapterTests.test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review) ... ok

----------------------------------------------------------------------
Ran 1 test in 0.181s

OK
```

## Exact region line bytes regression — RED / GREEN

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -k 'region_slice_is_exact_authoring_line' -v
```

Actual RED exit 1:

```text
test_region_slice_is_exact_authoring_line_excluding_appended_lf (test_pdf_adapter.PdfAdapterTests.test_region_slice_is_exact_authoring_line_excluding_appended_lf) ... FAIL

======================================================================
FAIL: test_region_slice_is_exact_authoring_line_excluding_appended_lf (test_pdf_adapter.PdfAdapterTests.test_region_slice_is_exact_authoring_line_excluding_appended_lf)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 342, in test_region_slice_is_exact_authoring_line_excluding_appended_lf
    self.assertEqual(text_bytes[reference["start_byte"]:reference["end_byte"]],
AssertionError: b'Noetome PDF fixture - physical page one\n' != b'Noetome PDF fixture - physical page one'

----------------------------------------------------------------------
Ran 1 test in 0.177s

FAILED (failures=1)
```

Actual GREEN exit 0:

```text
test_region_slice_is_exact_authoring_line_excluding_appended_lf (test_pdf_adapter.PdfAdapterTests.test_region_slice_is_exact_authoring_line_excluding_appended_lf) ... ok

----------------------------------------------------------------------
Ran 1 test in 0.187s

OK
```

## Final complete Python GREEN

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_*.py' -v
```

Actual exit 0: 40 methods, comprising 28 direct adapter methods and 12 existing dev-log regressions, with no failures/errors/skips. Adapter methods exercised 89 real isolated subprocess invocations, including table-driven request and dependency failures. The suite contains independently authored positive/negative PDF inputs, not only internal DTO mirrors.

Actual final output (line endings normalized for Markdown):

```text
test_atomic_replace_failure_preserves_previous_ledger (test_devlog.DevlogTests.test_atomic_replace_failure_preserves_previous_ledger) ... ok
test_batch_requires_current_round_completed (test_devlog.DevlogTests.test_batch_requires_current_round_completed) ... ok
test_block_reopen_and_preserve_completion_history (test_devlog.DevlogTests.test_block_reopen_and_preserve_completion_history) ... ok
test_check_is_read_only_and_detects_stale_view (test_devlog.DevlogTests.test_check_is_read_only_and_detects_stale_view) ... ok
test_completion_rejects_self_referencing_ledger_and_views (test_devlog.DevlogTests.test_completion_rejects_self_referencing_ledger_and_views) ... ok
test_completion_requires_explanation_and_existing_internal_artifact (test_devlog.DevlogTests.test_completion_requires_explanation_and_existing_internal_artifact) ... ok
test_concurrent_writer_is_rejected (test_devlog.DevlogTests.test_concurrent_writer_is_rejected) ... ok
test_dependencies_batch_and_single_active_task (test_devlog.DevlogTests.test_dependencies_batch_and_single_active_task) ... ok
test_display_time_uses_shanghai_and_crosses_utc_date_boundary (test_devlog.DevlogTests.test_display_time_uses_shanghai_and_crosses_utc_date_boundary) ... ok
test_invalid_dependencies_and_unrecorded_block_are_rejected (test_devlog.DevlogTests.test_invalid_dependencies_and_unrecorded_block_are_rejected) ... ok
test_reopening_dependency_cannot_invalidate_active_consumer (test_devlog.DevlogTests.test_reopening_dependency_cannot_invalidate_active_consumer) ... ok
test_view_failure_leaves_valid_committed_ledger_and_can_be_repaired (test_devlog.DevlogTests.test_view_failure_leaves_valid_committed_ledger_and_can_be_repaired) ... ok
test_actual_small_byte_count_and_runtime_limits_never_claim_success (test_pdf_adapter.PdfAdapterTests.test_actual_small_byte_count_and_runtime_limits_never_claim_success) ... ok
test_annotation_uses_raw_rotation_transform_without_following_action (test_pdf_adapter.PdfAdapterTests.test_annotation_uses_raw_rotation_transform_without_following_action) ... ok
test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit (test_pdf_adapter.PdfAdapterTests.test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit) ... ok
test_duplicate_json_and_extra_requests_and_invalid_utf8_are_rejected (test_pdf_adapter.PdfAdapterTests.test_duplicate_json_and_extra_requests_and_invalid_utf8_are_rejected) ... ok
test_empty_document_gets_failed_document_unit (test_pdf_adapter.PdfAdapterTests.test_empty_document_gets_failed_document_unit) ... ok
test_enumeration_failure_keeps_prefix_and_unknown_total_with_document_unit (test_pdf_adapter.PdfAdapterTests.test_enumeration_failure_keeps_prefix_and_unknown_total_with_document_unit) ... ok
test_existing_resource_collision_never_overwrites_human_bytes (test_pdf_adapter.PdfAdapterTests.test_existing_resource_collision_never_overwrites_human_bytes) ... ok
test_glyph_policy_overflow_does_not_publish_truncated_inventory (test_pdf_adapter.PdfAdapterTests.test_glyph_policy_overflow_does_not_publish_truncated_inventory) ... ok
test_glyphs_outside_cropbox_fail_page_instead_of_silently_clamping (test_pdf_adapter.PdfAdapterTests.test_glyphs_outside_cropbox_fail_page_instead_of_silently_clamping) ... ok
test_invalid_request_diagnostic_respects_small_explicit_byte_cap (test_pdf_adapter.PdfAdapterTests.test_invalid_request_diagnostic_respects_small_explicit_byte_cap) ... ok
test_overlapping_glyphs_are_preserved_and_require_review (test_pdf_adapter.PdfAdapterTests.test_overlapping_glyphs_are_preserved_and_require_review) ... ok
test_partial_page_failure_retains_confirmed_page_and_true_total (test_pdf_adapter.PdfAdapterTests.test_partial_page_failure_retains_confirmed_page_and_true_total) ... ok
test_pdf_page_policy_overflow_does_not_truncate_page_manifest (test_pdf_adapter.PdfAdapterTests.test_pdf_page_policy_overflow_does_not_truncate_page_manifest) ... ok
test_raw_pdf_policy_overflow_fails_before_any_resource_is_written (test_pdf_adapter.PdfAdapterTests.test_raw_pdf_policy_overflow_fails_before_any_resource_is_written) ... ok
test_real_cropbox_offset_matches_visible_origin (test_pdf_adapter.PdfAdapterTests.test_real_cropbox_offset_matches_visible_origin) ... ok
test_real_multiple_pages_preserves_every_physical_page (test_pdf_adapter.PdfAdapterTests.test_real_multiple_pages_preserves_every_physical_page) ... ok
test_real_single_page_matches_full_authoring_text_and_utf8_ranges (test_pdf_adapter.PdfAdapterTests.test_real_single_page_matches_full_authoring_text_and_utf8_ranges) ... ok
test_region_slice_is_exact_authoring_line_excluding_appended_lf (test_pdf_adapter.PdfAdapterTests.test_region_slice_is_exact_authoring_line_excluding_appended_lf) ... ok
test_rotations_and_userunit_keep_true_transforms_and_review_status (test_pdf_adapter.PdfAdapterTests.test_rotations_and_userunit_keep_true_transforms_and_review_status) ... ok
test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import (test_pdf_adapter.PdfAdapterTests.test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import) ... ok
test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) ... ok
test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) ... ok
test_total_glyph_policy_overflow_never_publishes_incomplete_manifest (test_pdf_adapter.PdfAdapterTests.test_total_glyph_policy_overflow_never_publishes_incomplete_manifest) ... ok
test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review (test_pdf_adapter.PdfAdapterTests.test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review) ... ok
test_unknown_and_missing_fields_and_bad_enums_are_rejected (test_pdf_adapter.PdfAdapterTests.test_unknown_and_missing_fields_and_bad_enums_are_rejected) ... ok
test_unknown_format_preserves_complete_raw_without_inventing_pages (test_pdf_adapter.PdfAdapterTests.test_unknown_format_preserves_complete_raw_without_inventing_pages) ... ok
test_visual_reading_order_preserves_actual_drawing_inventory (test_pdf_adapter.PdfAdapterTests.test_visual_reading_order_preserves_actual_drawing_inventory) ... ok
test_wrong_input_fingerprint_is_rejected_before_creating_resources (test_pdf_adapter.PdfAdapterTests.test_wrong_input_fingerprint_is_rejected_before_creating_resources) ... ok

----------------------------------------------------------------------
Ran 40 tests in 16.411s

OK
```

## Covered behavior and final source fingerprint

- Fixed authoring SHA/length, full single/multiple/cropped page text, all authoring glyph ordinals/text/bboxes, physical/display page identity, original drawing inventory and canonical visual line order, exact UTF8 line ranges without LF, resource bytes/SHA and exact file inventory.
- Original raw geometry with true rotation-specific CropBox offsets, all 0/90/180/270 transforms including nonzero MediaBox/CropBox origins and UserUnit=2. Native raw annotation rectangle transformation retains raw evidence without following its URI.
- Conservative needs_review for scan/no-text layer, unconfirmed blank, table/vector/image associations, superscript/formula, multiple columns, overlap, rotation and UserUnit. Damaged/encrypted/unknown/zero-page documents fail with a resolving document unit.
- Page-level invalid geometry and glyphs outside CropBox fail; successfully extracted neighboring pages remain. A true pdfminer raw enumeration exception preserves only the known prefix, page_count=null and a separate failed document unit.
- Unknown/missing fields, duplicate JSON, non-UTF8/trailing requests, wrong fingerprints, existing human resource collision, all requested byte/count/runtime limits, actual 16MiB raw/128-page/50,000-per-page/200,000-total glyph overflow, and every one of the eight dependency pins failing separately. Input paths include Unicode, spaces and literal `$()`; every process checks unchanged selected input and outside sentinel bytes.

Final files:

| Path | UTF8 bytes | SHA256 |
| --- | ---: | --- |
| sidecars/pdf_parser/adapter.py | 30977 | e98047b3e7823f4672182e764d65f93f8d31ffcae0e3e97feaab793c0175c250 |
| tests/test_pdf_adapter.py | 36180 | 17259f41b881acf36ba3a226170df54c2e8fa832a7b17c957db25c6720abf769 |

Final authoring expected.json SHA256: `9c9e8b91ef871c73d38d08637d1a39a11f1a6767a7b6927359f2c7c4b08656b8`. This agent did not change or regenerate fixture/expected files.

The final uncompressed adapter is 30,977 ASCII characters; the direct bundled-interpreter native argv is 31,136 characters. The parent-selected compressed bootstrap remains an integration check to be performed by the parent. AST parsing and independent trailing-whitespace checks passed for both newly created Python files. Whole tracked working-tree `git diff --check` returned exit 0 with EOL conversion warnings for concurrent Rust files. No staging, commits, pushes, dev-log mutations, Rust edits, dependency-lock/license edits or payload generation were performed by this agent.

## Limits of this evidence

The adapter supplies genuine byte/coordinate evidence and conservative review heuristics. It does not extract OCR, image/table/formula structure, knowledge IR or scientifically validated content, and consistency with its glyph inventory cannot prove arbitrary PDF completeness. All tests use the stated Windows x64 environment and bounded public/temp engineering inputs. This is not an OS/network sandbox claim or a 1,000-page, 24-hour, cross-platform or scientific-quality benchmark. Core Job timeout/cancellation, final resource guards, compressed-bootstrap byte identity, compiled CLI/inspect reports and final task/Git acceptance remain the parent's verification scope.




# B03-T02 empty page / protocol-v1 integration repair

This is a separate record. The previous adapter report remains unchanged as historical evidence. This round changes only adapter.py and test_pdf_adapter.py; it does not alter the generic host, dependency locks, payload/build tool, dev-log state or Git index.

## Observed real integration failure

The parent reported the compressed compiled CLI rejecting scanned-page.pdf (old payload, target/compiler-temp/b03-t02-scan-probe-01) as `SIDECAR_UNITS: invalid UTF8 text range`. The unchanged v1 host rejects every non-null range with start_byte >= end_byte. The prior adapter kept the real empty artifact but incorrectly declared its generic range as [0,0). blank-page.pdf had the same error.

## Contract and correction

For a successfully extracted empty page, retain page.text with byte_length=0, the genuine empty UTF8 file/SHA and resource_ids including page-text-000000. Declare generic unit.kind=unknown and the required nullable unit.text=null. Nonempty pages retain kind=text and their complete strictly nonempty half-open UTF8 range. Every non-null generic unit range is now tested with start_byte < end_byte. Scanned and unknown blank pages remain needs_review with blocking issues and raw evidence.

## Targeted RED / GREEN

Exact command for both runs:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -k 'scan_and_unknown_blank' -v
```

Actual RED: exit 1; one method, two failed real-PDF subtests:

```text
test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) ...
  test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) (filename='scanned-page.pdf') ... FAIL
  test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) (filename='blank-page.pdf') ... FAIL

======================================================================
FAIL: test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) (filename='scanned-page.pdf')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 448, in test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues
    case, response, manifest, resources = self.run_fixture(filename)
                                          ^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 237, in valid_response
    self.assertIsNone(unit["text"])
AssertionError: {'resource_id': 'page-text-000000', 'start_byte': 0, 'end_byte': 0} is not None

======================================================================
FAIL: test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) (filename='blank-page.pdf')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 448, in test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues
    case, response, manifest, resources = self.run_fixture(filename)
                                          ^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 143, in run_fixture
    response, manifest, resources = self.valid_response(result, workspace, request)
                                    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  File "D:\Noetome\tests\test_pdf_adapter.py", line 237, in valid_response
    self.assertIsNone(unit["text"])
AssertionError: {'resource_id': 'page-text-000000', 'start_byte': 0, 'end_byte': 0} is not None

----------------------------------------------------------------------
Ran 1 test in 0.458s

FAILED (failures=2)
```

Actual GREEN: exit 0:

```text
test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) ... ok

----------------------------------------------------------------------
Ran 1 test in 0.325s

OK
```

## Full 28-method adapter regression

Exact command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -v
```

Actual exit 0; all 28 methods / 89 isolated subprocess invocations pass, without failures, errors or skips. Complete output (line endings normalized for Markdown):

```text
test_actual_small_byte_count_and_runtime_limits_never_claim_success (test_pdf_adapter.PdfAdapterTests.test_actual_small_byte_count_and_runtime_limits_never_claim_success) ... ok
test_annotation_uses_raw_rotation_transform_without_following_action (test_pdf_adapter.PdfAdapterTests.test_annotation_uses_raw_rotation_transform_without_following_action) ... ok
test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit (test_pdf_adapter.PdfAdapterTests.test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit) ... ok
test_duplicate_json_and_extra_requests_and_invalid_utf8_are_rejected (test_pdf_adapter.PdfAdapterTests.test_duplicate_json_and_extra_requests_and_invalid_utf8_are_rejected) ... ok
test_empty_document_gets_failed_document_unit (test_pdf_adapter.PdfAdapterTests.test_empty_document_gets_failed_document_unit) ... ok
test_enumeration_failure_keeps_prefix_and_unknown_total_with_document_unit (test_pdf_adapter.PdfAdapterTests.test_enumeration_failure_keeps_prefix_and_unknown_total_with_document_unit) ... ok
test_existing_resource_collision_never_overwrites_human_bytes (test_pdf_adapter.PdfAdapterTests.test_existing_resource_collision_never_overwrites_human_bytes) ... ok
test_glyph_policy_overflow_does_not_publish_truncated_inventory (test_pdf_adapter.PdfAdapterTests.test_glyph_policy_overflow_does_not_publish_truncated_inventory) ... ok
test_glyphs_outside_cropbox_fail_page_instead_of_silently_clamping (test_pdf_adapter.PdfAdapterTests.test_glyphs_outside_cropbox_fail_page_instead_of_silently_clamping) ... ok
test_invalid_request_diagnostic_respects_small_explicit_byte_cap (test_pdf_adapter.PdfAdapterTests.test_invalid_request_diagnostic_respects_small_explicit_byte_cap) ... ok
test_overlapping_glyphs_are_preserved_and_require_review (test_pdf_adapter.PdfAdapterTests.test_overlapping_glyphs_are_preserved_and_require_review) ... ok
test_partial_page_failure_retains_confirmed_page_and_true_total (test_pdf_adapter.PdfAdapterTests.test_partial_page_failure_retains_confirmed_page_and_true_total) ... ok
test_pdf_page_policy_overflow_does_not_truncate_page_manifest (test_pdf_adapter.PdfAdapterTests.test_pdf_page_policy_overflow_does_not_truncate_page_manifest) ... ok
test_raw_pdf_policy_overflow_fails_before_any_resource_is_written (test_pdf_adapter.PdfAdapterTests.test_raw_pdf_policy_overflow_fails_before_any_resource_is_written) ... ok
test_real_cropbox_offset_matches_visible_origin (test_pdf_adapter.PdfAdapterTests.test_real_cropbox_offset_matches_visible_origin) ... ok
test_real_multiple_pages_preserves_every_physical_page (test_pdf_adapter.PdfAdapterTests.test_real_multiple_pages_preserves_every_physical_page) ... ok
test_real_single_page_matches_full_authoring_text_and_utf8_ranges (test_pdf_adapter.PdfAdapterTests.test_real_single_page_matches_full_authoring_text_and_utf8_ranges) ... ok
test_region_slice_is_exact_authoring_line_excluding_appended_lf (test_pdf_adapter.PdfAdapterTests.test_region_slice_is_exact_authoring_line_excluding_appended_lf) ... ok
test_rotations_and_userunit_keep_true_transforms_and_review_status (test_pdf_adapter.PdfAdapterTests.test_rotations_and_userunit_keep_true_transforms_and_review_status) ... ok
test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import (test_pdf_adapter.PdfAdapterTests.test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import) ... ok
test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) ... ok
test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) ... ok
test_total_glyph_policy_overflow_never_publishes_incomplete_manifest (test_pdf_adapter.PdfAdapterTests.test_total_glyph_policy_overflow_never_publishes_incomplete_manifest) ... ok
test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review (test_pdf_adapter.PdfAdapterTests.test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review) ... ok
test_unknown_and_missing_fields_and_bad_enums_are_rejected (test_pdf_adapter.PdfAdapterTests.test_unknown_and_missing_fields_and_bad_enums_are_rejected) ... ok
test_unknown_format_preserves_complete_raw_without_inventing_pages (test_pdf_adapter.PdfAdapterTests.test_unknown_format_preserves_complete_raw_without_inventing_pages) ... ok
test_visual_reading_order_preserves_actual_drawing_inventory (test_pdf_adapter.PdfAdapterTests.test_visual_reading_order_preserves_actual_drawing_inventory) ... ok
test_wrong_input_fingerprint_is_rejected_before_creating_resources (test_pdf_adapter.PdfAdapterTests.test_wrong_input_fingerprint_is_rejected_before_creating_resources) ... ok

----------------------------------------------------------------------
Ran 28 tests in 15.284s

OK
```

## Frozen handoff

| File | UTF8 bytes | SHA256 |
| --- | ---: | --- |
| sidecars/pdf_parser/adapter.py | 31014 | b4ee4721fe0e6eba18729e9d4fdcb2355d295a6541c277cbd79cead17af456b2 |
| tests/test_pdf_adapter.py | 37042 | 0ce75bd0f6016e4c3648694e5414d7b55bafc828035ed2ac1e751f763e4dbf77 |

Both files pass AST parsing and independent trailing-whitespace checks. The uncompressed source is 31,014 ASCII characters; native argv using the bundled interpreter is 31,173 characters. The parent must regenerate the compressed payload against the new source hash and rerun compiled CLI/Core/inspect checks. No further source edits are planned for this handoff. The tests remain bounded Windows x64 public engineering fixtures; they do not establish OCR, arbitrary-PDF completeness, scientific quality or cross-platform/performance benchmarks.



## 最终独立审查的三项修复记录

# B03-T02 adapter independent-review repair evidence

本记录仅覆盖当前 B03-T02 的 Python adapter 及其真实进程测试。父代理负责 Rust Core、生产压缩 payload、固定作者 fixture、标准、账本及 Git；本代理只修改 adapter.py、test_pdf_adapter.py 并新增此证据。未启动后续产品任务。

父代理独立审查已用自造真实 PDF 发现：错开基线的双列被误报 parsed；仅三个空格的页面被误报 parsed；ToUnicode 单 glyph 解码为 `A ` 后接 `B` 时额外插入空格，真实 Core CLI 因 PDF_TEXT 拒绝。此轮专测用标准库动态构造 PDF/ToUnicode CMap，literal 预期来自明确绘制和映射字符串，测试未硬编码 target 复现目录，也未依据 adapter 输出重新生成 fixture。此前完整证据文件保持原 SHA，历史失败不覆盖。

## Final source and scope

| File | Bytes | SHA256 |
| --- | ---: | --- |
| `D:\Noetome\sidecars\pdf_parser\adapter.py` | 31814 | `5e0a8c25c531085c5d04d55b094904fb83a79c3b3f2b3cd979e163c5bfc179b8` |
| `D:\Noetome\tests\test_pdf_adapter.py` | 43192 | `638e3b767e8148de37098fbf2eae27b22ae09aee18e4a9d3a78a1222486ae278` |
| `D:\Noetome\.artifacts\b03-t02-adapter-report.md` | 41921 | `90196dc4a08afbb17a39b4acf31517c5e78616812a31c4e19e76b6d87ec01778` |
| `D:\Noetome\.artifacts\b03-t02-adapter-empty-wire-red-green.md` | 11015 | `81685b5c96a63f7d07e254c03d0b947c51b3e04231c93e161b66d1738b8828d8` |

适配器最终源码冻结为 `5e0a8c25c531085c5d04d55b094904fb83a79c3b3f2b3cd979e163c5bfc179b8`。后续生产 payload 必须由父代理从此源重生成、验证解压字节一致，并重新链接真实 CLI；本记录中的适配器绿灯不替代该产品集成检查。

## Final behavior

- 相邻 canonical line regions 的实际 glyph envelope 若水平间隔大于 36pt，且 top_delta 在 `[0, max(72pt, 6 * pair最大行高)]` 中，添加 blocking `PDF_POSSIBLE_COLUMNS`，页与文档为 needs_review。行组只构建一次，之后线性检查相邻包络。保存全部 glyph 原绘制 inventory 和 canonical 行文本；此启发式不声称完成列分割或覆盖所有多栏。
- 没有非空白解码内容时报告 `PDF_UNKNOWN_BLANK`（纯无 glyph 图像页仍使用既有 PDF_NO_TEXT_LAYER）。三空格样例保留 glyphs3、artifact `b'   \n'`、region `[0,3)` 和 generic kind=text/range `[0,4)`。真正零字节 artifact 仍存在于资源及 page.text 中，generic kind=unknown/text=null，所有非 null generic 文本范围满足 start<end。
- 仅 gap>3pt 的插入空格检查 previous.text[-1].isspace() 与 current.text[0].isspace()；不 strip 或 rstrip 实际字形内容。gap>36pt 与重叠几何问题独立于空白边界。多码点 glyph 文本添加 blocking `PDF_MULTICODEPOINT_GLYPH`，保留原始 PDF 和字形，不承诺这种 font width/encoding 的独立定位支持。
- 第三专测包含六个真实 PDF 子例：尾空格、首空格、两边真实空格、编码尾空格后大间距、单独空白 glyph 前大间距、单独空白 glyph 与前 glyph 重叠。前三者 literal 分别为 `A B\n`、`A B\n`、`A  B\n`；后两者即使为单码点也必须因实际几何问题需要复核。

## Real test runtime and native launch

实际解释器：`C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe`，Python `3.12.14`。

| Distribution | importlib.metadata.version |
| --- | --- |
| pdfplumber | 0.11.9 |
| pdfminer.six | 20251230 |
| pypdfium2 | 5.14.0 |
| Pillow | 12.3.0 |
| charset-normalizer | 3.5.2 |
| cryptography | 50.0.2 |
| cffi | 2.1.1 |
| pycparser | 3.11 |

所有专测仍调用真正隔离子进程：`sys.executable -I -B -X utf8 -c <trusted wrapper>`；stdin 为单条真实请求，每次使用新的 TemporaryDirectory。测试标准库 zlib9 压缩包括测试 prefix 在内的可信动态 source，并断言解压字节等于编译 source。未写临时 driver、未通过 shell 启动子进程、未改生产 payload。

最终无 prefix source UTF8 31814 bytes，zlib9 8926 bytes；wrapper 17949 chars，包含此绝对解释器的原生命令 18059 chars（含终止 NUL）。完整未压缩 source 命令 31974 chars（含 NUL）。含 runtime dependency prefix 的既有 fail-closed 测试也在本轮完整 suite 中实际通过；不再压缩或删减可读源码以适配 argv。

当前 runner 实际加载更新后的 14 个固定作者 PDF，全部 raw SHA/长度逐一核对通过，包括父代理新增的 columns/blank 第二页与所有 rotation/crop/UserUnit glyph oracle。固定 expected 未由本代理修改；作者渲染计数和科学/性能评价属于独立证据。

## Sequential red and green command output

下面保留原始测试输出及当时 line numbers；之后新增测试导致源码行号变化不改写历史输出。每项专测先红后仅修对应行为再绿，最后完整 31 methods 运行。

### 1. Staggered columns: red

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -k staggered_columns -v
```

Exit code: `1`.

```text
test_staggered_columns_retain_all_text_and_require_review (test_pdf_adapter.PdfAdapterTests.test_staggered_columns_retain_all_text_and_require_review) ... FAIL

======================================================================
FAIL: test_staggered_columns_retain_all_text_and_require_review (test_pdf_adapter.PdfAdapterTests.test_staggered_columns_retain_all_text_and_require_review)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 403, in test_staggered_columns_retain_all_text_and_require_review
    self.assertEqual(page["status"], "needs_review")
AssertionError: 'parsed' != 'needs_review'
- parsed
+ needs_review


----------------------------------------------------------------------
Ran 1 test in 0.171s

FAILED (failures=1)
```

### 1. Staggered columns: green

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -k staggered_columns -v
```

Exit code: `0`.

```text
test_staggered_columns_retain_all_text_and_require_review (test_pdf_adapter.PdfAdapterTests.test_staggered_columns_retain_all_text_and_require_review) ... ok

----------------------------------------------------------------------
Ran 1 test in 0.179s

OK
```

### 2. Whitespace-only page: red

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -k whitespace_only -v
```

Exit code: `1`.

```text
test_whitespace_only_page_retains_nonempty_artifact_and_requires_review (test_pdf_adapter.PdfAdapterTests.test_whitespace_only_page_retains_nonempty_artifact_and_requires_review) ... FAIL

======================================================================
FAIL: test_whitespace_only_page_retains_nonempty_artifact_and_requires_review (test_pdf_adapter.PdfAdapterTests.test_whitespace_only_page_retains_nonempty_artifact_and_requires_review)
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 503, in test_whitespace_only_page_retains_nonempty_artifact_and_requires_review
    self.assertEqual(page["status"], "needs_review")
AssertionError: 'parsed' != 'needs_review'
- parsed
+ needs_review


----------------------------------------------------------------------
Ran 1 test in 0.165s

FAILED (failures=1)
```

### 2. Whitespace-only page: green

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -k whitespace_only -v
```

Exit code: `0`.

```text
test_whitespace_only_page_retains_nonempty_artifact_and_requires_review (test_pdf_adapter.PdfAdapterTests.test_whitespace_only_page_retains_nonempty_artifact_and_requires_review) ... ok

----------------------------------------------------------------------
Ran 1 test in 0.166s

OK
```

### 3. ToUnicode boundary/geometry: red

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -k tounicode_glyph_boundary -v
```

Exit code: `1`.

```text
test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) ...
  test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='trailing_space') ... FAIL
  test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='leading_space') ... FAIL
  test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='both_boundary_spaces') ... FAIL
  test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='large_gap_after_encoded_space') ... FAIL
  test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='large_gap_before_whitespace_glyph') ... FAIL
  test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='overlap_before_whitespace_glyph') ... FAIL

======================================================================
FAIL: test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='trailing_space')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 544, in test_tounicode_glyph_boundary_spaces_and_geometry_require_review
    self.assertEqual(text_bytes, expected)
AssertionError: b'A  B\n' != b'A B\n'

======================================================================
FAIL: test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='leading_space')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 544, in test_tounicode_glyph_boundary_spaces_and_geometry_require_review
    self.assertEqual(text_bytes, expected)
AssertionError: b'A  B\n' != b'A B\n'

======================================================================
FAIL: test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='both_boundary_spaces')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 544, in test_tounicode_glyph_boundary_spaces_and_geometry_require_review
    self.assertEqual(text_bytes, expected)
AssertionError: b'A   B\n' != b'A  B\n'

======================================================================
FAIL: test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='large_gap_after_encoded_space')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 544, in test_tounicode_glyph_boundary_spaces_and_geometry_require_review
    self.assertEqual(text_bytes, expected)
AssertionError: b'A  B\n' != b'A B\n'

======================================================================
FAIL: test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='large_gap_before_whitespace_glyph')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 548, in test_tounicode_glyph_boundary_spaces_and_geometry_require_review
    self.assertEqual(page["status"], "needs_review")
AssertionError: 'parsed' != 'needs_review'
- parsed
+ needs_review


======================================================================
FAIL: test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) (case='overlap_before_whitespace_glyph')
----------------------------------------------------------------------
Traceback (most recent call last):
  File "D:\Noetome\tests\test_pdf_adapter.py", line 548, in test_tounicode_glyph_boundary_spaces_and_geometry_require_review
    self.assertEqual(page["status"], "needs_review")
AssertionError: 'parsed' != 'needs_review'
- parsed
+ needs_review


----------------------------------------------------------------------
Ran 1 test in 1.036s

FAILED (failures=6)
```

### 3. ToUnicode boundary/geometry: green

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -k tounicode_glyph_boundary -v
```

Exit code: `0`.

```text
test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) ... ok

----------------------------------------------------------------------
Ran 1 test in 0.966s

OK
```

### Complete adapter suite: green

Command:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p 'test_pdf_adapter.py' -v
```

Exit code: `0`.

```text
test_actual_small_byte_count_and_runtime_limits_never_claim_success (test_pdf_adapter.PdfAdapterTests.test_actual_small_byte_count_and_runtime_limits_never_claim_success) ... ok
test_annotation_uses_raw_rotation_transform_without_following_action (test_pdf_adapter.PdfAdapterTests.test_annotation_uses_raw_rotation_transform_without_following_action) ... ok
test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit (test_pdf_adapter.PdfAdapterTests.test_damaged_and_encrypted_keep_raw_evidence_and_document_failure_unit) ... ok
test_duplicate_json_and_extra_requests_and_invalid_utf8_are_rejected (test_pdf_adapter.PdfAdapterTests.test_duplicate_json_and_extra_requests_and_invalid_utf8_are_rejected) ... ok
test_empty_document_gets_failed_document_unit (test_pdf_adapter.PdfAdapterTests.test_empty_document_gets_failed_document_unit) ... ok
test_enumeration_failure_keeps_prefix_and_unknown_total_with_document_unit (test_pdf_adapter.PdfAdapterTests.test_enumeration_failure_keeps_prefix_and_unknown_total_with_document_unit) ... ok
test_existing_resource_collision_never_overwrites_human_bytes (test_pdf_adapter.PdfAdapterTests.test_existing_resource_collision_never_overwrites_human_bytes) ... ok
test_glyph_policy_overflow_does_not_publish_truncated_inventory (test_pdf_adapter.PdfAdapterTests.test_glyph_policy_overflow_does_not_publish_truncated_inventory) ... ok
test_glyphs_outside_cropbox_fail_page_instead_of_silently_clamping (test_pdf_adapter.PdfAdapterTests.test_glyphs_outside_cropbox_fail_page_instead_of_silently_clamping) ... ok
test_invalid_request_diagnostic_respects_small_explicit_byte_cap (test_pdf_adapter.PdfAdapterTests.test_invalid_request_diagnostic_respects_small_explicit_byte_cap) ... ok
test_overlapping_glyphs_are_preserved_and_require_review (test_pdf_adapter.PdfAdapterTests.test_overlapping_glyphs_are_preserved_and_require_review) ... ok
test_partial_page_failure_retains_confirmed_page_and_true_total (test_pdf_adapter.PdfAdapterTests.test_partial_page_failure_retains_confirmed_page_and_true_total) ... ok
test_pdf_page_policy_overflow_does_not_truncate_page_manifest (test_pdf_adapter.PdfAdapterTests.test_pdf_page_policy_overflow_does_not_truncate_page_manifest) ... ok
test_raw_pdf_policy_overflow_fails_before_any_resource_is_written (test_pdf_adapter.PdfAdapterTests.test_raw_pdf_policy_overflow_fails_before_any_resource_is_written) ... ok
test_real_cropbox_offset_matches_visible_origin (test_pdf_adapter.PdfAdapterTests.test_real_cropbox_offset_matches_visible_origin) ... ok
test_real_multiple_pages_preserves_every_physical_page (test_pdf_adapter.PdfAdapterTests.test_real_multiple_pages_preserves_every_physical_page) ... ok
test_real_single_page_matches_full_authoring_text_and_utf8_ranges (test_pdf_adapter.PdfAdapterTests.test_real_single_page_matches_full_authoring_text_and_utf8_ranges) ... ok
test_region_slice_is_exact_authoring_line_excluding_appended_lf (test_pdf_adapter.PdfAdapterTests.test_region_slice_is_exact_authoring_line_excluding_appended_lf) ... ok
test_rotations_and_userunit_keep_true_transforms_and_review_status (test_pdf_adapter.PdfAdapterTests.test_rotations_and_userunit_keep_true_transforms_and_review_status) ... ok
test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import (test_pdf_adapter.PdfAdapterTests.test_runtime_dependency_mismatch_is_fail_closed_before_any_pdf_import) ... ok
test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues (test_pdf_adapter.PdfAdapterTests.test_scan_and_unknown_blank_have_real_empty_artifacts_and_review_issues) ... ok
test_staggered_columns_retain_all_text_and_require_review (test_pdf_adapter.PdfAdapterTests.test_staggered_columns_retain_all_text_and_require_review) ... ok
test_table_formula_image_and_columns_remain_honestly_needs_review (test_pdf_adapter.PdfAdapterTests.test_table_formula_image_and_columns_remain_honestly_needs_review) ... ok
test_total_glyph_policy_overflow_never_publishes_incomplete_manifest (test_pdf_adapter.PdfAdapterTests.test_total_glyph_policy_overflow_never_publishes_incomplete_manifest) ... ok
test_tounicode_glyph_boundary_spaces_and_geometry_require_review (test_pdf_adapter.PdfAdapterTests.test_tounicode_glyph_boundary_spaces_and_geometry_require_review) ... ok
test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review (test_pdf_adapter.PdfAdapterTests.test_underflowed_display_geometry_cannot_be_reported_as_zero_area_review) ... ok
test_unknown_and_missing_fields_and_bad_enums_are_rejected (test_pdf_adapter.PdfAdapterTests.test_unknown_and_missing_fields_and_bad_enums_are_rejected) ... ok
test_unknown_format_preserves_complete_raw_without_inventing_pages (test_pdf_adapter.PdfAdapterTests.test_unknown_format_preserves_complete_raw_without_inventing_pages) ... ok
test_visual_reading_order_preserves_actual_drawing_inventory (test_pdf_adapter.PdfAdapterTests.test_visual_reading_order_preserves_actual_drawing_inventory) ... ok
test_whitespace_only_page_retains_nonempty_artifact_and_requires_review (test_pdf_adapter.PdfAdapterTests.test_whitespace_only_page_retains_nonempty_artifact_and_requires_review) ... ok
test_wrong_input_fingerprint_is_rejected_before_creating_resources (test_pdf_adapter.PdfAdapterTests.test_wrong_input_fingerprint_is_rejected_before_creating_resources) ... ok

----------------------------------------------------------------------
Ran 31 tests in 16.845s

OK
```

## Static and fingerprint verification

Python AST parse、两份 owned Python 文件的行尾空白检查、固定 raw SHA/长度核对均成功；该脚本不执行 PDF 解析或生成预期。实际输出如下。

```json
{
  "files": [
    {
      "path": "D:\\Noetome\\sidecars\\pdf_parser\\adapter.py",
      "byte_length": 31814,
      "sha256": "5e0a8c25c531085c5d04d55b094904fb83a79c3b3f2b3cd979e163c5bfc179b8",
      "trailing_whitespace_lines": []
    },
    {
      "path": "D:\\Noetome\\tests\\test_pdf_adapter.py",
      "byte_length": 43192,
      "sha256": "638e3b767e8148de37098fbf2eae27b22ae09aee18e4a9d3a78a1222486ae278",
      "trailing_whitespace_lines": []
    },
    {
      "path": "D:\\Noetome\\.artifacts\\b03-t02-adapter-report.md",
      "byte_length": 41921,
      "sha256": "90196dc4a08afbb17a39b4acf31517c5e78616812a31c4e19e76b6d87ec01778"
    },
    {
      "path": "D:\\Noetome\\.artifacts\\b03-t02-adapter-empty-wire-red-green.md",
      "byte_length": 11015,
      "sha256": "81685b5c96a63f7d07e254c03d0b947c51b3e04231c93e161b66d1738b8828d8"
    }
  ],
  "python_executable": "C:\\Users\\Lyinn\\.cache\\codex-runtimes\\codex-primary-runtime\\dependencies\\python\\python.exe",
  "python_version": "3.12.14",
  "pins": {
    "pdfplumber": "0.11.9",
    "pdfminer.six": "20251230",
    "pypdfium2": "5.14.0",
    "Pillow": "12.3.0",
    "charset-normalizer": "3.5.2",
    "cryptography": "50.0.2",
    "cffi": "2.1.1",
    "pycparser": "3.11"
  },
  "test_methods": 31,
  "fixed_fixture_count": 14,
  "known_physical_pages": 24,
  "source_utf8_bytes": 31814,
  "compressed_source_bytes": 8926,
  "compressed_test_wrapper_chars": 17949,
  "native_test_command_chars": 18059,
  "native_full_source_command_chars": 31974
}
```

仓库现有 tracked diff 的 whitespace 检查（untracked owned 文件另有上面的显式行尾/AST检查）：

```powershell
git diff --check
```

Exit code: `0`.

```text
warning: in the working copy of 'crates/noetome-cli/src/lib.rs', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of 'crates/noetome-cli/src/main.rs', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of 'crates/noetome-cli/src/sidecar_process.rs', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of 'crates/noetome-cli/src/sidecar_workspace.rs', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of 'crates/noetome-cli/tests/pdf_parse.rs', LF will be replaced by CRLF the next time Git touches it
```

当前本代理没有运行 Rust 测试、生产 compressed CLI、库安装、跨平台或性能验证；父代理负责后续真实 CLI 与全项目验收。没有 OCR、表格/公式/图像结构支持、生产 Vault、项目 Schema 或知识质量声明。旧 b03-t02-adapter-report.md 中首次 WinError206、LF range 集成失败和其他历史红灯，以及 b03-t02-adapter-empty-wire-red-green.md 的零范围 wire 失败仍完整保留。
