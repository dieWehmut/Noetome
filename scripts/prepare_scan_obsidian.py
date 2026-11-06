"""Prepare exact, independently verified scan output for a fresh Obsidian capture.

This tool reads already captured live results. It never runs the product CLI,
launches Obsidian, accesses credentials, or initiates a recognition request.
The prepared directory is not evidence that native rendering has passed.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import sys


REPO = Path(__file__).resolve().parents[1]
RUNTIME_PINS = {
    "cli": {"sha256": "91fe246b522f8261da8ad678f465021bf76660ee402ad35e3a6bb4c4c2762be7", "byte_length": 25876480},
    "obsidian_exe": {"sha256": "2156f15d12da8cc292fe0cc1884f32410fa187fede67beb6102cfcdc6442fbe2", "byte_length": 176761056},
    "obsidian_asar": {"sha256": "ec2f038fc2f6ba59764bd737461afa21b5b32e5726b4922fdf1e2fd7c3b5cb8c", "byte_length": 22737197},
}
NATIVE = REPO / "doc/validation/assets/B06-T03/native"
LAUNCHER = REPO / "doc/validation/assets/B06V-T01/native/launch_capture.py"
SCOPE = "Exact saved live scan output after independent readback; native capture not_run; scientific quality not_run"

# Written from the frozen author's one equation, never from a product plan or DOM.
AUTHOR_TREE = ["row", [
    ["msub", [["mi", "x"], ["mi", "i"]]], ["mo", "="], ["mo", "−"],
    ["mfrac", [["msub", [["mi", "a"], ["mi", "i"]]], ["msub", [["mi", "b"], ["mi", "i"]]]]],
]]
CLICK_ANCHOR = "      assert.equal(clicked.path, target);\n"
THEME_ANCHOR = "  assert.deepEqual(result.identity.communityPlugins, []);\n"
THEME_CHECK = """  assert(['light', 'dark'].includes(input.spec.theme), 'explicit bounded capture theme');
  await obs.waitForFunction(theme => document.body.classList.contains('theme-' + theme), input.spec.theme, { timeout: 15000 });
  result.identity.theme = await obs.evaluate(() => window.app.vault.getConfig('theme'));
  result.identity.bodyClass = await obs.evaluate(() => document.body.className);
  assert(result.identity.bodyClass.split(/\\s+/).includes('theme-' + input.spec.theme), 'actual native body theme matches selected capture');
"""
CLICK_CHECK = """      const expectedImage = input.spec.attachment_images[target];
      assert(expectedImage, 'clicked attachment has independently bound PNG dimensions');
      assert.equal(clicked.viewType, 'image', 'actual click opened the native image view');
      await obs.waitForFunction(({ target, expected }) => {
        const view = window.app.workspace.activeLeaf?.view;
        if (window.app.workspace.getActiveFile()?.path !== target || view?.getViewType() !== 'image') return false;
        const images = Array.from(view.containerEl.querySelectorAll('img'));
        return images.length === 1 && images[0].complete && images[0].naturalWidth === expected.width && images[0].naturalHeight === expected.height;
      }, { target, expected: expectedImage }, { timeout: 15000 });
      const readClickedImage = () => obs.evaluate(() => {
        const view = window.app.workspace.activeLeaf.view;
        const images = Array.from(view.containerEl.querySelectorAll('img'));
        if (images.length !== 1) throw new Error('active image view must contain exactly one PNG');
        const img = images[0], r = img.getBoundingClientRect(), style = getComputedStyle(img);
        return { activeFile: window.app.workspace.getActiveFile().path, viewType: view.getViewType(),
          src: img.currentSrc || img.src, complete: img.complete, naturalWidth: img.naturalWidth, naturalHeight: img.naturalHeight,
          rect: { x: r.x, y: r.y, width: r.width, height: r.height }, display: style.display, visibility: style.visibility };
      });
      const firstImage = await readClickedImage();
      await obs.waitForTimeout(200);
      const image = await readClickedImage();
      assert.equal(image.activeFile, target);
      assert.equal(image.viewType, 'image');
      assert(image.complete && image.naturalWidth === expectedImage.width && image.naturalHeight === expectedImage.height);
      assert.equal(firstImage.naturalWidth, image.naturalWidth);
      assert.equal(firstImage.naturalHeight, image.naturalHeight);
      assert(image.rect.width > 0 && image.rect.height > 0 && image.display !== 'none' && image.visibility !== 'hidden');
      assert(!/^https?:/i.test(image.src), 'clicked PNG remains a local selected attachment');
      assert(image.src.includes(target) || image.src.includes(encodeURI(target)), 'clicked native image URL names the selected PNG');
      clicked.image = { expected: expectedImage, actual: image, status: 'passed', method: 'active native image view after real click; fixed source dimensions' };
"""


def require(condition, message):
    if not condition:
        raise ValueError(message)


def guard(path, *, directory=False, single_link=True):
    """Check lexical ancestors before following any explicitly selected path."""
    path = Path(path).absolute()
    require(".." not in path.parts, "parent traversal is not a selected capture path")
    prefix = Path(path.anchor)
    for part in path.parts[1:]:
        prefix /= part
        info = prefix.lstat()
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, "st_file_attributes", 0) & 0x400,
                "selected path or ancestor is a link/reparse point")
        require(stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode), "selected path is not ordinary")
        if prefix != path:
            require(stat.S_ISDIR(info.st_mode), "selected path has a nondirectory ancestor")
    info = path.lstat()
    require(stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode) and (not single_link or info.st_nlink == 1),
            "selected root must be an ordinary directory" if directory else "selected file must have one ordinary link")
    return path


def identity(info):
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns, info.st_nlink)


def same_open_file(path_info, descriptor_info, *, windows=None):
    """Compare common identity without mixing Windows creation/change times.

    Native CPython 3.12 path stat reports creation time in st_ctime_ns, while
    descriptor fstat reports change time. Each full tuple must still remain
    unchanged within its own domain before/after the bounded read.
    """
    if not stat.S_ISREG(descriptor_info.st_mode):
        return False
    if windows is None:
        windows = os.name == "nt"
    if not windows:
        return identity(path_info) == identity(descriptor_info)
    selected, opened = identity(path_info), identity(descriptor_info)
    if selected[:4] + selected[5:] != opened[:4] + opened[5:]:
        return False
    if hasattr(path_info, "st_birthtime_ns") and hasattr(descriptor_info, "st_birthtime_ns"):
        return path_info.st_birthtime_ns == descriptor_info.st_birthtime_ns
    return True


def file_pin(path, maximum=512 * 1024 * 1024, *, single_link=True):
    path = guard(path, single_link=single_link)
    selected_info = path.stat()
    before = identity(selected_info)
    require(before[2] <= maximum, "selected file exceeds readback limit")
    digest, size = hashlib.sha256(), 0
    with path.open("rb") as stream:
        opened_info = os.fstat(stream.fileno())
        require(same_open_file(selected_info, opened_info), "selected file identity changed before reading")
        opened_before = identity(opened_info)
        while chunk := stream.read(min(1024 * 1024, maximum - size + 1)):
            digest.update(chunk)
            size += len(chunk)
            require(size <= maximum, "selected file exceeds readback limit while reading")
        require(identity(os.fstat(stream.fileno())) == opened_before, "selected file identity changed during reading")
    guard(path, single_link=single_link)
    require(identity(path.stat()) == before and size == before[2], "selected file changed while reading")
    return {"sha256": digest.hexdigest(), "byte_length": size}


def raw_pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def read(path, maximum=8 * 1024 * 1024):
    path = guard(path)
    require(path.stat().st_size <= maximum, "selected input exceeds readback limit")
    before = file_pin(path, maximum=maximum)
    pieces, size = [], 0
    with path.open("rb") as stream:
        state = identity(os.fstat(stream.fileno()))
        while chunk := stream.read(min(1024 * 1024, maximum - size + 1)):
            size += len(chunk)
            require(size <= maximum, "selected input exceeds readback limit while reading")
            pieces.append(chunk)
        require(identity(os.fstat(stream.fileno())) == state, "selected input changed while reading")
    raw = b"".join(pieces)
    require(raw_pin(raw) == before == file_pin(path, maximum=maximum), "selected input changed while reading")
    return raw


def inventory(path):
    path = guard(path, directory=True)
    entries = sorted(path.iterdir())
    # Reject unknown directories without inspecting their contents.
    for entry in entries:
        guard(entry)
    files = {entry.name: read(entry, maximum=12 * 1024 * 1024) for entry in entries}
    require([p.name for p in sorted(path.iterdir())] == list(files), "selected directory changed while reading")
    return files


def load_verifier():
    path = REPO / "scripts/verify_scan_formula.py"
    specification = importlib.util.spec_from_file_location("prepare_scan_independent_verifier", path)
    require(specification is not None and specification.loader is not None, "independent checker unavailable")
    module = importlib.util.module_from_spec(specification)
    specification.loader.exec_module(module)
    return module


def save(path, value):
    with Path(path).open("x", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")


def derived_capture(raw):
    """Keep the existing bounded collector and add only a scoped PNG click gate."""
    text = raw.decode("utf-8")
    require(text.count(CLICK_ANCHOR) == 1, "native capture insertion anchor must occur exactly once")
    require(text.count(THEME_ANCHOR) == 1, "native theme insertion anchor must occur exactly once")
    return text.replace(CLICK_ANCHOR, CLICK_ANCHOR + CLICK_CHECK).replace(THEME_ANCHOR, THEME_ANCHOR + THEME_CHECK).encode("utf-8")


def capture_spec(report, expected, checked, source_pins, expected_path, include_blocked, theme="light"):
    representation = checked["representation"]
    require(representation in (None, "native_math", "formula_image"), "unsupported native capture representation")
    require(report["decision"]["representation"] == representation, "checked representation differs from saved report")
    if representation is None:
        require(include_blocked, "blocked scan requires --include-blocked-report; no successful expression is prepared")
        note = {"file": "scan.md", "math_trees": [], "images": [],
                "required_texts": ["needs_review", "blocked", *checked["issue_codes"]]}
    else:
        require(report["status"] == "preview_ready" and report["preview_status"] == "saved", "scan note is not ready")
        require(expected["author_case"] == "clear" and expected["success_allowed"] is True,
                "only the frozen clear author may prepare a successful note")
        require(expected["expression"] == r"x_i = -\frac{a_i}{b_i}", "independent tree only covers the frozen author equation")
        recognized = report["recognition"]["recognized_tex"]
        require(isinstance(recognized, str) and not report["recognition"]["uncertainties"], "recognition is not complete")
        exact = [report["case"]["title"], recognized]
        for symbol in expected["symbols"]:
            exact.extend([symbol["symbol"], symbol["definition"], symbol["unit"] or "来源未给出单位"])
        for field in ("conditions", "ranges", "exceptions"):
            exact.extend(expected[field])
        attachments = ["original.png", "page.png", "response.bin", "recognized.tex"]
        if report["case"]["pdf"] is not None:
            attachments.append("original.pdf")
        require(all(name in source_pins for name in attachments), "successful scan attachment is absent")
        note = {"file": "扫描公式预览.md", "math_trees": [copy.deepcopy(AUTHOR_TREE)] if representation == "native_math" else [],
                "images": [{"file": "original.png", "width": expected["bounds"][2], "height": expected["bounds"][3]}]
                          if representation == "formula_image" else [],
                "required_texts": [*exact, "作者原始 TeX 未知"], "preformatted_exact": exact,
                "link_destinations": attachments, "click_attachments": ["original.png", "page.png"]}
    semantic_source = {key: copy.deepcopy(expected[key]) for key in ("symbols", "conditions", "ranges", "exceptions")}
    return {"case": checked["author_state"] + "-" + (representation or "blocked-report"),
            "scope": SCOPE, "theme": theme, "mathjax_version": "3.2.2", "require_mathjax_runtime": representation == "native_math",
            "notes": [note], "representation": representation, "issue_codes": checked["issue_codes"],
            "semantic_source": semantic_source, "expected_file": str(expected_path),
            "expected_fingerprint": raw_pin(read(expected_path)), "original_inventory": source_pins,
            "blocked_absent": ["扫描公式预览.md", "plan.json", "compiled.tex"] if representation is None else [],
            "attachment_images": {"original.png": {"width": expected["bounds"][2], "height": expected["bounds"][3]},
                                  "page.png": {"width": expected["canvas"][0], "height": expected["canvas"][1]}},
            "author_tree_provenance": "handwritten from frozen independent author x_i = -frac(a_i,b_i)",
            "native_capture_status": "not_run", "successful_expression": representation is not None}


def prepare(args):
    checker_pin = file_pin(REPO / "scripts/verify_scan_formula.py", maximum=8 * 1024 * 1024)
    verifier = load_verifier()
    require(checker_pin == file_pin(REPO / "scripts/verify_scan_formula.py", maximum=8 * 1024 * 1024),
            "independent checker changed while loading")
    source = guard(args.vault, directory=True)
    new_root = Path(args.new_root).absolute()
    require(".." not in new_root.parts, "parent traversal is not a new capture root")
    guard(new_root.parent, directory=True)
    require(not new_root.exists() and not new_root.is_symlink(), "capture preparation root already exists")
    paths = {name: guard(getattr(args, name)) for name in
             ("api_config", "profile", "expected", "invocation_receipt", "budget_binding", "cli", "obsidian_exe", "obsidian_asar")}
    case = guard(args.case, directory=True)
    initial = inventory(source)
    case_before = inventory(case)
    def selected_pins():
        return {name: file_pin(path, maximum=512 * 1024 * 1024 if name in RUNTIME_PINS else 8 * 1024 * 1024)
                for name, path in paths.items()}
    input_pins = selected_pins()
    for name, wanted in RUNTIME_PINS.items():
        require(input_pins[name] == wanted, "selected " + name + " differs from the accepted B06V-T02 runtime")
    checked = verifier.verify_output(
        case, paths["api_config"], paths["profile"], source, paths["expected"],
        invocation_receipt=verifier.strict_json(read(paths["invocation_receipt"])),
        budget_receipt=read(paths["budget_binding"]), budget_anchor=args.budget_binding_sha256)
    require(checked["status"] == "passed" and checked["ordinary_cli_binding"] == "passed"
            and checked["cumulative_budget_binding"] == "passed", "independent launch and budget readback did not pass")
    report = verifier.strict_json(initial["scan.json"])
    require(report["api"]["transport_mode"] == "live" and checked["real_model_calls"] == 1
            and report["network_calls"] == 1, "capture preparation requires one saved real live call; synthetic output is not live evidence")
    binding = verifier.strict_json(read(paths["budget_binding"]))
    budget_pins = {ref["file"]: file_pin(ref["file"]) for ref in
                   [*binding["receipts"].values(), *binding["copies"].values()]}
    first = verifier.strict_json(read(binding["receipts"]["before"]["file"]))
    require({key: first["selected_binary"][key] for key in ("sha256", "byte_length")} == RUNTIME_PINS["cli"],
            "captured ordinary invocation used a different CLI")
    expected = verifier.strict_json(read(paths["expected"]))
    pins = {name: raw_pin(raw) for name, raw in initial.items()}
    spec = capture_spec(report, expected, checked, pins, paths["expected"], args.include_blocked_report, getattr(args, "theme", "light"))
    native_paths = {name: NATIVE / name for name in ("launch_capture.py", "capture_formula.mjs", "run_suite.py")}
    native_paths["launch_capture.py"] = LAUNCHER
    native_pins = {name: file_pin(path) for name, path in native_paths.items()}
    derived_bytes = derived_capture(read(native_paths["capture_formula.mjs"]))
    launch_options = (args.native_python, args.powershell, args.playwright_module)
    require(all(launch_options) or not any(launch_options), "native-python, powershell and playwright-module must be supplied together")
    launch_paths = {name: guard(getattr(args, name), single_link=name != "powershell")
                    for name in ("native_python", "powershell", "playwright_module")} if all(launch_options) else {}
    def selected_launch_pins():
        return {name: file_pin(path, single_link=name != "powershell") for name, path in launch_paths.items()}
    launch_pins = selected_launch_pins()
    require(initial == inventory(source) and case_before == inventory(case), "source changed before preparation")
    require(input_pins == selected_pins(), "selected input/runtime changed before preparation")
    require(checker_pin == file_pin(REPO / "scripts/verify_scan_formula.py"), "independent checker changed during readback")
    new_root.mkdir()
    output = new_root / "selected-output"
    output.mkdir()
    for name, raw in initial.items():
        with (output / name).open("xb") as stream:
            stream.write(raw)
    require(inventory(output) == initial, "copied scan bytes differ from independently checked source")
    spec_path = new_root / "capture-spec.json"
    save(spec_path, spec)
    derived_path = new_root / "capture_scan_formula.mjs"
    with derived_path.open("xb") as stream:
        stream.write(derived_bytes)
    captures = new_root / "captures"
    captures.mkdir()
    command = None
    if all(launch_options):
        command = [sys.executable, str(native_paths["launch_capture.py"]), str(output), str(spec_path), str(captures / spec["case"]),
                   str(launch_paths["native_python"]), str(paths["obsidian_exe"]), str(launch_paths["powershell"]),
                   str(launch_paths["playwright_module"]), "--capture-script", str(derived_path)]
    require(initial == inventory(source) and case_before == inventory(case), "source changed after copying")
    require(input_pins == selected_pins(), "selected input/runtime changed after copying")
    require(budget_pins == {path: file_pin(path) for path in budget_pins}, "anchored public budget originals changed after copying")
    require(native_pins == {name: file_pin(path) for name, path in native_paths.items()}, "native capture scripts changed after copying")
    require(launch_pins == selected_launch_pins(), "selected launch runtime changed after copying")
    receipt = {"schema_version": 1, "status": "prepared", "scope": SCOPE,
               "independent_readback": checked, "checker": {"path": str(REPO / "scripts/verify_scan_formula.py"), **checker_pin},
               "source": str(source), "source_pins": pins, "copied_pins": {name: raw_pin(raw) for name, raw in inventory(output).items()},
               "selected_inputs": {name: {"path": str(paths[name]), **pin} for name, pin in input_pins.items()},
               "case": {"path": str(case), "pins": {name: raw_pin(raw) for name, raw in case_before.items()}},
               "budget_binding_sha256": args.budget_binding_sha256, "public_budget_originals": budget_pins,
               "native_scripts": {name: {"path": str(native_paths[name]), **pin} for name, pin in native_pins.items()},
               "derived_capture": {"path": str(derived_path), **raw_pin(derived_bytes),
                                   "source": native_pins["capture_formula.mjs"], "anchor": raw_pin(CLICK_ANCHOR.encode()),
                                   "insertion": raw_pin(CLICK_CHECK.encode()), "theme_anchor": raw_pin(THEME_ANCHOR.encode()),
                                   "theme_insertion": raw_pin(THEME_CHECK.encode()), "method": "two single exact anchor insertions; existing collector retained"},
               "launch_runtime": {name: {"path": str(launch_paths[name]), **pin} for name, pin in launch_pins.items()},
               "source_unchanged": True, "copied_bytes_identical": True, "native_capture_status": "not_run",
               "model_requests_in_preparation": 0, "cases": [{"case": spec["case"], "output": str(output),
                   "capture_spec": str(spec_path), "expected": str(paths["expected"]), "representation": spec["representation"],
                   "exit_code": 0 if spec["representation"] else 3, "launch_argv": command}]}
    save(new_root / "prepared.json", receipt)
    return receipt


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    for name in ("vault", "case", "api-config", "profile", "expected", "invocation-receipt", "budget-binding", "cli", "obsidian-exe", "obsidian-asar", "new-root"):
        result.add_argument("--" + name, type=Path, required=True)
    result.add_argument("--budget-binding-sha256", required=True, help="SHA captured independently with the public original receipts")
    result.add_argument("--include-blocked-report", action="store_true", help="prepare only scan.md for a verified needs_review output")
    result.add_argument("--theme", choices=("light", "dark"), default="light", help="fresh capture Vault theme; use distinct new roots for each theme")
    for name in ("native-python", "powershell", "playwright-module"):
        result.add_argument("--" + name, type=Path, help="optional explicit launch metadata; provide all three")
    return result


def main():
    try:
        args = parser().parse_args()
        receipt = prepare(args)
        print(json.dumps({"status": "prepared", "root": str(Path(args.new_root).absolute()),
                          "cases": receipt["cases"], "native_capture_status": "not_run"}, ensure_ascii=False, indent=2))
        return 0
    except (AssertionError, ValueError, OSError, KeyError, TypeError) as error:
        print("SCAN_CAPTURE_PREPARATION_REJECTED: " + str(error), file=sys.stderr)
        return 3


if __name__ == "__main__":
    raise SystemExit(main())
