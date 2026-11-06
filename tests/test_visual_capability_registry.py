"""Independent capability and source gates using unchanged author originals."""
import copy
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import visual_capability_registry as C
import verify_formula as F
import verify_image as I
import verify_process_flow as P
import verify_scan_formula as S


def profile(path):
    return F.strict_json(F.read(ROOT / "fixtures" / path))


def math_profiles():
    original = profile("formula-fidelity/profile-native.json")
    yield original
    yield dict(original, app_version="1.15.0", mathjax_version="4.1.3")
    # Both app/engine cross-pairs must stay unmeasured, even after each version
    # has its own eventual acceptance. The truth flags remain enabled here.
    for edit in ({"app": "OtherReader"}, {"app_version": "99.0"},
                 {"mathjax_version": "4.1.3"}, {"app_version": "1.15.0"},
                 {"verified": False}, {"native_math": False}):
        yield dict(original, **edit)


class IndependentCapabilityTests(unittest.TestCase):
    def test_separately_measured_current_targets_keep_exact_engine_profiles(self):
        math = profile("formula-fidelity/profile-native.json")
        math.update(app_version="1.15.0", mathjax_version="4.1.3")
        self.assertTrue(C.supports_native_math(math))
        image = profile("image-fidelity/profile-native.json")
        image["app_version"] = "1.15.0"
        self.assertTrue(C.supports_svg_target(image))
        flow = profile("process-flow-v1/profile-obsidian_1_6_7_flowchart.json")
        flow.update(app_version="1.15.0", mermaid_version="1.8.0",
                    profile_id="obsidian_1_15_0_flowchart")
        self.assertFalse(C.supports_flowchart(flow))

    def test_flag_integers_and_schema_boolean_cannot_grant_capabilities(self):
        math = profile("formula-fidelity/profile-native.json")
        for field in ("verified", "native_math", "schema_version"):
            changed = dict(math, **{field: True if field == "schema_version" else 1})
            self.assertFalse(C.supports_native_math(changed))
            with self.assertRaises(AssertionError):
                F.profile_schema(changed)
        image = profile("image-fidelity/profile-native.json")
        self.assertFalse(C.supports_svg_target(dict(image, verified=1)))
        flow = profile("process-flow-v1/profile-obsidian_1_6_7_flowchart.json")
        self.assertFalse(C.supports_flowchart(dict(flow, html_labels=1)))

    def test_native_formula_mixed_targets_fall_back_without_changing_tokens(self):
        case, semantics, source = F.selected_source(ROOT / "fixtures/formula-fidelity/native")
        case["_tex_bytes"] = source["original.tex"]
        for index, target in enumerate(math_profiles()):
            with self.subTest(target=target):
                result = F.classify(case, semantics, target)
                self.assertEqual(result[:3], ("native_math", [], []) if index < 2 else
                                 ("formula_image", [], ["FORMULA_NATIVE_UNAVAILABLE"]))
                self.assertEqual(result[4], F.tokens(semantics["expression"]))

    def test_formula_semantic_mismatch_is_blocked_before_target_fallback(self):
        case, semantics, source = F.selected_source(ROOT / "fixtures/formula-fidelity/native")
        case["_tex_bytes"] = source["original.tex"].replace(b"= -", b"= +", 1)
        self.assertNotEqual(case["_tex_bytes"], source["original.tex"])
        for target in math_profiles():
            with self.subTest(target=target):
                self.assertEqual(F.classify(case, semantics, target)[:4],
                                 (None, ["FORMULA_TEX_SEMANTIC_MISMATCH"], [], []))

    def test_scan_uses_same_pairs_and_never_fills_unknown_sign_or_subscript(self):
        case, semantics, _ = S.selected_source(ROOT / "fixtures/scan-formula/cases/clear")
        recognition = dict(schema_version=1, recognized_tex=semantics["expression"], uncertainties=[])
        for index, target in enumerate(math_profiles()):
            with self.subTest(target=target):
                decision, tokens = S.classify(case, semantics, target, recognition)
                self.assertEqual(decision["issue_codes"], [])
                self.assertEqual(decision["representation"], "native_math" if index < 2 else "formula_image")
                self.assertEqual(decision["fallback_reasons"], [] if index < 2 else ["SCAN_NATIVE_UNAVAILABLE"])
                self.assertEqual(tokens, S.math_tokens(semantics["expression"]))
                for name in ("unreadable_sign", "unreadable_subscript"):
                    damaged, unknown, _ = S.selected_source(ROOT / "fixtures/scan-formula/cases" / name)
                    blocked, _ = S.classify(damaged, unknown, target, recognition)
                    self.assertIsNone(blocked["representation"])
                    self.assertIn("SCAN_CRITICAL_SYMBOL_UNKNOWN", blocked["issue_codes"])
                    self.assertEqual(blocked["fallback_reasons"], [])

    def test_scan_changed_negative_and_subscript_remain_semantic_mismatch(self):
        case, semantics, _ = S.selected_source(ROOT / "fixtures/scan-formula/cases/clear")
        for changed in (semantics["expression"].replace("-", "+"),
                        semantics["expression"].replace("b_i", "b_j")):
            recognition = dict(schema_version=1, recognized_tex=changed, uncertainties=[])
            for target in math_profiles():
                with self.subTest(target=target, changed=changed):
                    decision, _ = S.classify(case, semantics, target, recognition)
                    self.assertIsNone(decision["representation"])
                    self.assertIn("SCAN_RECOGNITION_SEMANTIC_MISMATCH", decision["issue_codes"])
                    self.assertEqual(decision["fallback_reasons"], [])

    def test_svg_unknown_target_and_flag_keep_separate_diagnostics(self):
        case, semantics, _ = I.selected_source(ROOT / "fixtures/image-fidelity/signed_arrows")
        original = profile("image-fidelity/profile-native.json")
        for target in (dict(original, app="OtherReader"), dict(original, app_version="99.0"),
                       dict(original, verified=False)):
            self.assertEqual(I.classify(case, semantics, target),
                             ("original_image", [], ["IMAGE_TARGET_UNVERIFIED"]))
            self.assertEqual(I.classify(case, semantics, dict(target, svg_supported=False)),
                             ("original_image", [], ["IMAGE_TARGET_UNVERIFIED", "IMAGE_SVG_UNAVAILABLE"]))
            unknown = copy.deepcopy(semantics)
            unknown["relations"][0]["effect"] = "unknown"
            self.assertEqual(I.classify(case, unknown, target), (None, ["IMAGE_UNKNOWN_RELATION"], []))

    def test_flow_mixed_engine_and_identity_cannot_bypass_unknown_direction(self):
        original = profile("process-flow-v1/profile-obsidian_1_6_7_flowchart.json")
        linear = P.parse_source(F.read(ROOT / "fixtures/process-flow-v1/linear.txt"))[0]
        unknown = P.parse_source(F.read(ROOT / "fixtures/process-flow-v1/direction_unknown.txt"))[0]
        self.assertEqual(P.classify(linear, original)[:2], ("flowchart", []))
        for edit in ({"app": "OtherReader"}, {"app_version": "99.0"},
                     {"app_version": "1.15.0"}, {"mermaid_version": "1.8.0"},
                     {"profile_id": "obsidian_1_15_0_flowchart"},
                     {"flowchart_support": "unverified"}, {"html_labels": False}):
            target = dict(original, **edit)
            with self.subTest(target=target):
                self.assertEqual(P.classify(linear, target)[:2], ("ordered_list", ["FLOW_TARGET_UNVERIFIED"]))
                self.assertEqual(P.classify(unknown, target), (None, ["FLOW_DIRECTION_UNKNOWN"], []))


if __name__ == "__main__":
    unittest.main()
