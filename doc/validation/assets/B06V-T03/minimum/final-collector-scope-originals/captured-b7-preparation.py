"""Prepare one frozen visual case for one independently selected app runtime.

This utility only reads, verifies and copies public test inputs. It does not
launch a process, install an app, access credentials or make a model request.
The derived collector retains the old semantic gates; preparation is not a
successful native capture.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import re
import struct
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import prepare_scan_obsidian as P
from verify_extraction_knowledge import strict_json

REPO = Path(__file__).resolve().parents[1]
COLLECTORS = {
    "table": REPO / "doc/validation/assets/B06-T01/obsidian/capture_obsidian.mjs",
    "flow": REPO / "doc/validation/assets/B06-T02/obsidian/capture_obsidian.mjs",
    "formula": REPO / "doc/validation/assets/B06-T03/native/capture_formula.mjs",
    "scan": REPO / "doc/validation/assets/B06-T03/native/capture_formula.mjs",
    "image": REPO / "doc/validation/assets/B06V-T01/native/capture_image.mjs",
}
CHECKERS = {
    "table": "verify_knowledge_presentation.py", "flow": "verify_process_flow.py",
    "formula": "verify_formula.py", "image": "verify_image.py", "scan": "verify_scan_formula.py",
}
MATRIX_CASES = {
    (13, 12, 1): {"table-escaping-names", "flow-branch", "flow-multiline-labels", "formula-native",
                  "formula-native-relational", "formula-image-fallback", "image-signed-arrows", "image-curve-no-data",
                  "scan-clear", "scan-fallback", "scan-sign", "scan-subscript", "hand-authored-format-probe"},
    (3, 3, 0): {"flow-complex-labels", "flow-promotes", "flow-inhibits"},
}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def pin(raw):
    return {"sha256": hashlib.sha256(raw).hexdigest(), "byte_length": len(raw)}


def relative(name):
    require(isinstance(name, str) and name and "\\" not in name and "\x00" not in name,
            "selected fixture path must be relative")
    value = PurePosixPath(name)
    require(not value.is_absolute() and all(p not in ("", ".", "..") for p in name.split("/")),
            "selected fixture path escapes its root")
    return value


def under(root, name):
    return Path(root).joinpath(*relative(name).parts)


def files(root):
    """Enumerate only the explicit public test root, guarding before descent."""
    root = P.guard(root, directory=True)
    result = {}
    for folder, directories, names in os.walk(root, followlinks=False):
        for name in directories:
            P.guard(Path(folder) / name, directory=True)
        for name in names:
            path = P.guard(Path(folder) / name)
            result[path.relative_to(root).as_posix()] = P.read(path, maximum=16 * 1024 * 1024)
    return result


def fixture(root, anchor):
    raw = P.read(Path(root) / "frozen-sha256.json")
    require(pin(raw)["sha256"] == anchor, "independent fixture freeze SHA mismatch")
    manifest = strict_json(raw)
    require(isinstance(manifest.get("files"), dict), "frozen fixture files are required")
    actual = files(root)
    for name, wanted in manifest["files"].items():
        relative(name)
        require(name in actual, "frozen author or output is missing: " + name)
        require(pin(actual[name]) == wanted, "frozen author or output bytes changed: " + name)
    # The parent freeze deliberately omits each embedded original author's own
    # frozen-sha256.json. Their exact bytes are independently referenced by its
    # pinned source-pins.json; do not accept arbitrary unlisted fixture files.
    inventory = set(manifest["files"]) | {"frozen-sha256.json"}
    source_pins = strict_json(actual["source-pins.json"])
    for ref in source_pins["original_sources"].values():
        name = ref["captured_file"]
        if name.endswith("/frozen-sha256.json"):
            relative(name)
            require(name.startswith("source-originals/fixtures/"), "embedded author freeze must remain in its original root")
            require(name in actual and pin(actual[name]) == {k: ref[k] for k in ("sha256", "byte_length")},
                    "embedded original author freeze bytes changed: " + name)
            inventory.add(name)
    require(set(actual) == inventory, "frozen fixture inventory mismatch")
    cases = strict_json(actual["cases.json"])
    shape = (cases["case_count"], cases["product_case_count"], cases["hand_authored_format_probe_count"])
    require(shape in MATRIX_CASES and len(cases["cases"]) == shape[0]
            and {c["case_id"] for c in cases["cases"]} == MATRIX_CASES[shape],
            "exact frozen 13-case matrix or independent 3-flow extension required")
    return cases, actual


def member(archive, name):
    relative(name)
    require(len(archive) >= 16, "truncated ASAR header")
    header = struct.unpack("<IIII", archive[:16])
    require(0 < header[3] <= 8 * 1024 * 1024 and 16 + header[3] <= len(archive), "bounded ASAR index")
    node = strict_json(archive[16:16 + header[3]])
    for part in PurePosixPath(name).parts:
        node = node["files"][part]
    require(not node.get("link") and not node.get("unpacked"), "selected ASAR member is embedded ordinary data")
    require(type(node["size"]) is int and 0 <= node["size"] <= 32 * 1024 * 1024, "bounded selected ASAR member")
    start = 8 + header[1] + int(node["offset"])
    require(0 <= start <= start + node["size"] <= len(archive), "ASAR member out of bounds")
    raw = archive[start:start + node["size"]]
    if "integrity" in node:
        require(node["integrity"]["algorithm"].upper() == "SHA256"
                and pin(raw)["sha256"] == node["integrity"]["hash"], "ASAR member integrity mismatch")
    return raw


def windows_path(path):
    text = str(path)
    if PureWindowsPath(text).is_absolute():
        return text
    match = re.fullmatch(r"/mnt/([a-z])/(.*)", text)
    require(match is not None, "capture runtime requires an explicit Windows or WSL-mounted path")
    return str(PureWindowsPath(match[1].upper() + ":/").joinpath(*PurePosixPath(match[2]).parts))


def runtime(path, anchor):
    raw = P.read(path)
    require(pin(raw)["sha256"] == anchor, "independent runtime record SHA mismatch")
    value = strict_json(raw)
    require(set(value) >= {"app_version", "executable", "app_asar", "obsidian_asar", "mathjax", "mermaid"},
            "normalized independently selected runtime record required")
    require(value["app_version"] in ("1.6.7", "1.15.0"), "runtime is outside this selected minimum/stable matrix")
    result = copy.deepcopy(value)
    selected = {}
    archives = {}
    for key, suffix in (("executable", "Obsidian.exe"), ("app_asar", "app.asar"), ("obsidian_asar", "obsidian.asar")):
        ref = value[key]
        require(set(ref) in ({"path", "sha256", "byte_length"}, {"path", "windows_path", "sha256", "byte_length"}),
                "strict selected runtime file reference")
        actual_path = Path(ref["path"])
        require(actual_path.name == suffix, "explicit selected runtime filename mismatch")
        before = P.file_pin(actual_path)
        require(before == {k: ref[k] for k in ("sha256", "byte_length")}, "selected runtime bytes changed: " + key)
        selected[key] = {"path": str(actual_path), **before}
        result[key]["path"] = windows_path(actual_path)
        if "windows_path" in ref:
            require(PureWindowsPath(ref["windows_path"]) == PureWindowsPath(result[key]["path"]),
                    "recorded native runtime path differs from selected actual file")
        if key != "executable":
            archives[key] = P.read(actual_path, maximum=64 * 1024 * 1024)
            require(strict_json(member(archives[key], "package.json"))["version"] == value["app_version"],
                    "actual ASAR package version differs from independent runtime record")
    for key in ("mathjax", "mermaid"):
        ref = value[key]
        require(set(ref) >= {"version", "member"} and (ref["version"] is None or isinstance(ref["version"], str)),
                "runtime module version must be observed or explicitly unknown")
        wanted = ref["member"]
        require(set(wanted) == {"path", "sha256", "byte_length"}, "strict selected module member reference")
        actual = member(archives["obsidian_asar"], wanted["path"])
        require(pin(actual) == {k: wanted[k] for k in ("sha256", "byte_length")}, "selected module member bytes changed")
        if key == "mermaid":
            engine = ref.get("engine", "mermaid")
            engine_name = engine.get("name") if isinstance(engine, dict) else engine
            if engine_name in ("pele", "pele_compatibility_shim"):
                require(ref["version"] is None, "Pele backend must not invent a Mermaid runtime version")
                banner = re.match(rb"/\*! Pele ([0-9]+\.[0-9]+\.[0-9]+) \| MIT License \| https://github.com/obsidianmd/pele \*/", actual)
                require(banner is not None, "selected Pele member must carry its independent installed engine banner")
                observed = banner[1].decode("ascii")
                declared = engine.get("version") if isinstance(engine, dict) else ref.get("engine_version")
                require(declared is None or declared == observed, "Pele engine declaration differs from installed banner")
                result[key]["version_binding"] = {"kind": "installed_engine_banner", "engine_version": observed,
                    "marker": banner[0].decode("ascii"), "runtime_exposed_version": None}
            else:
                require(engine_name == "mermaid", "unknown independently selected diagram renderer backend")
                if ref["version"] not in (None, "unknown"):
                    banner = re.match(rb"// Source: https://unpkg.com/mermaid@([0-9]+\.[0-9]+\.[0-9]+)/dist/mermaid.min.js", actual)
                    require(banner is not None and banner[1].decode("ascii") == ref["version"],
                            "Mermaid version declaration differs from installed source literal")
                    result[key]["version_binding"] = {"kind": "installed_source_literal", "version": ref["version"],
                        "marker": banner[0].decode("ascii"), "runtime_exposed_version": ref.get("runtime_exposed_version")}
    if "app_bundle" in value:
        wanted = value["app_bundle"]["member"]
        actual = member(archives["obsidian_asar"], wanted["path"])
        require(pin(actual) == {k: wanted[k] for k in ("sha256", "byte_length")}, "selected app bundle bytes changed")
    return result, selected


def load_checker(kind):
    path = REPO / "scripts" / CHECKERS[kind]
    before = P.file_pin(path)
    spec = importlib.util.spec_from_file_location("compatibility_independent_" + kind, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    require(P.file_pin(path) == before, "independent semantic checker changed while loading")
    return module, {"path": str(path), **before}


def verify_case(root, case, expected, source):
    kind = case["verification"]["kind"]
    require(expected["case_id"] == case["case_id"] and expected["feature"] == case["feature"]
            and expected["published"] is False and expected["compatibility_status"] == "not_run",
            "selected independent compatibility expectation differs")
    inputs = {key: under(root, path) for key, path in case["verification"]["inputs"].items()}
    if kind == "format":
        require(P.read(inputs["probe"]) == source["格式兼容探针.md"], "format probe is not the selected authored bytes")
        return {"status": "passed", "scope": "frozen hand-authored bytes; native properties/links not_run"}, None
    checker, checker_pin = load_checker(kind)
    if kind == "table":
        checked = checker.verify_report(inputs["project"], inputs["coordination_case"],
                                        inputs["coordination_output"], inputs["preview_root"], inputs["expected"])
        for name in case["verification"]["compare_selected_display_notes"]:
            require(source[name] == P.read(inputs["preview_root"] / name), "table source differs from verified original")
    elif kind == "flow":
        checked = checker.verify_flow(inputs["project"], inputs["case"], inputs["review"], inputs["profile"],
                                      inputs["vault"], inputs["expected"])
        require(files(inputs["vault"]) == source, "flow bytes differ from independently checked output")
    elif kind in ("formula", "image"):
        checked = checker.verify_output(inputs["case"], inputs["profile"], inputs["vault"], inputs["expected"])
        require(files(inputs["vault"]) == source, "selected bytes differ from independently checked output")
    else:
        checked = checker.verify_output(inputs["case"], inputs["api_config"], inputs["profile"], inputs["vault"], inputs["expected"])
        require(files(inputs["vault"]) == source, "scan bytes differ from independently checked output")
        require(checked["representation"] == expected["representation"], "independent scan representation mismatch")
    if kind in ("table", "flow"):
        bridge = strict_json(P.read(under(root, case["collector_input_file"])))
        for key, value in expected["semantic_source"].items():
            require(bridge["expected"].get(key) == value, "collector bridge differs from independent author semantics: " + key)
        require(bridge["expected"]["representation"] == expected["representation"], "collector bridge representation differs")
        if kind == "flow":
            require(bridge["plan"] == strict_json(P.read(inputs["vault"] / "plan.json")),
                    "flow collector bridge plan differs from independently checked original")
        # Table SQL and legacy native bridge are separate real import runs.
        # The complete display bytes and author semantic fields above must
        # match, while their request/project/occurrence identities stay intact.
    return {"status": "passed", "result": checked, "budget_scope": case["verification"].get("budget_validation_scope")}, checker_pin


# Executed only by a separately launched, identity-checked native collector.
# The preparation utility itself never imports Playwright or starts a process.
RUNTIME_HELPERS = r"""
const compatibilityInput = input.spec;
assert(compatibilityInput && compatibilityInput.runtime && ['light','dark'].includes(compatibilityInput.theme));
const compatibilityCdp = await obs.context().newCDPSession(obs);
const compatibilityScripts = new Map();
compatibilityCdp.on('Debugger.scriptParsed', e => compatibilityScripts.set(e.scriptId, {id:e.scriptId,url:e.url}));
await compatibilityCdp.send('Debugger.enable');
const compatibilityIdentity = async () => {
  const expected = compatibilityInput.runtime;
  const actual = await obs.evaluate(runtime => {
    const fs = require('fs'), rawFs = require('original-fs'), path = require('path'), crypto = require('crypto');
    const pin = file => {const raw=rawFs.readFileSync(file); return {sha256:crypto.createHash('sha256').update(raw).digest('hex'),byte_length:raw.length};};
    const files = {};
    for (const key of ['executable','app_asar','obsidian_asar']) files[key] = pin(runtime[key].path);
    return {appVersion:require('@electron/remote').app.getVersion(),execPath:process.execPath,files,
      appPackageVersion:JSON.parse(fs.readFileSync(path.join(runtime.app_asar.path,'package.json'),'utf8')).version,
      obsidianPackageVersion:JSON.parse(fs.readFileSync(path.join(runtime.obsidian_asar.path,'package.json'),'utf8')).version,
      themeClass:document.body.className};
  }, expected);
  const norm = p => p.replace(/\\+/g,'/').toLowerCase();
  assert.equal(norm(actual.execPath),norm(expected.executable.path),'actual native executable path');
  assert.equal(actual.appVersion,expected.app_version,'actual application version from independent installation');
  assert.equal(actual.appPackageVersion,expected.app_version);
  assert.equal(actual.obsidianPackageVersion,expected.app_version);
  for (const key of ['executable','app_asar','obsidian_asar'])
    assert.deepEqual(actual.files[key],{sha256:expected[key].sha256,byte_length:expected[key].byte_length},'actual selected installation bytes');
  await obs.waitForFunction(theme => document.body.classList.contains('theme-'+theme),compatibilityInput.theme,{timeout:15000});
  actual.themeClass = await obs.evaluate(() => document.body.className);
  assert(actual.themeClass.split(/\s+/).includes('theme-'+compatibilityInput.theme),'actual selected theme');
  const copied = await obs.evaluate(({root,pins}) => {
    const fs=require('fs'),path=require('path'),crypto=require('crypto');
    return Object.fromEntries(Object.keys(pins).map(name => {
      const raw=fs.readFileSync(path.join(root,name));
      return [name,{sha256:crypto.createHash('sha256').update(raw).digest('hex'),byte_length:raw.length}];
    }));
  },{root:input.vault,pins:input.source_pins});
  assert.deepEqual(copied,input.source_pins,'native collector reads all exact frozen output files');
  return actual;
};
const compatibilityLoaded = async () => {
  const globals = await obs.evaluate(() => ({mathjaxVersion:window.MathJax?.version ?? null,
    mermaidVersion:window.mermaid?.version ?? null,mermaidPresent:!!window.mermaid,
    pelePresent:typeof pele!=='undefined',peleVersion:typeof pele!=='undefined'?pele.version??null:null,
    resourceUrls:performance.getEntriesByType('resource').map(e=>e.name)}));
  const evidence = {globals,modules:{}};
  const crypto = await import('node:crypto');
  const checks = [['mathjax',compatibilityInput.require_mathjax_runtime],['mermaid',compatibilityInput.require_mermaid_runtime]];
  if (compatibilityInput.runtime.app_bundle) checks.push(['app_bundle',true]);
  for (const [key,required] of checks) {
    const expected = compatibilityInput.runtime[key], wanted = expected.member;
    if (!required) {evidence.modules[key]={status:'not_loaded_or_not_required',declaredVersion:expected.version};continue;}
    if (key==='mathjax') assert.equal(globals.mathjaxVersion,expected.version,'actual MathJax runtime version');
    else if (key==='mermaid') {
      const engine=typeof expected.engine==='object'?expected.engine.name:expected.engine;
      if (engine!=='pele' && engine!=='pele_compatibility_shim')
        assert(globals.mermaidPresent,'actual standard Mermaid API is present');
      if (globals.mermaidVersion!==null && expected.version!==null && expected.version!=='unknown')
        assert.equal(globals.mermaidVersion,expected.version,'actual exposed Mermaid version');
      if (expected.version!==null && expected.version!=='unknown' && globals.mermaidVersion===null)
        assert.equal(expected.version_binding?.version,expected.version,'unknown global version still requires installed source literal and executed member bytes');
    }
    const basename = wanted.path.split('/').pop();
    const candidates = [...compatibilityScripts.values()].filter(e => e.url.split(/[?#]/)[0].endsWith('/'+basename));
    let bound = null;
    for (const candidate of candidates) {
      const response = await compatibilityCdp.send('Debugger.getScriptSource',{scriptId:candidate.id});
      const raw = Buffer.from(response.scriptSource,'utf8'), prefix = raw.subarray(0,wanted.byte_length);
      const digest = crypto.createHash('sha256').update(prefix).digest('hex');
      const suffix = raw.subarray(wanted.byte_length).toString('utf8');
      // Obsidian may append an explicit sourceURL comment to evaluated library
      // bytes. Do not discard code, select another answer, or normalize syntax.
      const allowedSuffix = suffix==='' || suffix==='\n//# sourceURL='+candidate.url || suffix==='\n//# sourceURL='+candidate.url+'\n';
      if (prefix.length===wanted.byte_length && digest===wanted.sha256 && allowedSuffix) {
        bound={status:'passed',url:candidate.url,member:wanted,rawSha256:crypto.createHash('sha256').update(raw).digest('hex'),
          rawByteLength:raw.length,sourceUrlComment:suffix,declaredVersion:expected.version??null,backend:expected.engine??expected.backend??null,
          observedVersion:key==='mathjax'?globals.mathjaxVersion:key==='mermaid'?globals.mermaidVersion:null,
          versionBinding:expected.version_binding??null,
          method:'actual CDP executed script source bound to installed ASAR member bytes'};
        break;
      }
    }
    assert(bound,'actual executed '+key+' resource must match the independently pinned installed member');
    evidence.modules[key]=bound;
  }
  return evidence;
};
"""


def replace_once(text, before, after):
    require(text.count(before) == 1, "collector derivation anchor must occur exactly once")
    return text.replace(before, after)


PELE_READ = r"""
    const svgPoint = (element, point) => {
      const p=new DOMPoint(point.x,point.y).matrixTransform(element.getScreenCTM());
      return {x:p.x,y:p.y};
    };
    const attributes = element => element ? Object.fromEntries(Array.from(element.attributes).map(a=>[a.name,a.value])) : null;
    const label = element => {
      if(!element)return null;const box=element.getBBox(),style=getComputedStyle(element);
      return {text:element.textContent,attributes:attributes(element),rect:rect(element),
        bbox:{x:box.x,y:box.y,width:box.width,height:box.height},glyphRanges:ranges(element),
        style:{display:style.display,visibility:style.visibility,opacity:style.opacity,fill:style.fill}};
    };
    const outline = element => {
      if(!element)return [];
      if(element.tagName==='polygon')return Array.from(element.points).map(p=>svgPoint(element,p));
      if(element.tagName==='rect') {
        const x=element.x.baseVal.value,y=element.y.baseVal.value,w=element.width.baseVal.value,h=element.height.baseVal.value;
        return [{x,y},{x:x+w,y},{x:x+w,y:y+h},{x,y:y+h}].map(p=>svgPoint(element,p));
      }
      return [];
    };
    const pele = svg?.classList.contains('pele-flowchart') ? {
      svgAttributes:attributes(svg),
      nodes:Array.from(svg.querySelectorAll('g.pele-node')).map(node=>{
        const shapes=Array.from(node.querySelectorAll(':scope > rect,:scope > polygon'));
        const labels=Array.from(node.querySelectorAll(':scope > text.pele-label'));
        return {dataId:node.getAttribute('data-id'),attributes:attributes(node),rect:rect(node),
          shapes:shapes.map(shape=>({tag:shape.tagName,attributes:attributes(shape),outline:outline(shape)})),
          labels:labels.map(label)};
      }),
      edges:Array.from(svg.querySelectorAll('g.pele-edge')).map(edge=>{
        const lines=Array.from(edge.querySelectorAll(':scope > path:not(.pele-marker)'));
        const markers=Array.from(edge.querySelectorAll(':scope > path.pele-marker'));
        return {dataId:edge.getAttribute('data-id'),attributes:attributes(edge),
          lines:lines.map(line=>{
            const length=line.getTotalLength(),start=line.getPointAtLength(0),end=line.getPointAtLength(length),nearEnd=line.getPointAtLength(Math.max(0,length-2));
            return {attributes:attributes(line),length,start:svgPoint(line,start),end:svgPoint(line,end),nearEnd:svgPoint(line,nearEnd)};
          }),
          markers:markers.map(marker=>{
            const d=marker.getAttribute('d');
            const match=/^M(-?[\d.]+),(-?[\d.]+)L(-?[\d.]+),(-?[\d.]+)L(-?[\d.]+),(-?[\d.]+)Z$/.exec(d);
            return {attributes:attributes(marker),points:match?[1,3,5].map(i=>svgPoint(marker,{x:Number(match[i]),y:Number(match[i+1])})):null};
          })};
      }),
      edgeLabels:Array.from(svg.querySelectorAll('g.pele-edge-label')).map(edge=>({dataId:edge.getAttribute('data-id'),
        attributes:attributes(edge),labels:Array.from(edge.querySelectorAll(':scope > text')).map(label)}))
    } : null;
"""


PELE_VALIDATE = r"""
const compatibilityPeleCheck = (actual, expected) => {
  const nodes=[...expected.nodes].sort((a,b)=>a.id.localeCompare(b.id));
  const wanted=nodes.map((n,i)=>({id:'n'+i,kind:n.kind,text:n.kind==='decision'?n.label+'：'+n.criterion:n.label}));
  assert.deepEqual(actual.nodes.map(n=>n.dataId),wanted.map(n=>n.id),'actual Pele data-id values, without invented Mermaid IDs');
  const visible = value => {
    assert(value && value.rect.width>0 && value.rect.height>0 && value.bbox.width>0 && value.bbox.height>0,'actual visible SVG text geometry');
    assert(value.style.display!=='none' && value.style.visibility!=='hidden' && Number(value.style.opacity)>0,'actual SVG label visibility');
  };
  actual.nodes.forEach((node,i)=>{
    const wantedNode=wanted[i];assert.equal(node.shapes.length,1);assert.equal(node.labels.length,1);
    assert.equal(node.shapes[0].tag,wantedNode.kind==='decision'?'polygon':'rect','actual Pele decision/step shape');
    assert(node.attributes.class.split(/\s+/u).includes(wantedNode.kind==='decision'?'pele-shape-diam':'pele-shape-rect'));
    assert.equal(node.labels[0].text,wantedNode.text,'literal native Pele node text and criterion');visible(node.labels[0]);
    assert.equal(node.shapes[0].outline.length,4);assert(node.rect.width>0 && node.rect.height>0);
  });
  const ids=new Map(nodes.map((n,i)=>[n.id,'n'+i]));
  const relations=[...expected.edges].sort((a,b)=>a.id.localeCompare(b.id));
  const meaning=edge=>({sequence:'随后',causes:'导致',promotes:'+：促进',inhibits:'−：抑制'}[edge.kind]??
    (edge.kind==='branch'?'判据为'+(edge.branch_outcome?'真':'假')+'：'+nodes.find(n=>n.id===edge.left).criterion:assert.fail('unsupported independent relation')));
  const edgeIds=relations.map(edge=>'L_'+ids.get(edge.left)+'_'+ids.get(edge.right)+'_0');
  assert.deepEqual(actual.edges.map(e=>e.dataId),edgeIds,'actual Pele edge data-id source and target');
  assert.deepEqual(actual.edgeLabels.map(e=>e.dataId),edgeIds,'actual signed/conditional label belongs to its exact native edge');
  actual.edgeLabels.forEach((edge,i)=>{assert.equal(edge.labels.length,1);assert.equal(edge.labels[0].text,meaning(relations[i]),'literal native signed/conditional Pele edge label');visible(edge.labels[0]);});
  const distance=(a,b)=>Math.hypot(a.x-b.x,a.y-b.y);
  const outlineDistance=(p,outline)=>Math.min(...outline.map((a,i)=>{
    const b=outline[(i+1)%outline.length],dx=b.x-a.x,dy=b.y-a.y,denominator=dx*dx+dy*dy;
    const t=denominator?Math.max(0,Math.min(1,((p.x-a.x)*dx+(p.y-a.y)*dy)/denominator)):0;
    return distance(p,{x:a.x+t*dx,y:a.y+t*dy});
  }));
  return actual.edges.map((edge,i)=>{
    const relation=relations[i];assert.equal(edge.lines.length,1);assert.equal(edge.markers.length,1,'exactly one native Pele end arrow triangle');
    const line=edge.lines[0],marker=edge.markers[0],points=marker.points;
    assert(points?.length===3,'actual marker is a closed three-point triangle; retain its original path');
    assert(Number.isFinite(line.length)&&line.length>0);assert(line.end.y>line.start.y,'TD visual arrow direction');
    assert.equal(line.attributes['marker-start']??null,null);assert.equal(line.attributes['marker-end']??null,null);
    const tip=points[0],base={x:(points[1].x+points[2].x)/2,y:(points[1].y+points[2].y)/2};
    assert(distance(points[1],points[2])>0 && distance(tip,base)>0,'nondegenerate actual arrow triangle');
    const tangent={x:line.end.x-line.nearEnd.x,y:line.end.y-line.nearEnd.y};
    assert(tangent.x*(tip.x-base.x)+tangent.y*(tip.y-base.y)>0,'actual native arrow triangle points along the line end');
    assert(distance(base,line.end)<=3 && distance(tip,line.end)<=20,'actual end triangle stays attached to its line');
    const source=actual.nodes.find(n=>n.dataId===ids.get(relation.left));
    const target=actual.nodes.find(n=>n.dataId===ids.get(relation.right));
    const sourceDistance=outlineDistance(line.start,source.shapes[0].outline),targetDistance=outlineDistance(tip,target.shapes[0].outline);
    assert(sourceDistance<=2 && targetDistance<=2,'actual line leaves source shape and arrow tip reaches target shape');
    return {relationId:relation.id,source:relation.left,target:relation.right,backend:'pele',dataId:edge.dataId,
      actualLine:line,actualMarker:marker,sourceDistance,targetDistance,endpointToleranceCssPixels:2,positiveVerticalDirection:true};
  });
};
"""


SCREENSHOT_PIXELS = r"""
const compatibilityZlib=await import('node:zlib');
const compatibilityCrypto=await import('node:crypto');
const compatibilityCrcTable=Array.from({length:256},(_,n)=>{
  for(let i=0;i<8;i++)n=(n&1)?0xedb88320^(n>>>1):n>>>1;return n>>>0;
});
const compatibilityCrc=bytes=>{
  let crc=0xffffffff;for(const b of bytes)crc=compatibilityCrcTable[(crc^b)&255]^(crc>>>8);return (crc^0xffffffff)>>>0;
};
const compatibilityPng=raw=>{
  assert.equal(raw.subarray(0,8).toString('hex'),'89504e470d0a1a0a','actual screenshot PNG signature');
  let at=8,header=null,ended=false;const chunks=[];
  while(at<raw.length){
    assert(at+12<=raw.length,'bounded screenshot PNG chunk');const size=raw.readUInt32BE(at),type=raw.subarray(at+4,at+8).toString('ascii');
    assert(size<=64*1024*1024 && at+12+size<=raw.length,'bounded screenshot PNG data');
    assert.equal(compatibilityCrc(raw.subarray(at+4,at+8+size)),raw.readUInt32BE(at+8+size),'actual PNG chunk CRC');
    const bytes=raw.subarray(at+8,at+8+size);
    if(type==='IHDR'){assert(!header && at===8 && size===13);header=bytes;}
    else if(type==='IDAT'){assert(header);chunks.push(bytes);}
    else if(type==='IEND'){assert.equal(size,0);assert.equal(at+12,raw.length);ended=true;break;}
    else assert(type[0]===type[0].toLowerCase(),'no unsupported critical screenshot PNG chunk');
    at+=12+size;
  }
  assert(header && ended && chunks.length);const width=header.readUInt32BE(0),height=header.readUInt32BE(4),kind=header[9];
  assert(width>0 && height>0 && width*height<=16*1024*1024,'bounded full screenshot pixel array');
  assert.equal(header[8],8);assert([2,6].includes(kind),'screenshot must be RGB/RGBA PNG');
  assert.equal(header[10],0);assert.equal(header[11],0);assert.equal(header[12],0,'no interlaced screenshot');
  const channels=kind===2?3:4,rowSize=width*channels;
  const packed=compatibilityZlib.inflateSync(Buffer.concat(chunks),{maxOutputLength:(rowSize+1)*height});
  assert.equal(packed.length,(rowSize+1)*height);const decoded=Buffer.alloc(rowSize*height);
  const paeth=(a,b,c)=>{const p=a+b-c,pa=Math.abs(p-a),pb=Math.abs(p-b),pc=Math.abs(p-c);return pa<=pb&&pa<=pc?a:pb<=pc?b:c;};
  for(let y=0;y<height;y++){
    const filter=packed[y*(rowSize+1)];assert(filter<=4);
    for(let x=0;x<rowSize;x++){
      const a=x>=channels?decoded[y*rowSize+x-channels]:0,b=y?decoded[(y-1)*rowSize+x]:0,c=y&&x>=channels?decoded[(y-1)*rowSize+x-channels]:0;
      decoded[y*rowSize+x]=(packed[y*(rowSize+1)+1+x]+[0,a,b,Math.floor((a+b)/2),paeth(a,b,c)][filter])&255;
    }
  }
  const rgba=Buffer.alloc(width*height*4);
  for(let p=0;p<width*height;p++){rgba[p*4]=decoded[p*channels];rgba[p*4+1]=decoded[p*channels+1];rgba[p*4+2]=decoded[p*channels+2];rgba[p*4+3]=channels===4?decoded[p*channels+3]:255;}
  return {width,height,rgba,sha256:compatibilityCrypto.createHash('sha256').update(rgba).digest('hex')};
};
"""


NATIVE_SCREENSHOT = r"""
      native = await obs.evaluate(file => {
        const e=Array.from(document.querySelectorAll('.workspace-leaf.mod-active .markdown-preview-view img')).find(e=>decodeURI(e.currentSrc||e.src).includes(file));
        if(!e || !e.complete || !e.naturalWidth)throw Error('actual native SVG image is not loaded');
        const fs=require('fs'),path=require('path'),crypto=require('crypto'),owner=path.join(window.app.vault.adapter.basePath,file);
        const url=new URL(e.currentSrc||e.src),norm=p=>p.replace(/\\+/g,'/').toLowerCase();
        if(!['app:','file:'].includes(url.protocol) || norm(decodeURIComponent(url.pathname).replace(/^\/(?=[a-z]:)/i,''))!==norm(owner))
          throw Error('actual native SVG src does not name the exact owned pinned file');
        const bytes=fs.readFileSync(owner),r=e.getBoundingClientRect(),style=getComputedStyle(e),view=document.querySelector('.workspace-leaf.mod-active .markdown-preview-view');
        let ancestor=e.parentElement;while(ancestor && /^rgba\(.+,\s*0\)$/.test(getComputedStyle(ancestor).backgroundColor))ancestor=ancestor.parentElement;
        return {src:e.currentSrc||e.src,sourceFile:owner,sourceProtocol:url.protocol,complete:e.complete,naturalWidth:e.naturalWidth,naturalHeight:e.naturalHeight,
          svgDataUrl:'data:image/svg+xml;base64,'+bytes.toString('base64'),svgSha256:crypto.createHash('sha256').update(bytes).digest('hex'),sourceByteLength:bytes.length,
          rect:{x:r.x,y:r.y,width:r.width,height:r.height},clip:{x:Math.floor(r.x),y:Math.floor(r.y),width:Math.ceil(r.right)-Math.floor(r.x),height:Math.ceil(r.bottom)-Math.floor(r.y)},
          viewport:{width:innerWidth,height:innerHeight,devicePixelRatio},backdrop:ancestor?getComputedStyle(ancestor).backgroundColor:getComputedStyle(document.body).backgroundColor,
          style:Object.fromEntries(['width','height','objectFit','objectPosition','imageRendering','borderRadius','borderTopWidth','borderRightWidth','borderBottomWidth','borderLeftWidth','paddingTop','paddingRight','paddingBottom','paddingLeft','filter','opacity','mixBlendMode','transform','boxShadow'].map(k=>[k,style[k]])),
          nativePreviewBackground:getComputedStyle(view).backgroundColor,nativeBackgroundPrimary:getComputedStyle(view).getPropertyValue('--background-primary').trim(),
          dom:view.outerHTML,pixelMethod:'actual native img composited screenshot; native src remains the owned app-protocol file'};
      },semantic.file);
      nativeDecodedPng='note-'+index+'-svg-native-displayed.png';nativePreviewScreenshot='note-'+index+'-svg-preview.png';
      fs.writeFileSync(path.join(work,'note-'+index+'-svg-native-before-pixels.html'),native.dom,{flag:'wx'});
      await obs.screenshot({path:path.join(work,nativePreviewScreenshot),fullPage:true,timeout:60000});
      const {svgDataUrl:earlyUrl,...beforePixels}=native;
      record.svgEvidence={status:'checking',native:beforePixels,nativeDecodedPng,nativePreviewScreenshot,semanticExpected:semantic};
      assert.equal(native.svgSha256,input.source_pins[semantic.file].sha256);assert.equal(native.sourceByteLength,input.source_pins[semantic.file].byte_length);
      assert(native.clip.x>=0 && native.clip.y>=0 && native.clip.x+native.clip.width<=native.viewport.width && native.clip.y+native.clip.height<=native.viewport.height,'entire actual native image clip is visible');
      for(const key of ['borderTopWidth','borderRightWidth','borderBottomWidth','borderLeftWidth','paddingTop','paddingRight','paddingBottom','paddingLeft'])assert.equal(native.style[key],'0px');
      assert.equal(native.style.filter,'none');assert.equal(native.style.opacity,'1');assert.equal(native.style.mixBlendMode,'normal');assert.equal(native.style.transform,'none');
      const nativePng=await obs.screenshot({clip:native.clip,scale:'css',timeout:60000});
      fs.writeFileSync(path.join(work,nativeDecodedPng),nativePng,{flag:'wx'});
      const pixels=compatibilityPng(nativePng);native.decodedRgbaSha256=pixels.sha256;native.rgbaWidth=pixels.width;native.rgbaHeight=pixels.height;
      native.transparentPixelCount=0;for(let p=3;p<pixels.rgba.length;p+=4)if(pixels.rgba[p]<255)native.transparentPixelCount++;
      native.pixelCount=pixels.width*pixels.height;
      const inset=Math.ceil(parseFloat(native.style.borderRadius)||0)+2,cx=Math.ceil(native.rect.x)-native.clip.x+inset,cy=Math.ceil(native.rect.y)-native.clip.y+inset;
      assert(cx<pixels.width && cy<pixels.height);native.cornerRgba=Array.from(pixels.rgba.subarray((cy*pixels.width+cx)*4,(cy*pixels.width+cx)*4+4));native.whiteCanvasSample={x:cx,y:cy,method:'displayed image corner inside the observed CSS radius; intrinsic white canvas also checked from exact source SVG'};
      const {svgDataUrl,...earlyNative}=native;record.svgEvidence={status:'checking',native:earlyNative,nativeDecodedPng,nativePreviewScreenshot,semanticExpected:semantic};
"""


AUXILIARY_SCREENSHOT = r"""
        if(native.pixelMethod){
          let intrinsicTransparent=0;for(let p=3;p<rgba.length;p+=4)if(rgba[p]<255)intrinsicTransparent++;
          if(semantic.source){assert.equal(intrinsicTransparent,0,'pinned auxiliary intrinsic SVG also retains the opaque white source canvas');assert.deepEqual(rgba.slice(0,4),[255,255,255,255]);}
          record.svgEvidence.intrinsicAuxiliary={width:native.naturalWidth,height:native.naturalHeight,transparentPixelCount:intrinsicTransparent,rgbaSha256:createHash('sha256').update(Buffer.from(rgba)).digest('hex')};
          const css={position:'absolute',left:native.rect.x+'px',top:native.rect.y+'px',width:native.rect.width+'px',height:native.rect.height+'px',margin:'0',padding:'0',border:'0',
            objectFit:native.style.objectFit,objectPosition:native.style.objectPosition,imageRendering:native.style.imageRendering,borderRadius:native.style.borderRadius,boxShadow:native.style.boxShadow};
          const html='<!doctype html><html><body><img id="matched-image"><script>document.body.style.margin="0";document.body.style.background='+JSON.stringify(native.backdrop)+';const image=document.getElementById("matched-image");Object.assign(image.style,'+JSON.stringify(css)+');image.src='+JSON.stringify(native.svgDataUrl)+';</script></body></html>';
          const url='data:text/html;base64,'+Buffer.from(html,'utf8').toString('base64');
          await obs.evaluate(({id,url})=>require('@electron/remote').BrowserWindow.fromId(id).loadURL(url),{id:auxiliaryId,url});
          await auxiliary.waitForURL(url);await auxiliary.setViewportSize({width:native.viewport.width,height:native.viewport.height});
          await auxiliary.waitForFunction(()=>document.querySelector('#matched-image')?.complete && document.querySelector('#matched-image').naturalWidth>0);
          const matched=await auxiliary.evaluate(()=>{const e=document.querySelector('#matched-image'),r=e.getBoundingClientRect();return {src:e.src,complete:e.complete,naturalWidth:e.naturalWidth,naturalHeight:e.naturalHeight,devicePixelRatio,rect:{x:r.x,y:r.y,width:r.width,height:r.height},dom:document.documentElement.outerHTML};});
          assert.equal(matched.devicePixelRatio,native.viewport.devicePixelRatio);assert.equal(matched.naturalWidth,native.naturalWidth);assert.equal(matched.naturalHeight,native.naturalHeight);
          for(const key of ['x','y','width','height'])assert(Math.abs(matched.rect[key]-native.rect[key])<.02,'same-instance auxiliary has matching actual CSS geometry');
          const auxiliaryPng=await auxiliary.screenshot({clip:native.clip,scale:'css',timeout:60000}),decoded=compatibilityPng(auxiliaryPng);
          const auxiliaryFile='note-'+index+'-svg-matched-auxiliary.png';fs.writeFileSync(path.join(work,auxiliaryFile),auxiliaryPng,{flag:'wx'});
          fs.writeFileSync(path.join(work,'note-'+index+'-svg-matched-auxiliary.html'),matched.dom,{flag:'wx'});
          record.svgEvidence.matchedScreenshot={auxiliaryFile,matched:{...matched,src:'exact pinned source SVG data URL in owned auxiliary only'},width:decoded.width,height:decoded.height,
            rgbaSha256:decoded.sha256,nativeRgbaSha256:native.decodedRgbaSha256,method:'actual native app-protocol img remains unchanged; same-instance pinned auxiliary at matching viewport/CSS position and size; complete decoded RGBA arrays'};
          assert.equal(decoded.width,native.rgbaWidth);assert.equal(decoded.height,native.rgbaHeight);auxiliaryRgbaSha256=decoded.sha256;
        }
"""


def screenshot_image_collector(text):
    start = "      const native = await obs.evaluate(file => {"
    end = "      // Electron CDP rejects Target.createTarget."
    require(text.count(start) == text.count(end) == 1, "exact native SVG pixel observation anchors required")
    a, b = text.index(start), text.index(end)
    legacy = text[a:b].replace("const native =", "native =", 1).replace("const nativeDecodedPng=", "nativeDecodedPng=", 1).replace("const nativePreviewScreenshot=", "nativePreviewScreenshot=", 1)
    text = text[:a] + "      let native,nativeDecodedPng,nativePreviewScreenshot;\n      if(input.spec.runtime.app_version==='1.15.0'){\n" + NATIVE_SCREENSHOT + "      }else{\n" + legacy + "      }\n" + text[b:]
    text = replace_once(text, "        const auxiliaryRgbaSha256 = createHash('sha256').update(Buffer.from(rgba)).digest('hex');",
                        "        let auxiliaryRgbaSha256 = createHash('sha256').update(Buffer.from(rgba)).digest('hex');" + AUXILIARY_SCREENSHOT)
    text = replace_once(text, "          native:nativeReceipt, auxiliaryRgbaSha256, geometry, semanticExpected:semantic,",
                        "          ...record.svgEvidence,native:nativeReceipt, auxiliaryRgbaSha256, geometry, semanticExpected:semantic,")
    text = replace_once(text, "method:'native Obsidian external SVG img decoding plus exact-byte auxiliary same-Chromium rendered bbox; equal decoded RGBA hashes',",
                        "method:native.pixelMethod?'actual unchanged native SVG img screenshot plus same-instance pinned auxiliary at matching CSS geometry; equal complete screenshot RGBA arrays; intrinsic source canvas independently checked':'native Obsidian external SVG img decoding plus exact-byte auxiliary same-Chromium rendered bbox; equal decoded RGBA hashes',")
    return text


def derived_collector(raw, kind):
    text = raw.decode("utf8")
    # A bridge contains only the pinned old semantic input fields, never the old
    # profile, Vault, port or runtime identity. The launcher supplies those anew.
    input_line = "const input = JSON.parse(fs.readFileSync(inputPath, 'utf8'));"
    text = replace_once(text, input_line, input_line + "\nObject.assign(input, input.spec.bridge ?? {});")
    text = replace_once(text, "let owned = false;", RUNTIME_HELPERS + "\nlet owned = false;")
    if kind == "image":
        text = replace_once(text, "let owned = false;", SCREENSHOT_PIXELS + "\nlet owned = false;")
        text = screenshot_image_collector(text)
    if kind == "flow":
        text = replace_once(text, "const svg = view.querySelector('.mermaid svg');",
                            "const svg = view.querySelector('.mermaid svg');" + PELE_READ)
        text = replace_once(text, "svg: svg?.outerHTML ?? null,", "svg: svg?.outerHTML ?? null, pele,")
        text = replace_once(text, "svg: diagram.svg, nodes: diagram.nodes,", "svg: diagram.svg, pele: diagram.pele, nodes: diagram.nodes,")
        text = replace_once(text, "let owned = false;", PELE_VALIDATE + "\nlet owned = false;")
        text = replace_once(text, "    if (hasDiagram) {\n      const expectedNodes", """    if (hasDiagram) {
      const engine=input.spec.runtime.mermaid.engine;
      const expectedPele=['pele','pele_compatibility_shim'].includes(typeof engine==='object'?engine.name:engine);
      assert.equal(Boolean(actual.pele),expectedPele,'actual graph renderer matches the independently selected backend');
      if(actual.pele)record.edgeDirections=compatibilityPeleCheck(actual.pele,input.expected);
    }
    if (hasDiagram && !actual.pele) {
      const expectedNodes""")
    if kind != "table":
        text = replace_once(text, "assert.equal(result.identity.appVersion, '1.6.7');",
                            "assert.equal(result.identity.appVersion, input.spec.runtime.app_version);")
    text = replace_once(text, "assert.deepEqual(result.identity.communityPlugins, []);",
                        "assert.deepEqual(result.identity.communityPlugins, []);\n  result.compatibilityIdentity = await compatibilityIdentity();")
    if kind in ("formula", "scan", "image"):
        text = replace_once(text, P.CLICK_ANCHOR, P.CLICK_ANCHOR + P.CLICK_CHECK)
    text = replace_once(text, "  result.status = 'passed';",
                        "  result.compatibilityRuntime = await compatibilityLoaded();\n  result.status = 'passed';")
    # Use the launcher's common result path without changing any semantic gate.
    if kind == "table":
        text = replace_once(text, "path.join(work, 'render-result.json')", "path.join(work, 'result.json')")
    elif kind == "flow":
        text = replace_once(text, "path.join(work, 'render-result.json')", "path.join(work, 'result.json')")
    return text.encode("utf8")


FORMAT_TEXT = r"""
const compatibilityParagraphText=(raw,terminalBreak)=>({paragraphText:raw,
  comparisonText:terminalBreak && raw.endsWith('\n')?raw.slice(0,-1):raw,
  removedOneTerminalLf:terminalBreak && raw.endsWith('\n')});
"""


FORMAT_COLLECTOR = r"""import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
const [modulePath,inputPath]=process.argv.slice(2);
const {chromium}=await import(pathToFileURL(modulePath));
const input=JSON.parse(fs.readFileSync(inputPath,'utf8'));
const work=path.dirname(inputPath);
const browser=await chromium.connectOverCDP('http://127.0.0.1:'+input.port);
const obs=browser.contexts()[0].pages()[0];
const result={status:'running',notes:[],clicks:[]};
__RUNTIME_HELPERS__
__FORMAT_TEXT__
const readFormatTarget = wanted => {
  const view=document.querySelector('.workspace-leaf.mod-active .markdown-preview-view');
  const matches=Array.from(view.querySelectorAll(wanted.blockId?'p':'h2')).map(element=>{
    const finalElement=element.lastElementChild;
    const terminalBreak=element.tagName==='P' && finalElement?.tagName==='BR' &&
      Array.from(element.childNodes).slice(Array.from(element.childNodes).indexOf(finalElement)+1).every(n=>n.nodeType===Node.TEXT_NODE && n.textContent==='\n');
    const raw=element.innerText;
    const comparison={paragraphText:raw,comparisonText:terminalBreak && raw.endsWith('\n')?raw.slice(0,-1):raw,
      removedOneTerminalLf:terminalBreak && raw.endsWith('\n')};
    const a=element.getBoundingClientRect(),b=view.getBoundingClientRect();
    return {...comparison,terminalBreak,dom:element.outerHTML,rect:{x:a.x,y:a.y,width:a.width,height:a.height},
      previewRect:{x:b.x,y:b.y,width:b.width,height:b.height},
      visible:a.width>0 && a.height>0 && a.top>=b.top-1 && a.bottom<=b.bottom+1};
  }).filter(element=>element.comparisonText===wanted.text);
  const cache=window.app.metadataCache.getFileCache(window.app.vault.getAbstractFileByPath(wanted.file));
  return {activeFile:window.app.workspace.getActiveFile()?.path,activeState:window.app.workspace.activeLeaf.view.getState(),
    blockId:wanted.blockId??null,cachedBlock:wanted.blockId?cache?.blocks?.[wanted.blockId]??null:null,
    targetCount:matches.length,targets:matches,visible:matches.length===1 && matches[0].visible &&
      window.app.workspace.getActiveFile()?.path===wanted.file && (!wanted.blockId || cache?.blocks?.[wanted.blockId]?.id===wanted.blockId),dom:view.outerHTML};
};
let owned=false;
try {
  await obs.waitForFunction(()=>window.app?.workspace?.layoutReady,{timeout:30000});
  const identity=await obs.evaluate(()=>({userData:require('@electron/remote').app.getPath('userData'),
    vaultPath:window.app.vault.adapter.basePath,communityPlugins:Object.keys(window.app.plugins.plugins)}));
  const norm=p=>p.replace(/\\+/g,'/').toLowerCase();
  assert.equal(norm(identity.userData),norm(input.profile));assert.equal(norm(identity.vaultPath),norm(input.vault));
  owned=true;assert.deepEqual(identity.communityPlugins,[]);
  result.compatibilityIdentity=await compatibilityIdentity();
  const name=input.spec.notes[0],expected=input.spec.format_expected;
  await obs.waitForFunction(p=>window.app.vault.getAbstractFileByPath(p),name);
  await obs.evaluate(async p=>window.app.workspace.getLeaf(false).openFile(window.app.vault.getAbstractFileByPath(p),{active:true,state:{mode:'preview'}}),name);
  await obs.waitForFunction(p=>window.app.workspace.getActiveFile()?.path===p&&window.app.workspace.activeLeaf?.view?.getState()?.mode==='preview',name);
  await obs.locator('.workspace-leaf.mod-active .markdown-preview-view').waitFor({state:'visible'});
  const actual=await obs.evaluate(async name=>{
    const view=document.querySelector('.workspace-leaf.mod-active .markdown-preview-view'),cache=window.app.metadataCache.getFileCache(window.app.vault.getAbstractFileByPath(name));
    return {frontmatter:cache?.frontmatter,blocks:cache?.blocks,rawSource:await window.app.vault.cachedRead(window.app.vault.getAbstractFileByPath(name)),properties:Array.from(view.querySelectorAll('.metadata-property')).map(e=>{
      const value=e.querySelector('.metadata-property-value'),r=e.getBoundingClientRect(),style=getComputedStyle(e);
      return {key:e.getAttribute('data-property-key'),parentType:e.getAttribute('data-property-type'),valueType:value?.getAttribute('data-property-type')??null,
        parentAttributes:Object.fromEntries(Array.from(e.attributes).map(a=>[a.name,a.value])),valueAttributes:value?Object.fromEntries(Array.from(value.attributes).map(a=>[a.name,a.value])):null,
        fieldLabel:e.querySelector('.metadata-property-key-input')?.value??null,titleValue:e.querySelector('.metadata-input-longtext')?.innerText??null,
        checkboxChecked:e.querySelector('.metadata-input-checkbox')?.checked??null,chips:Array.from(e.querySelectorAll('.multi-select-pill-content')).map(n=>n.innerText),
        rect:{width:r.width,height:r.height},display:style.display,visibility:style.visibility,text:e.innerText};}),
      callouts:Array.from(view.querySelectorAll('[data-callout]')).map(e=>({type:e.getAttribute('data-callout'),title:e.querySelector('.callout-title-inner')?.innerText,body:e.querySelector('.callout-content')?.innerText})),
      headings:Array.from(view.querySelectorAll('h2')).map(e=>e.innerText),links:Array.from(view.querySelectorAll('a.internal-link')).map(e=>({href:e.getAttribute('data-href'),text:e.innerText})),dom:view.outerHTML};
  },name);
  const record={file:name,actual,status:'checking'};result.notes.push(record);
  fs.writeFileSync(path.join(work,'note-0.html'),actual.dom,{flag:'wx'});
  await obs.screenshot({path:path.join(work,'note-0.png'),fullPage:true});
  for (const [key,value] of Object.entries(expected.semantic_source.frontmatter)) assert.deepEqual(actual.frontmatter[key],value,'native YAML/Properties type and value');
  for (const [key,type] of Object.entries(expected.collector.properties_types)) {
    const property=actual.properties.find(p=>p.key===key);assert(property,'actual visible Properties row '+key);
    // Native Obsidian calls its aliases/list widget "multitext". Keep the
    // author's YAML array expectation and preserve the observed native enum.
    const current=input.spec.runtime.app_version==='1.15.0';
    const nativeType={text:'text',checkbox:'checkbox',list:current?'aliases':'multitext'}[type];
    assert(nativeType,'explicit supported conceptual Properties type');
    property.typeSource=current?'actual value child data-property-type':'actual property row data-property-type';
    property.type=current?property.valueType:property.parentType;
    property.expectedSemanticType=type;property.expectedNativeType=nativeType;
    assert.equal(property.type,nativeType,'actual Properties renderer type');
    assert(property.rect.width>0 && property.rect.height>0 && property.display!=='none' && property.visibility!=='hidden','actual Properties control has visible area');
    assert.equal(property.fieldLabel,key,'actual visible Properties field label');
    const authoredValue=expected.semantic_source.frontmatter[key];
    if(type==='text')assert.equal(property.titleValue,authoredValue,'actual visible text control preserves the authored value');
    else if(type==='checkbox')assert.equal(property.checkboxChecked,authoredValue,'actual native checkbox preserves the authored boolean');
    else assert.deepEqual(property.chips,authoredValue,'actual native aliases/list chips preserve the authored array');
  }
  const wanted=expected.collector.callout;
  assert(actual.callouts.some(c=>c.type===expected.semantic_source.callout.type&&c.title===wanted.title&&c.body===wanted.body),'native Callout type/title/body');
  const authoredBlock=expected.semantic_source.block,cachedBlock=actual.blocks?.[authoredBlock.id];
  assert(cachedBlock && cachedBlock.id===authoredBlock.id,'actual metadata cache retains the exact authored block ID');
  const sourceLines=actual.rawSource.split('\n'),line=cachedBlock.position.start.line;
  assert.equal(sourceLines[line],authoredBlock.text,'actual cached block source start identifies the exact authored paragraph');
  assert.equal(sourceLines[line+1],'^'+authoredBlock.id,'actual block marker is attached to that exact source paragraph');
  record.status='passed';
  for (const link of expected.collector.real_click_links) {
    const locator=obs.locator('.workspace-leaf.mod-active .markdown-preview-view a.internal-link').filter({hasText:link.label});
    assert.equal(await locator.count(),1);assert.equal(await locator.getAttribute('data-href'),link.target,'actual clicked source fragment');await locator.click();
    await obs.waitForFunction(p=>window.app.workspace.getActiveFile()?.path===p,link.destination);
    const target=link.target_heading??link.target_text;
    const args={text:target,file:link.destination,blockId:link.target_block_id??null};
    const clicked={target:link.target,destination:link.destination,actualClick:true,status:'checking',beforeWait:await obs.evaluate(readFormatTarget,args)};
    result.clicks.push(clicked);const number=result.clicks.length;
    fs.writeFileSync(path.join(work,'click-'+number+'-before-wait.html'),clicked.beforeWait.dom,{flag:'wx'});
    await obs.screenshot({path:path.join(work,'click-'+number+'-before-wait.png'),fullPage:true});
    try {
      await obs.waitForFunction('wanted => ('+readFormatTarget.toString()+')(wanted).visible',args,{timeout:10000});
      clicked.afterWait=await obs.evaluate(readFormatTarget,args);
      assert.equal(clicked.afterWait.targetCount,1);assert(clicked.afterWait.visible,'actual clicked fragment target is visible without corrective scrolling');
      if(link.target_block_id){assert.equal(clicked.afterWait.cachedBlock.id,link.target_block_id);
        const p=clicked.afterWait.targets[0],comparison=compatibilityParagraphText(p.paragraphText,p.terminalBreak);
        assert.equal(comparison.comparisonText,target,'only the observed single terminal BR newline may differ');
        clicked.paragraphComparison=comparison;}
      clicked.visibleTarget=target;clicked.status='passed';
    } finally {
      clicked.finalObservation=await obs.evaluate(readFormatTarget,args);
      fs.writeFileSync(path.join(work,'click-'+number+'.html'),clicked.finalObservation.dom,{flag:'wx'});
      await obs.screenshot({path:path.join(work,'click-'+number+'.png'),fullPage:true});
    }
  }
  result.compatibilityRuntime=await compatibilityLoaded();result.status='passed';
}catch(error){result.status='failed';result.error={message:String(error),stack:error.stack};process.exitCode=1;
}finally{
  fs.writeFileSync(path.join(work,'result.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
  if(owned)await obs.evaluate(()=>setTimeout(()=>require('@electron/remote').app.quit(),100)).catch(()=>{});
  await browser.close();
}
console.log(JSON.stringify({status:result.status,notes:result.notes.length,clicks:result.clicks.length}));
"""


def capture_spec(root, case, expected, source, installed, theme):
    collector = copy.deepcopy(expected["collector"])
    spec = {"case": case["case_id"], "scope": "Frozen bounded visual compatibility; scientific quality not_run; published false",
            "runtime": installed, "theme": theme, "mathjax_version": installed["mathjax"]["version"],
            "require_mathjax_runtime": False, "require_mermaid_runtime": bool(collector.get("graph")),
            "blocked_absent": collector.get("blocked_absent", []), "attachment_images": collector.get("attachment_images", {}),
            "notes": collector["notes"], "format_expected": expected, "bridge": {}}
    if case.get("collector_input_file"):
        bridge = strict_json(P.read(under(root, case["collector_input_file"])))
        spec["bridge"] = {key: copy.deepcopy(bridge[key]) for key in ("plan", "expected", "case_name") if key in bridge}
        require("plan" in spec["bridge"] and "expected" in spec["bridge"], "frozen table/flow semantic bridge required")
        spec["bridge"]["case_name"] = case["author_case"]
    if case["feature"] in ("formula", "image", "scan"):
        for note in spec["notes"]:
            note.setdefault("images", [])
            note.setdefault("required_texts", [])
        spec["require_mathjax_runtime"] = any(note["math_trees"] for note in spec["notes"])
    if case["feature"] == "scan" and expected["representation"] is not None:
        report = strict_json(source["scan.json"])
        semantic = expected["semantic_source"]
        texts = [report["case"]["title"], report["recognition"]["recognized_tex"]]
        for symbol in semantic["symbols"]:
            texts.extend([symbol["symbol"], symbol["definition"], symbol["unit"] or "来源未给出单位"])
        for key in ("conditions", "ranges", "exceptions"):
            texts.extend(semantic[key])
        spec["notes"][0]["preformatted_exact"] = texts
        spec["notes"][0]["required_texts"] = ["作者原始 TeX 未知"]
    return spec


def prepare(args):
    preparation_pin = {"path": str(Path(__file__).absolute()), **P.file_pin(__file__)}
    preparation_dependencies = [{"path": str(Path(module.__file__).absolute()), **P.file_pin(module.__file__)}
                                for module in (P, sys.modules[strict_json.__module__])]
    require(args.theme in ("light", "dark"), "explicit light or dark theme required")
    root = P.guard(args.fixture_root, directory=True)
    target = Path(args.new_root).absolute()
    require(".." not in target.parts, "new preparation root cannot traverse ancestors")
    P.guard(target.parent, directory=True)
    require(not target.exists() and not target.is_symlink(), "preparation root must be brand-new")
    require(not target.is_relative_to(root), "preparation output cannot be inside the immutable fixture root")
    cases, originals = fixture(root, args.fixture_sha256)
    chosen = [case for case in cases["cases"] if case["case_id"] == args.case_id]
    require(len(chosen) == 1, "unknown selected compatibility case")
    case = chosen[0]
    expected = strict_json(originals[case["expected_file"]])
    source = {}
    if case["representative_kind"] == "hand_authored_format_probe":
        require(case["representative_output"] is None and case["verification"]["kind"] == "format",
                "hand-authored format probe must not claim a product output")
        source["格式兼容探针.md"] = originals[case["selected_source"]["file"]]
    else:
        require(case["representative_kind"] == "product_output", "unknown visual case source authority")
        for name, ref in case["representative_output"]["files"].items():
            relative(name)
            raw = originals[ref["captured_file"]]
            require(pin(raw) == {k: ref[k] for k in ("sha256", "byte_length")}, "selected output reference mismatch")
            source[name] = raw
    verified, checker_pin = verify_case(root, case, expected, source)
    installed, runtime_pins = runtime(args.runtime_record, args.runtime_sha256)
    spec = capture_spec(root, case, expected, source, installed, args.theme)
    kind = case["verification"]["kind"]
    if kind == "format":
        derived = FORMAT_COLLECTOR.replace("__RUNTIME_HELPERS__", RUNTIME_HELPERS).replace("__FORMAT_TEXT__", FORMAT_TEXT).encode("utf8")
        collector_pin = None
    else:
        original_collector = P.read(COLLECTORS[kind])
        collector_pin = {"path": str(COLLECTORS[kind]), **pin(original_collector)}
        derived = derived_collector(original_collector, kind)
    require(fixture(root, args.fixture_sha256)[1] == originals, "frozen inputs changed during preparation")
    require(P.file_pin(args.runtime_record)["sha256"] == args.runtime_sha256, "runtime record changed during preparation")
    for key, selected in runtime_pins.items():
        require(P.file_pin(selected["path"]) == {k: selected[k] for k in ("sha256", "byte_length")},
                "selected installation changed during preparation")
    target.mkdir()
    output = target / "selected-output"
    output.mkdir()
    for name, raw in source.items():
        destination = under(output, name)
        destination.parent.mkdir(parents=True, exist_ok=True)
        with destination.open("xb") as stream:
            stream.write(raw)
    require(files(output) == source, "prepared display bytes changed")
    P.save(target / "capture-spec.json", spec)
    with (target / "capture_visual.mjs").open("xb") as stream:
        stream.write(derived)
    require(fixture(root, args.fixture_sha256)[1] == originals, "frozen inputs changed after copying")
    require(P.file_pin(args.runtime_record)["sha256"] == args.runtime_sha256, "runtime record changed after copying")
    for key, selected in runtime_pins.items():
        require(P.file_pin(selected["path"]) == {k: selected[k] for k in ("sha256", "byte_length")},
                "selected installation changed after copying")
    if collector_pin:
        require(P.file_pin(collector_pin["path"]) == {k: collector_pin[k] for k in ("sha256", "byte_length")},
                "original collector changed during preparation")
    for selected in [preparation_pin, *preparation_dependencies]:
        require(P.file_pin(selected["path"]) == {k: selected[k] for k in ("sha256", "byte_length")},
                "preparation tool source changed during execution")
    receipt = {"schema_version": 1, "status": "prepared", "case_id": args.case_id, "theme": args.theme,
               "fixture_sha256": args.fixture_sha256, "runtime_sha256": args.runtime_sha256,
               "independent_readback": verified, "checker": checker_pin, "original_collector": collector_pin,
               "preparation_tool": preparation_pin, "preparation_dependencies": preparation_dependencies,
               "derived_collector": pin(derived), "runtime_inputs": runtime_pins,
               "source_pins": {name: pin(raw) for name, raw in source.items()},
               "capture_spec": pin(P.read(target / "capture-spec.json")), "source_unchanged": True,
               "output": str(output), "capture_spec_path": str(target / "capture-spec.json"),
               "capture_script_path": str(target / "capture_visual.mjs"), "native_capture_status": "not_run",
               "model_requests": 0, "processes_launched": 0, "scientific_quality": "not_run", "published": False}
    P.save(target / "prepared.json", receipt)
    return receipt


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    for key in ("fixture-root", "runtime-record", "new-root"):
        result.add_argument("--" + key, required=True, type=Path)
    for key in ("fixture-sha256", "runtime-sha256", "case-id"):
        result.add_argument("--" + key, required=True)
    result.add_argument("--theme", required=True, choices=("light", "dark"))
    return result


if __name__ == "__main__":
    try:
        print(json.dumps(prepare(parser().parse_args()), ensure_ascii=False))
    except (AssertionError, ValueError, KeyError, OSError) as error:
        print("visual compatibility preparation refused: " + str(error), file=sys.stderr)
        raise SystemExit(1)
