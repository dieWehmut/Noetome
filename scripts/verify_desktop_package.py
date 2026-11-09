#!/usr/bin/env python3
"""Independent read-only audit of the Noetome desktop packaging configuration.

This auditor never imports the Rust implementation and never runs `tauri build`.
It reads the frozen packaging surface and re-derives the expectations that the
install-time acceptance checklist depends on:

* `crates/noetome-desktop/tauri.conf.json` identifiers, window, CSP and bundle
  values (targets, install mode, installer languages, icon list);
* the icon files, their byte digests and the authored `icons/manifest.json`;
* the capability manifest (no file/network/process plugin permission);
* `package.json` scripts, the pinned front-end build output and its shape.

Usage:
    python3 scripts/verify_desktop_package.py [--json] [--freeze]
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
from pathlib import Path

CONFIG = "crates/noetome-desktop/tauri.conf.json"
CAPABILITIES = "crates/noetome-desktop/capabilities/main.json"
ICON_MANIFEST = "crates/noetome-desktop/icons/manifest.json"
ICON_FILES = (
    "crates/noetome-desktop/icons/32x32.png",
    "crates/noetome-desktop/icons/128x128.png",
    "crates/noetome-desktop/icons/128x128@2x.png",
    "crates/noetome-desktop/icons/icon.ico",
    "apps/desktop/public/app-icon.png",
)
DIST = "apps/desktop/dist"
DIST_INDEX = "apps/desktop/dist/index.html"
PACKAGE_JSON = "package.json"
ACCEPTANCE = "doc/standards/DESKTOP_ACCEPTANCE.md"
FROZEN = "crates/noetome-desktop/packaging-frozen.json"
INSTALL_CONFIG = "crates/noetome-desktop/tauri.install.conf.json"
INSTALLER_SURFACE = (
    INSTALL_CONFIG,
    "scripts/prepare_windows_installer_payload.ps1",
    "scripts/record_windows_webview_installer.ps1",
    "doc/technical/WINDOWS_INSTALLER_CONTRACT.md",
    "doc/windows/QUICKSTART.zh-CN.md",
)

CONFIG_KEYS = {
    "$schema",
    "productName",
    "version",
    "identifier",
    "build",
    "app",
    "bundle",
}
BUILD_KEYS = {"beforeDevCommand", "beforeBuildCommand", "devUrl", "frontendDist"}
APP_KEYS = {"windows", "security"}
WINDOW_KEYS = {
    "label",
    "title",
    "width",
    "height",
    "minWidth",
    "minHeight",
    "center",
    "decorations",
}
SECURITY_KEYS = {"csp"}
BUNDLE_KEYS = {"active", "targets", "shortDescription", "longDescription", "icon", "windows"}
NSIS_KEYS = {
    "installMode",
    "installerIcon",
    "uninstallerIcon",
    "languages",
    "displayLanguageSelector",
}

# The frozen packaging surface. Any change here must be deliberate and recorded
# in the validation note; the audit reports a drift instead of silently passing.
EXPECTED_PRODUCT_NAME = "Noetome"
EXPECTED_IDENTIFIER = "com.noetome.desktop"
EXPECTED_TARGETS = ["nsis"]
EXPECTED_INSTALL_MODE = "currentUser"
EXPECTED_LANGUAGES = ["SimpChinese", "English"]
EXPECTED_LANGUAGE_SELECTOR = False
EXPECTED_ICON_LIST = [
    "icons/32x32.png",
    "icons/128x128.png",
    "icons/128x128@2x.png",
    "icons/icon.ico",
]
EXPECTED_WINDOW_LABEL = "main"
EXPECTED_WINDOW_MIN = (640, 600)
EXPECTED_DECORATIONS = False
EXPECTED_SHELL_COMMANDS = (
    "runtime_info", "cli_preview", "pick_directory", "project_create", "project_open",
    "task_read", "task_kinds", "get_close_behavior", "set_close_behavior",
    "project_create_in_parent", "source_pick", "source_import", "source_choices",
    "source_units", "source_extract", "source_cancel", "source_preview_export",
    "model_status", "model_credential_store", "model_budget_restore", "desktop_self_test",
)
MAIN_RS = "crates/noetome-desktop/src/main.rs"

FORBIDDEN_PERMISSIONS = (
    "dialog:",
    "fs:",
    "http:",
    "shell:",
    "process:",
    "upload:",
    "download:",
    "websocket:",
    "os:",
)


class Report:
    def __init__(self) -> None:
        self.items: list[dict] = []

    def record(self, name: str, ok: bool, detail: str, warn: bool = False) -> None:
        self.items.append({"name": name, "ok": ok, "detail": detail, "warn": warn})

    def ok(self, name: str, detail: str = "") -> None:
        self.record(name, True, detail)

    def fail(self, name: str, detail: str) -> None:
        self.record(name, False, detail)

    def warn(self, name: str, detail: str) -> None:
        self.record(name, False, detail, warn=True)

    @property
    def failures(self) -> list[dict]:
        return [item for item in self.items if not item["ok"] and not item["warn"]]

    @property
    def warnings(self) -> list[dict]:
        return [item for item in self.items if item["warn"]]

    def dump(self) -> dict:
        return {
            "total": len(self.items),
            "failures": len(self.failures),
            "warnings": len(self.warnings),
            "items": self.items,
        }


def read_bytes(path: Path) -> bytes:
    return path.read_bytes()


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_json(path: Path, label: str, report: Report) -> dict | None:
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError as error:
        report.fail(f"{label}-read", f"无法读取 {path}：{error}")
        return None
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        report.fail(f"{label}-json", f"{path} 不是严格 JSON：{error}")
        return None
    if not isinstance(value, dict):
        report.fail(f"{label}-shape", f"{path} 顶层必须是对象")
        return None
    return value


def check_keys(name: str, value: dict, expected: set[str], report: Report) -> None:
    actual = set(value)
    if actual == expected:
        report.ok(f"{name}-keys", "字段集合与冻结预期一致")
        return
    missing = sorted(expected - actual)
    extra = sorted(actual - expected)
    report.fail(f"{name}-keys", f"字段集合不符：缺少 {missing}、多出 {extra}")


def check_config(root: Path, report: Report) -> dict | None:
    config = load_json(root / CONFIG, "config", report)
    if config is None:
        return None
    check_keys("config", config, CONFIG_KEYS, report)
    if config.get("productName") == EXPECTED_PRODUCT_NAME:
        report.ok("config-product-name", EXPECTED_PRODUCT_NAME)
    else:
        report.fail("config-product-name", f"productName 应为 {EXPECTED_PRODUCT_NAME}，实际 {config.get('productName')!r}")
    if config.get("identifier") == EXPECTED_IDENTIFIER:
        report.ok("config-identifier", EXPECTED_IDENTIFIER)
    else:
        report.fail("config-identifier", f"identifier 应为 {EXPECTED_IDENTIFIER}，实际 {config.get('identifier')!r}")
    version = config.get("version")
    if isinstance(version, str) and version.count(".") == 2 and all(part.isdigit() for part in version.split(".")):
        report.ok("config-version", version)
    else:
        report.fail("config-version", f"version 必须是三段数字版本，实际 {version!r}")

    build = config.get("build")
    if not isinstance(build, dict):
        report.fail("build-shape", "build 必须是对象")
    else:
        check_keys("build", build, BUILD_KEYS, report)
        if build.get("frontendDist") == "../../apps/desktop/dist":
            report.ok("build-frontend-dist", build["frontendDist"])
        else:
            report.fail("build-frontend-dist", f"frontendDist 应指向 apps/desktop/dist，实际 {build.get('frontendDist')!r}")
        if build.get("beforeBuildCommand") == "npm run web:build":
            report.ok("build-before-build", build["beforeBuildCommand"])
        else:
            report.fail("build-before-build", f"beforeBuildCommand 应为 npm run web:build，实际 {build.get('beforeBuildCommand')!r}")

    app = config.get("app")
    if not isinstance(app, dict):
        report.fail("app-shape", "app 必须是对象")
    else:
        check_keys("app", app, APP_KEYS, report)
        windows = app.get("windows")
        if not isinstance(windows, list) or len(windows) != 1:
            report.fail("app-windows", "必须恰好声明一个窗口")
        else:
            window = windows[0]
            if isinstance(window, dict):
                check_keys("window", window, WINDOW_KEYS, report)
                if window.get("label") == EXPECTED_WINDOW_LABEL:
                    report.ok("window-label", EXPECTED_WINDOW_LABEL)
                else:
                    report.fail("window-label", f"窗口 label 应为 {EXPECTED_WINDOW_LABEL}，实际 {window.get('label')!r}")
                minimum = (window.get("minWidth"), window.get("minHeight"))
                if minimum == EXPECTED_WINDOW_MIN:
                    report.ok("window-minimum", f"{minimum[0]}x{minimum[1]}")
                else:
                    report.fail("window-minimum", f"最小尺寸应为 {EXPECTED_WINDOW_MIN}，实际 {minimum}")
                if window.get("decorations") is EXPECTED_DECORATIONS:
                    report.ok("window-decorations", "无系统标题栏（由前端自绘）")
                else:
                    report.fail("window-decorations", f"decorations 应为 {EXPECTED_DECORATIONS}")
            else:
                report.fail("window-shape", "窗口声明必须是对象")
        security = app.get("security")
        if not isinstance(security, dict):
            report.fail("security-shape", "security 必须是对象")
        else:
            check_keys("security", security, SECURITY_KEYS, report)
            csp = security.get("csp")
            if isinstance(csp, str) and "default-src 'self'" in csp:
                report.ok("security-csp-default", "CSP 限制 default-src 为 'self'")
            else:
                report.fail("security-csp-default", f"CSP 缺少 default-src 'self'：{csp!r}")
            if isinstance(csp, str) and "connect-src ipc:" in csp:
                report.ok("security-csp-ipc", "CSP 只开放 ipc 通道")
            else:
                report.fail("security-csp-ipc", f"CSP 未限制 connect-src 到 ipc：{csp!r}")
            remote = csp.replace("http://ipc.localhost", "") if isinstance(csp, str) else ""
            if "http://" in remote or "https://" in remote:
                report.fail("security-csp-remote", f"CSP 仍包含远程 http(s) 来源：{csp!r}")
            else:
                report.ok("security-csp-remote", "CSP 不含远程 http(s) 来源")

    bundle = config.get("bundle")
    if not isinstance(bundle, dict):
        report.fail("bundle-shape", "bundle 必须是对象")
        return config
    check_keys("bundle", bundle, BUNDLE_KEYS, report)
    if bundle.get("active") is True:
        report.ok("bundle-active", "bundle 已启用")
    else:
        report.fail("bundle-active", "bundle.active 必须为 true，否则不会产出安装包")
    if bundle.get("targets") == EXPECTED_TARGETS:
        report.ok("bundle-targets", "targets 为 nsis")
    else:
        report.fail("bundle-targets", f"targets 应为 {EXPECTED_TARGETS}，实际 {bundle.get('targets')!r}")
    if bundle.get("icon") == EXPECTED_ICON_LIST:
        report.ok("bundle-icons", f"{len(EXPECTED_ICON_LIST)} 个图标条目")
    else:
        report.fail("bundle-icons", f"icon 列表应为 {EXPECTED_ICON_LIST}，实际 {bundle.get('icon')!r}")
    windows_bundle = bundle.get("windows")
    if not isinstance(windows_bundle, dict) or not isinstance(windows_bundle.get("nsis"), dict):
        report.fail("bundle-nsis-shape", "bundle.windows.nsis 必须是对象")
        return config
    nsis = windows_bundle["nsis"]
    check_keys("nsis", nsis, NSIS_KEYS, report)
    if nsis.get("installMode") == EXPECTED_INSTALL_MODE:
        report.ok("nsis-install-mode", EXPECTED_INSTALL_MODE)
    else:
        report.fail("nsis-install-mode", f"installMode 应为 {EXPECTED_INSTALL_MODE}，实际 {nsis.get('installMode')!r}")
    if nsis.get("languages") == EXPECTED_LANGUAGES:
        report.ok("nsis-languages", "、".join(EXPECTED_LANGUAGES))
    else:
        report.fail("nsis-languages", f"languages 应为 {EXPECTED_LANGUAGES}，实际 {nsis.get('languages')!r}")
    if nsis.get("displayLanguageSelector") is EXPECTED_LANGUAGE_SELECTOR:
        report.ok("nsis-language-selector", "不显示语言选择器（按安装器语言自动选择）")
    else:
        report.fail("nsis-language-selector", f"displayLanguageSelector 应为 {EXPECTED_LANGUAGE_SELECTOR}")
    for key in ("installerIcon", "uninstallerIcon"):
        if nsis.get(key) == "icons/icon.ico":
            report.ok(f"nsis-{key}", "icons/icon.ico")
        else:
            report.fail(f"nsis-{key}", f"{key} 应为 icons/icon.ico，实际 {nsis.get(key)!r}")
    return config


def check_icons(root: Path, report: Report) -> None:
    for relative in ICON_FILES:
        path = root / relative
        if not path.is_file():
            report.fail("icon-file", f"缺少图标文件 {relative}")
            continue
        data = read_bytes(path)
        if len(data) == 0:
            report.fail("icon-empty", f"{relative} 是空文件")
            continue
        report.ok("icon-file", f"{relative}（{len(data)} 字节，{sha256(data)[:16]}）")
    manifest = load_json(root / ICON_MANIFEST, "icon-manifest", report)
    if manifest is None:
        return
    assets = manifest.get("assets")
    if not isinstance(assets, list) or not assets:
        report.fail("icon-manifest-assets", "manifest.assets 必须是非空数组")
        return
    recorded = {}
    for item in assets:
        if not isinstance(item, dict) or "path" not in item or "sha256" not in item:
            report.fail("icon-manifest-entry", f"条目缺少 path/sha256：{item!r}")
            continue
        recorded[item["path"]] = item["sha256"]
    expected = set(ICON_FILES)
    if set(recorded) == expected:
        report.ok("icon-manifest-paths", f"{len(expected)} 个资产路径与冻结清单一致")
    else:
        report.fail("icon-manifest-paths", f"路径集合不符：缺少 {sorted(expected - set(recorded))}、多出 {sorted(set(recorded) - expected)}")
    for relative, digest in sorted(recorded.items()):
        path = root / relative
        if not path.is_file():
            continue
        actual = sha256(read_bytes(path))
        if actual == digest:
            report.ok("icon-digest", f"{relative} 摘要一致")
        else:
            report.fail("icon-digest", f"{relative} 摘要不符：清单 {digest}，实际 {actual}")
    source_digest = manifest.get("source_sha256")
    if isinstance(source_digest, str) and len(source_digest) == 64:
        report.ok("icon-source-digest", "记录了图标源文件摘要（源文件本身不入库）")
    else:
        report.fail("icon-source-digest", "manifest.source_sha256 必须是 64 位十六进制")


def check_capabilities(root: Path, report: Report) -> None:
    capabilities = load_json(root / CAPABILITIES, "capabilities", report)
    if capabilities is None:
        return
    windows = capabilities.get("windows")
    if windows == [EXPECTED_WINDOW_LABEL]:
        report.ok("capabilities-windows", "只对主窗口生效")
    else:
        report.fail("capabilities-windows", f"windows 应为 [\"main\"]，实际 {windows!r}")
    permissions = capabilities.get("permissions")
    if not isinstance(permissions, list) or not permissions:
        report.fail("capabilities-permissions", "permissions 必须是非空数组")
        return
    report.ok("capabilities-permissions", f"{len(permissions)} 条权限")
    offending = [item for item in permissions if isinstance(item, str) and item.startswith(FORBIDDEN_PERMISSIONS)]
    if offending:
        report.fail("capabilities-forbidden", f"不允许的文件/网络/进程插件权限：{offending}")
    else:
        report.ok("capabilities-forbidden", "无文件、网络或进程插件权限")
    # Every permission must belong to the core window/event namespace.
    unknown = [
        item
        for item in permissions
        if not (isinstance(item, str) and (item.startswith("core:window:") or item.startswith("core:event:")))
    ]
    if unknown:
        report.fail("capabilities-namespace", f"只允许 core:window / core:event 权限，发现 {unknown}")
    else:
        report.ok("capabilities-namespace", "全部权限位于 core:window / core:event")


def check_package_json(root: Path, report: Report) -> None:
    package = load_json(root / PACKAGE_JSON, "package", report)
    if package is None:
        return
    scripts = package.get("scripts")
    if not isinstance(scripts, dict):
        report.fail("package-scripts", "scripts 必须是对象")
        return
    expected = {
        "web:check": "tsc --project apps/desktop/tsconfig.json --noEmit",
        "desktop:build": "tauri build --config crates/noetome-desktop/tauri.conf.json",
    }
    for key, value in expected.items():
        if scripts.get(key) == value:
            report.ok(f"package-script-{key}", value)
        else:
            report.fail(f"package-script-{key}", f"{key} 应为 {value!r}，实际 {scripts.get(key)!r}")
    engines = package.get("engines")
    if isinstance(engines, dict) and isinstance(engines.get("node"), str):
        report.ok("package-engines-node", engines["node"])
    else:
        report.fail("package-engines-node", "engines.node 必须声明最低 Node 版本")


def check_dist(root: Path, report: Report) -> None:
    index = root / DIST_INDEX
    if not index.is_file():
        report.warn("dist-index", f"未找到 {DIST_INDEX}；先运行 npm run web:build 再审计")
        return
    html = index.read_text(encoding="utf-8")
    if 'src="/assets/' in html and 'href="/assets/' in html:
        report.ok("dist-index-assets", "入口引用了构建产物")
    else:
        report.fail("dist-index-assets", "index.html 未引用 /assets/ 下的构建产物")
    assets = root / DIST / "assets"
    if not assets.is_dir():
        report.fail("dist-assets", f"缺少 {DIST}/assets")
        return
    files = sorted(item.name for item in assets.iterdir() if item.is_file())
    if len(files) >= 2 and any(name.endswith(".js") for name in files) and any(name.endswith(".css") for name in files):
        report.ok("dist-assets", "、".join(files))
    else:
        report.fail("dist-assets", f"构建产物不足（需要至少一个 js 与一个 css）：{files}")
    for reference in _asset_references(html):
        if not (root / DIST / reference.lstrip("/")).is_file():
            report.fail("dist-reference", f"index.html 引用的 {reference} 不存在")


def _asset_references(html: str) -> list[str]:
    references = []
    for token in ('src="/', 'href="/'):
        start = 0
        while True:
            index = html.find(token, start)
            if index < 0:
                break
            value_start = index + len(token) - 1
            value_end = html.find('"', value_start + 1)
            if value_end < 0:
                break
            references.append(html[value_start:value_end])
            start = value_end
    return [item for item in references if "/assets/" in item]


def check_main_rs(root: Path, report: Report) -> None:
    path = root / MAIN_RS
    if not path.is_file():
        report.fail("main-rs", f"缺少 {MAIN_RS}")
        return
    text = path.read_text(encoding="utf-8")
    for command in EXPECTED_SHELL_COMMANDS:
        if f"fn {command}(" in text:
            report.ok("main-rs-command", command)
        else:
            report.fail("main-rs-command", f"缺少命令 {command}")
    handler = re.search(r"generate_handler!\s*\[([^\]]*)\]", text)
    if handler:
        registered = {entry.strip() for entry in handler.group(1).split(",") if entry.strip()}
        expected = set(EXPECTED_SHELL_COMMANDS)
        if registered == expected:
            report.ok("main-rs-handler", f"完整注册 {len(expected)} 条受限应用命令")
        else:
            report.fail("main-rs-handler", f"未注册 {sorted(expected - registered)}；未声明 {sorted(registered - expected)}")
    else:
        report.fail("main-rs-handler", "缺少 generate_handler 注册")
    if "fs::write" in text and "preferences" not in text:
        report.warn("main-rs-write", "main.rs 出现 fs::write，请确认不是新增写路径")


def check_acceptance(root: Path, report: Report) -> None:
    path = root / ACCEPTANCE
    if not path.is_file():
        report.fail("acceptance-file", f"缺少安装后验收清单 {ACCEPTANCE}")
        return
    text = path.read_text(encoding="utf-8")
    required_sections = [
        "安装与首次启动",
        "窗口与导航",
        "Rust 组件通信",
        "项目入口",
        "资料与模型笔记",
        "任务中心",
        "关闭行为、托盘与重启",
        "卸载",
        "判定规则",
    ]
    for section in required_sections:
        if section in text:
            report.ok("acceptance-section", section)
        else:
            report.fail("acceptance-section", f"清单缺少章节「{section}」")
    if "静态预览" in text and "不能替代" in text:
        report.ok("acceptance-disclaimer", "明确声明静态检查不能替代安装后测试")
    else:
        report.fail("acceptance-disclaimer", "缺少「静态检查不能替代安装后测试」的声明")


def audit(root: Path) -> Report:
    report = Report()
    if not (root / CONFIG).is_file():
        report.fail("config-present", f"未找到 {CONFIG}")
    check_config(root, report)
    check_icons(root, report)
    check_capabilities(root, report)
    check_package_json(root, report)
    check_dist(root, report)
    check_main_rs(root, report)
    check_acceptance(root, report)
    check_installer_overlay(root, report)
    check_frozen(root, report)
    return report


def check_installer_overlay(root: Path, report: Report) -> None:
    overlay = load_json(root / INSTALL_CONFIG, "installer-config", report)
    if overlay is None:
        return
    if not isinstance(overlay, dict) or not isinstance(overlay.get("bundle"), dict):
        report.fail("installer-shape", "安装覆盖配置和 bundle 必须是对象")
        return
    if set(overlay) - {"$schema", "bundle"}:
        report.fail("installer-scope", "安装覆盖配置只能调整 bundle")
    else:
        report.ok("installer-scope", "显式安装配置，不修改窗口、标识或权限")
    bundle = overlay.get("bundle", {})
    if bundle.get("resources") == {"../../target/windows-installer-payload/": ""}:
        report.ok("installer-resource-layout", "目录映射保留完整运行时层级")
    else:
        report.fail("installer-resource-layout", "完整payload必须用目录映射，不能用会扁平化的glob")
    windows = bundle.get("windows")
    if isinstance(windows, dict) and windows.get("webviewInstallMode") == {"type": "offlineInstaller", "silent": True}:
        report.ok("installer-webview", "随包离线WebView2安装器")
    else:
        report.fail("installer-webview", "安装版须内置离线WebView2安装器")
    if (root / "crates/noetome-desktop/tauri.windows.conf.json").exists():
        report.fail("installer-opt-in", "安装资源覆盖不应自动影响普通Rust与无bundle构建")
    else:
        report.ok("installer-opt-in", "通过显式 --config 启用完整安装资源")
    for relative in INSTALLER_SURFACE[1:]:
        if (root / relative).is_file():
            report.ok("installer-source-present", relative)
        else:
            report.fail("installer-source-present", f"缺少 {relative}")


def check_frozen(root: Path, report: Report) -> None:
    path = root / FROZEN
    if not path.is_file():
        report.warn("frozen-manifest", f"未找到 {FROZEN}；先运行 --freeze 生成冻结清单")
        return
    frozen = load_json(path, "frozen", report)
    if frozen is None:
        return
    files = frozen.get("files")
    if not isinstance(files, dict) or not files:
        report.fail("frozen-files", "files 必须是非空对象")
        return
    for relative, expected in sorted(files.items()):
        target = root / relative
        if not target.is_file():
            report.fail("frozen-missing", f"{relative} 已不在工作区")
            continue
        actual = sha256(read_bytes(target))
        if actual == expected.get("sha256"):
            report.ok("frozen-digest", relative)
        else:
            report.fail("frozen-digest", f"{relative} 与冻结摘要不符：冻结 {expected.get('sha256')}，实际 {actual}")


def freeze(root: Path) -> None:
    config = json.loads((root / CONFIG).read_text(encoding="utf-8"))
    version = config.get("version", "0.0.0")
    files: dict[str, dict] = {}
    for relative in (CONFIG, CAPABILITIES, ICON_MANIFEST, *ICON_FILES, PACKAGE_JSON, ACCEPTANCE, *INSTALLER_SURFACE):
        path = root / relative
        if not path.is_file():
            raise SystemExit(f"冻结失败：缺少 {relative}")
        data = read_bytes(path)
        files[relative] = {"sha256": sha256(data), "byte_length": len(data)}
    payload = {
        "schema_version": 1,
        "producer": "independent_packaging_freeze",
        "workflow_id": "noetome-desktop-personal-edition",
        "version": version,
        "files": files,
    }
    target = root / FROZEN
    target.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(f"已冻结 {len(files)} 个成员（version={version}）")


def main(argv: list[str]) -> int:
    if argv and argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    as_json = False
    do_freeze = False
    positionals: list[str] = []
    for argument in argv:
        if argument == "--json":
            as_json = True
        elif argument == "--freeze":
            do_freeze = True
        elif argument.startswith("-"):
            print(f"未知参数：{argument}", file=sys.stderr)
            return 2
        else:
            positionals.append(argument)
    root = Path(positionals[0]) if positionals else Path(".")
    if len(positionals) > 1:
        print("最多接受一个仓库根参数", file=sys.stderr)
        return 2
    if not root.is_dir():
        print(f"仓库根不存在：{root}", file=sys.stderr)
        return 2
    if do_freeze:
        freeze(root)
        return 0
    report = audit(root)
    if as_json:
        print(json.dumps(report.dump(), ensure_ascii=False, indent=2))
    else:
        for item in report.items:
            if not item["ok"]:
                print(f"[{'提示' if item['warn'] else '失败'}] {item['name']}：{item['detail']}")
        print(f"合计 {len(report.items)} 项检查：失败 {len(report.failures)}、提示 {len(report.warnings)}")
    return 1 if report.failures else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
