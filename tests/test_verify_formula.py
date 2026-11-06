"""Independent formula source, syntax and ordinary CLI read-back regressions."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("verify_formula", ROOT / "scripts/verify_formula.py")
V = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(V)
FIXTURE = ROOT / "fixtures/formula-fidelity"


class FormulaAuthorTests(unittest.TestCase):
    def source(self, name="native"):
        case, semantics, source = V.selected_source(FIXTURE / name)
        if case["tex"]:
            case["_tex_bytes"] = source["original.tex"]
        return case, semantics, source

    def profile(self, name="native"):
        return V.strict_json(V.read(FIXTURE / ("profile-" + name + ".json")))

    def test_fifteen_frozen_expectations_are_independent_of_compiler_outputs(self):
        self.assertEqual(len(V.verify_fixture()["cases"]), 15)

    def test_modified_expected_cannot_become_new_author_truth(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "authors";shutil.copytree(FIXTURE, root)
            p = root / "expected-native.json";e = json.loads(p.read_bytes());e["tokens"][7] = "+";V.save(p,e)
            with self.assertRaises(AssertionError):V.verify_fixture(root)

    def test_subscripts_superscripts_negation_and_grouping_are_not_collapsed(self):
        original = r"C_i(t) = -\frac{a}{b} t^{2}"
        base = V.tokens(original)
        for changed in (original.replace("C_i", "C_j"), original.replace("= -", "= +"),
                        original.replace("t^{2}", "t^{3}"), original.replace("{a}{b}", "{b}{a}"),
                        original.replace("C_i", "C_{i}")):
            with self.subTest(changed=changed):self.assertNotEqual(base,V.tokens(changed))

    def test_only_declared_dfrac_and_ascii_whitespace_normalize(self):
        self.assertEqual(V.tokens(r"a=\dfrac{b}{c}"),V.tokens(" a = \\frac{ b }{c}\r\n"))
        for changed in ("a−b", "a\u00a0b", r"a=\tfrac{b}{c}", "α = b", "a%ignored\nb"):
            with self.subTest(changed=changed),self.assertRaises(AssertionError):V.tokens(changed)

    def test_complete_bounded_grammar_rejects_renderable_or_malformed_inputs(self):
        inputs = ["", "a=", "=a", "a+", "a++b", "a+-b", "a--b", "a*--b", "a/+-b", "a_12", "a_(i)", r"a_\frac{b}{c}", r"a^\exp(b)", r"\exp[b]", "a__i", "a^2^3", "a_{i", "a{}", r"a=\frac{}{b}",
                  r"a=\frac{b}", r"\exp", r"\exp()", r"a=\href{https://example.org}{b}",
                  r"a=\require{html}", r"a=\def\x{b}", "a\x00b", "a.1", "1.", "a\\"]
        for formula in inputs:
            with self.subTest(formula=formula),self.assertRaises(AssertionError):V.tokens(formula)

    def test_exponent_function_and_greek_symbols_have_explicit_structure(self):
        self.assertEqual(V.tokens(r"C_i(t)=C_0\exp(-k t)")[10:], ["\\exp","(","-","k","t",")"])
        self.assertEqual(V.tokens(r"\alpha_i \le \frac{a}{b}"),["\\alpha","_","i","\\le","\\frac","{","a","}","{","b","}"])
        self.assertEqual(V.tokens(r"\exp{a}"),["\\exp","{","a","}"])
        self.assertEqual(V.tokens(r"\exp({a})"),["\\exp","(","{","a","}",")"])
        self.assertEqual(V.tokens("a*-b"),["a","*","-","b"])
        self.assertEqual(V.tokens("a/-b"),["a","/","-","b"])

    def test_nesting_and_token_budget_are_bounded(self):
        for expression in ("("*80+"x"+")"*80,"x "*5000):
            with self.assertRaises(AssertionError):V.tokens(expression)
        for depth in (63,64):
            self.assertEqual(len(V.tokens("("*depth+"x"+")"*depth)),depth*2+1)
        with self.assertRaises(AssertionError):V.tokens("("*65+"x"+")"*65)

    def test_native_requires_measured_target_and_declared_capability(self):
        case,semantics,_=self.source()
        for field,value in (("verified",False),("native_math",False),("app","UnknownApp"),("app_version","99.0"),("mathjax_version","3.2.1")):
            profile=self.profile();profile[field]=value
            with self.subTest(field=field):
                self.assertEqual(V.classify(case,semantics,profile)[:3],("formula_image",[],["FORMULA_NATIVE_UNAVAILABLE"]))

    def test_unknown_symbol_stays_blocked_even_if_original_image_is_clear(self):
        case,semantics,_=self.source("unknown_symbol")
        for name in ("native","no-native"):
            self.assertEqual(V.classify(case,semantics,self.profile(name))[:4],(None,["FORMULA_UNKNOWN_SYMBOL"],[],[]))

    def test_symbol_names_and_duplicates_must_match_actual_formula(self):
        case,semantics,_=self.source()
        for change in (lambda s:s["symbols"].pop(),lambda s:s["symbols"].append(copy.deepcopy(s["symbols"][0])),
                       lambda s:s["symbols"][0].update(symbol="D")):
            value=copy.deepcopy(semantics);change(value)
            self.assertEqual(V.classify(case,value,self.profile())[:2],(None,["FORMULA_SYMBOLS_MISMATCH"]))

    def test_tex_semantics_mismatch_cannot_be_hidden_by_image_fallback(self):
        case,semantics,_=self.source("expression_mismatch")
        for profile in (self.profile(),self.profile("no-native")):
            self.assertEqual(V.classify(case,semantics,profile)[:2],(None,["FORMULA_TEX_SEMANTIC_MISMATCH"]))

    def test_selected_region_is_never_claimed_as_original_tex(self):
        case,semantics,source=self.source("region_only")
        self.assertIsNone(case["tex"]);self.assertNotIn("original.tex",source)
        self.assertEqual(V.classify(case,semantics,self.profile())[:3],("formula_image",[],["FORMULA_REGION_ONLY"]))

    def test_unreadable_unknown_macro_and_curves_cannot_pass_as_fallback(self):
        for name,code in (("unreadable_region","FORMULA_REGION_UNREADABLE"),("unknown_macro","FORMULA_UNSUPPORTED_SYNTAX"),("curve_no_data","FORMULA_CURVE_NO_DATA")):
            case,semantics,_=self.source(name)
            for profile in (self.profile(),self.profile("no-native")):
                self.assertEqual(V.classify(case,semantics,profile)[:4],(None,[code],[],[]))
        case,semantics,_=self.source("curve_no_data");case["data_available"]=True
        self.assertEqual(V.classify(case,semantics,self.profile())[:2],(None,["FORMULA_CURVE_UNSUPPORTED"]))

    def test_compatibility_uses_actual_original_macro_not_metadata_spelling(self):
        case,semantics,_=self.source("compatibility")
        semantics["expression"]=semantics["expression"].replace("\\dfrac","\\frac")
        self.assertEqual(V.classify(case,semantics,self.profile())[3],["FORMULA_DFRAC_COMPATIBILITY"])
        self.assertEqual(V.classify(case,semantics,self.profile("no-dfrac"))[:3],("formula_image",[],["FORMULA_DFRAC_UNAVAILABLE"]))

    def test_raw_tex_crlf_and_region_bytes_are_preserved(self):
        _,_,source=self.source()
        self.assertTrue(source["original.tex"].endswith(b"\r\n"))
        self.assertEqual(V.png_info(source["original.png"]),(800,240))

    def test_png_crc_and_decompressed_scanlines_are_checked(self):
        _,_,source=self.source();raw=source["original.png"]
        for damaged in (raw[:40],raw+b"tail",raw[:50]+bytes([raw[50]^1])+raw[51:]):
            with self.assertRaises(AssertionError):V.png_info(damaged)

    def test_png_decompression_is_bounded_before_allocation(self):
        def chunk(name,payload):
            return len(payload).to_bytes(4,"big")+name+payload+V.zlib.crc32(name+payload).to_bytes(4,"big")
        header=(1).to_bytes(4,"big")*2+b"\x08\x02\x00\x00\x00"
        bomb=b"\x89PNG\r\n\x1a\n"+chunk(b"IHDR",header)+chunk(b"IDAT",V.zlib.compress(b"\x00"*65536))+chunk(b"IEND",b"")
        constructor=V.zlib.decompressobj;observed=[]
        class TrackingDecoder:
            def __init__(self):self.inner=constructor()
            def decompress(self,data,max_length=0):
                observed.append(max_length)
                self.assert_bounded(max_length)
                return self.inner.decompress(data,max_length)
            @staticmethod
            def assert_bounded(max_length):assert max_length==5,"1x1 RGB scanline must allocate at most expected4+1 bytes"
            def __getattr__(self,name):return getattr(self.inner,name)
        with mock.patch.object(V.zlib,"decompressobj",TrackingDecoder),self.assertRaises(AssertionError):V.png_info(bomb)
        self.assertEqual(observed,[5])

    def test_source_boundaries_collision_missing_nullable_and_ancestor_links(self):
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory)
            for label,operation in (("bounds",lambda c:c["region"].update(bounds=[900,0,800,240])),
                                    ("dimension",lambda c:c["region"].update(bounds=[0,0,700,240])),
                                    ("traversal",lambda c:c["tex"].update(file="../outside.tex")),
                                    ("missing",lambda c:c.pop("region")),
                                    ("bool",lambda c:c.update(schema_version=True))):
                source=base/label;shutil.copytree(FIXTURE/"native",source)
                p=source/"case.json";case=json.loads(p.read_bytes());operation(case);V.save(p,case)
                with self.subTest(label=label),self.assertRaises((AssertionError,KeyError)):V.selected_source(source)

    def test_literal_fence_reader_keeps_markup_inside_source_text(self):
        raw=b"# preview\n\n## context\n\n````text\n```\n## hidden\n![[outside.png]]\n````\n"
        nodes=V.markdown_nodes(raw)
        self.assertEqual([n["kind"] for n in nodes],["heading","heading","literal"])
        self.assertEqual(nodes[-1]["value"],"```\n## hidden\n![[outside.png]]")


class FormulaCliReaderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        selected=os.environ.get("NOETOME_TEST_BINARY")
        if not selected:raise unittest.SkipTest("ordinary CLI is selected explicitly by NOETOME_TEST_BINARY; separate audit exercises it")
        cls.cli=Path(selected)
        if not cls.cli.is_file():raise RuntimeError("selected ordinary CLI does not exist")
        cls.temp=tempfile.TemporaryDirectory(prefix="noetome-formula-reader-")
        cls.root=Path(cls.temp.name)
        cls.vault=cls.root/"baseline"
        result=subprocess.run([str(cls.cli),"knowledge","formula",str(FIXTURE/"native"),str(FIXTURE/"profile-native.json"),str(cls.vault)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
        assert result.returncode==0,result.stderr.decode(errors="replace")
        V.verify_output(FIXTURE/"native",FIXTURE/"profile-native.json",cls.vault,FIXTURE/"expected-native.json")

    @classmethod
    def tearDownClass(cls):cls.temp.cleanup()

    def copy_vault(self):
        path=self.root/(self._testMethodName+"-"+str(len(list(self.root.iterdir()))));shutil.copytree(self.vault,path);return path

    def checked(self,path):return V.verify_output(FIXTURE/"native",FIXTURE/"profile-native.json",path,FIXTURE/"expected-native.json")

    def test_ordinary_cli_readback_has_preserved_sources_and_semantic_expression(self):
        self.assertEqual(self.checked(self.vault)["representation"],"native_math")

    def test_explicit_target_original_recomputes_fallback_and_preserves_author_freeze(self):
        author_before=V.pins(V.inventory(FIXTURE))
        profile=self.root/"formula-target.json";value=V.strict_json(V.read(FIXTURE/"profile-native.json"));value["app_version"]="99.0";V.save(profile,value)
        path=self.root/"formula-explicit-target"
        result=subprocess.run([str(self.cli),"knowledge","formula",str(FIXTURE/"native"),str(profile),str(path)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
        self.assertEqual(result.returncode,0,result.stderr.decode(errors="replace"))
        checked=V.verify_output(FIXTURE/"native",FIXTURE/"profile-native.json",path,FIXTURE/"expected-native.json",target_profile=profile)
        self.assertEqual(checked["representation"],"formula_image")
        audit=subprocess.run([V.sys.executable,str(ROOT/"scripts/verify_formula.py"),"check",str(FIXTURE/"native"),str(FIXTURE/"profile-native.json"),str(path),str(FIXTURE/"expected-native.json"),"--target-profile",str(profile)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
        self.assertEqual(audit.returncode,0,audit.stderr.decode(errors="replace"));self.assertEqual(V.strict_json(audit.stdout),checked)
        with self.assertRaises(AssertionError):self.checked(path)
        with self.assertRaisesRegex(AssertionError,"frozen selected capability profile"):
            V.verify_output(FIXTURE/"native",profile,path,FIXTURE/"expected-native.json",target_profile=profile)
        self.assertEqual(V.pins(V.inventory(FIXTURE)),author_before)

    def test_explicit_target_cannot_turn_unknown_source_symbol_into_fallback_success(self):
        profile=self.root/"formula-blocked-target.json";value=V.strict_json(V.read(FIXTURE/"profile-native.json"));value["app_version"]="99.0";V.save(profile,value)
        path=self.root/"formula-unknown-explicit-target"
        result=subprocess.run([str(self.cli),"knowledge","formula",str(FIXTURE/"unknown_symbol"),str(profile),str(path)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
        self.assertEqual(result.returncode,3)
        checked=V.verify_output(FIXTURE/"unknown_symbol",FIXTURE/"profile-native.json",path,FIXTURE/"expected-unknown_symbol.json",target_profile=profile)
        self.assertIsNone(checked["representation"]);self.assertEqual(checked["issue_codes"],["FORMULA_UNKNOWN_SYMBOL"])

    def test_synchronized_unknown_target_native_claim_rejected_with_explicit_target(self):
        path=self.copy_vault();profile=self.root/"formula-forged-target.json";value=V.strict_json(V.read(FIXTURE/"profile-native.json"));value["app_version"]="99.0";V.save(profile,value)
        (path/"profile.json").write_bytes(profile.read_bytes());report=V.strict_json(V.read(path/"formula.json"));report["profile"]=value;report["inputs"]["profile.json"]=V.fingerprint(profile.read_bytes());V.save(path/"formula.json",report);V.resynchronize_result(path)
        with self.assertRaisesRegex(AssertionError,"source-grounded expression outcome"):
            V.verify_output(FIXTURE/"native",FIXTURE/"profile-native.json",path,FIXTURE/"expected-native.json",target_profile=profile)

    def test_explicit_measured_target_still_rejects_synced_negative_sign_loss(self):
        path=self.copy_vault();note=path/"公式预览.md";note.write_bytes(note.read_bytes().replace(b"= -",b"= +",1));V.resynchronize_result(path)
        with self.assertRaises(AssertionError):
            V.verify_output(FIXTURE/"native",FIXTURE/"profile-native.json",path,FIXTURE/"expected-native.json",target_profile=FIXTURE/"profile-native.json")

    def test_synced_hashes_do_not_confirm_negative_sign_or_subscript(self):
        for old,new in ((b"= -",b"= +"),(b"C_i",b"C_j"),(b"t^{2}",b"t^{3}")):
            path=self.copy_vault();p=path/"公式预览.md";p.write_bytes(p.read_bytes().replace(old,new,1));V.resynchronize_result(path)
            with self.subTest(old=old),self.assertRaises(AssertionError):self.checked(path)

    def test_mapping_byte_ranges_are_not_a_substitute_for_visible_context(self):
        path=self.copy_vault();p=path/"公式预览.md";p.write_bytes(p.read_bytes().replace(b"a > 0 and b > 0",b"a > 0",1));V.resynchronize_result(path)
        with self.assertRaises(AssertionError):self.checked(path)

    def test_fake_success_and_modified_original_are_rejected_independently(self):
        path=self.copy_vault();p=path/"original.tex";p.write_bytes(p.read_bytes().replace(b"= -",b"= +"));V.resynchronize_result(path)
        with self.assertRaises(AssertionError):self.checked(path)

    def test_false_individual_check_is_rejected_when_final_blocked_status_remains(self):
        path=self.root/"unknown-symbol-original"
        result=subprocess.run([str(self.cli),"knowledge","formula",str(FIXTURE/"unknown_symbol"),str(FIXTURE/"profile-native.json"),str(path)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=30)
        self.assertEqual(result.returncode,3)
        original=V.strict_json(V.read(path/"formula.json"));self.assertEqual(original["status"],"needs_review")
        original["checks"]["symbol_definitions"]="passed";V.save(path/"formula.json",original);V.resynchronize_result(path)
        with self.assertRaises(AssertionError):V.verify_output(FIXTURE/"unknown_symbol",FIXTURE/"profile-native.json",path,FIXTURE/"expected-unknown_symbol.json")

    def test_symbol_labels_must_remain_adjacent_to_their_meaningful_values(self):
        path=self.copy_vault()
        V.misassociate_symbol_labels(path);V.resynchronize_result(path)
        with self.assertRaises(AssertionError):self.checked(path)

    def test_false_visible_report_body_is_rejected_even_when_payload_and_summary_agree(self):
        path=self.copy_vault();p=path/"formula.md"
        p.write_bytes(p.read_bytes().replace("网络与付费调用：0。".encode(),"科学质量：passed；发布：true；网络与付费调用：2。".encode(),1))
        V.resynchronize_result(path)
        with self.assertRaises(AssertionError):self.checked(path)


if __name__=="__main__":unittest.main()
