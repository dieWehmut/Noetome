"""Exact source scope check; does not run an application or alter originals."""
import difflib
import hashlib
import importlib.util
import json
from pathlib import Path
import re

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[5]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def pin(raw):
    return {'sha256': hashlib.sha256(raw).hexdigest(), 'byte_length': len(raw)}


old_path = REPO / '.artifacts/b06v-t03-minimum-capture-20261106-g/repo/scripts/prepare_visual_compatibility.py'
new_path = REPO / 'scripts/prepare_visual_compatibility.py'
assert old_path.read_bytes() == (HERE / 'captured-b7-preparation.py').read_bytes()
assert new_path.read_bytes() == (HERE / 'final-b22-preparation.py').read_bytes()
assert pin(new_path.read_bytes())['sha256'] == 'b22fc9c0f91812fac99dd0087291c67cc98249f00f4da068271bdd2c0f551a49'
old, new = load('captured_b7_scope', old_path), load('final_b22_scope', new_path)
shared = {key: getattr(old, key) == getattr(new, key) for key in ('RUNTIME_HELPERS', 'FORMAT_TEXT', 'FORMAT_COLLECTOR', 'PELE_READ', 'PELE_VALIDATE', 'NATIVE_SCREENSHOT', 'AUXILIARY_SCREENSHOT')}
assert all(shared.values())
raw = old.P.read(old.COLLECTORS['image'])
assert raw == new.P.read(new.COLLECTORS['image'])
a, b = old.derived_collector(raw, 'image'), new.derived_collector(raw, 'image')
assert new.SCREENSHOT_PIXELS.startswith(old.SCREENSHOT_PIXELS)
old_gate = re.search(r"^    if\(bounds\) assert\(e\.bbox.+nominal font-metric pixel'\);$", a.decode(), re.MULTILINE).group(0)
new_gate = '    const measuredBounds=e.actualInk?.bbox??e.bbox;\n' + old_gate.replace('e.bbox', 'measuredBounds')
assert b.decode().count(new.INTRINSIC_INK) == b.decode().count(new_gate) == 1
normalized = b.decode().replace(new.SCREENSHOT_PIXELS, old.SCREENSHOT_PIXELS, 1).replace(new.INTRINSIC_INK, '', 1).replace(new_gate, old_gate, 1).encode()
assert normalized == a
assert 'if(native.pixelMethod && semantic.source)' in new.INTRINSIC_INK


def legacy(text):
    start = text.index('      }else{\n', text.index("if(input.spec.runtime.app_version==='1.15.0')")) + len('      }else{\n')
    end = text.index('      }\n      // Electron CDP rejects Target.createTarget.', start)
    return text[start:end]


assert legacy(a.decode()) == legacy(b.decode())
assert 'pixelMethod' not in legacy(a.decode()) and re.search(r'\.actualInk\s*=', raw.decode()) is None
others = {kind: old.derived_collector(old.P.read(old.COLLECTORS[kind]), kind) == new.derived_collector(new.P.read(new.COLLECTORS[kind]), kind) for kind in ('table', 'flow', 'formula', 'scan')}
assert all(others.values())
observations = {}
for theme in ('light', 'dark'):
    path = Path('/mnt/d/Noetome/.artifacts/b06v-t03-minimum-20261106-g/captures/image-signed-arrows-' + theme + '/result.json')
    data = json.loads(path.read_bytes())
    native = data['notes'][0]['svgEvidence']['native']
    assert data['status'] == 'passed' and 'pixelMethod' not in native
    assert all('actualInk' not in element for element in data['notes'][0]['svgEvidence']['geometry']['renderedElements'])
    observations[theme] = {'result_pin': pin(path.read_bytes()), 'actual_legacy_native_has_no_pixelMethod': True, 'actual_geometry_elements_have_no_actualInk': True}
(HERE / 'raw-source.diff').write_text(''.join(difflib.unified_diff(old_path.read_text().splitlines(True), new_path.read_text().splitlines(True), fromfile='captured-b7', tofile='final-b22')))
result = {'status': 'passed', 'captured_tool': pin(old_path.read_bytes()), 'final_tool': pin(new_path.read_bytes()),
          'shared_helpers_byte_identical': shared, 'non_image_derived_byte_identical': others,
          'original_image_collector_byte_identical': True, 'minimum_legacy_native_branch_byte_identical': True,
          'minimum_branch_has_no_pixelMethod': True, 'original_geometry_has_no_actualInk': True,
          'new_intrinsic_ink_explicit_guard': 'native.pixelMethod && semantic.source',
          'changed_shape_gate': 'actualInk bbox if present; exact original bbox retained for observed minimum geometry without actualInk',
          'image_collector_captured': pin(a), 'image_collector_final': pin(b),
          'only_current_ink_additions_removed_recovers_exact_captured_collector': normalized == a,
          'actual_minimum_observations': observations,
          'scope': 'Final minimum captures remain actual b7-derived original evidence. Final b22 current-only ink additions have no observed legacy-runtime effects; no claim the whole b22 image script was executed on minimum.'}
(HERE.parent / 'final-collector-scope.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
