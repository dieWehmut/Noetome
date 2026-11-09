"""Real Windows CLI + CurrentUser DPAPI + local fake HTTP, with no external model calls."""

from __future__ import annotations
import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import unittest

from tests.test_text_model_profile import BASELINE, PUBLIC_SECRET, SHELL, quote, winpath

ROOT = Path(__file__).resolve().parents[1]
EVIDENCE_ENV = "NOETOME_TEXT_NATIVE_CLI_EVIDENCE"
BINARY = Path(
    os.environ.get("NOETOME_TEST_NATIVE_CLI", str(ROOT / "target/debug/noetome.exe"))
)


@unittest.skipUnless(
    SHELL and BINARY.is_file(),
    "Current source native Windows CLI and DPAPI are unavailable",
)
class TextApiNativeCliTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        selected = (
            subprocess.check_output(
                [SHELL, "-NoProfile", "-Command", "[IO.Path]::GetTempPath()"],
                timeout=20,
            )
            .decode()
            .strip()
        )
        cls.windows_temp = (
            Path(selected)
            if os.name == "nt"
            else Path(
                subprocess.check_output(["wslpath", "-u", selected], text=True).strip()
            )
        )

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(
            prefix="noetome-real-text-cli-", dir=self.windows_temp
        )
        self.work = Path(self.directory.name)
        self.addCleanup(self.directory.cleanup)
        self.addCleanup(self.preserve)
        # Cargo may hardlink its bin to deps; copying gives the installed ordinary-file shape.
        self.binary = self.work / "noetome.exe"
        shutil.copyfile(BINARY, self.binary)
        for name in ("invoke_text_api_credential.ps1", "text_api_support.ps1"):
            shutil.copyfile(ROOT / "scripts" / name, self.work / name)
        self.credential = self.work / "public.dpapi"
        self.ledger = self.work / "usage.json"
        self.ledger.write_bytes(BASELINE.read_bytes())
        self.captures = 0
        result = self.ps(
            "$s=ConvertTo-SecureString -AsPlainText -Force "
            + quote(PUBLIC_SECRET)
            + ";try{[IO.File]::WriteAllText("
            + quote(winpath(self.credential))
            + ",(ConvertFrom-SecureString $s),[Text.Encoding]::ASCII)}finally{$s.Dispose()};exit 0"
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.project = self.work / "自己的资料 with spaces"
        self.source = self.work / "source.txt"
        self.source.write_bytes(
            (ROOT / "fixtures/extraction-gateway-v1/source.txt").read_bytes()
        )
        self.cli("project", "init", self.project, json_record=False)
        self.imported = self.cli("source", "import", self.project, self.source)
        self.selection = [{"kind": "text", "ordinal": 0}]
        selected = self.work / "selection.json"
        selected.write_text(json.dumps(self.selection), encoding="utf-8", newline="\n")
        self.units = self.cli(
            "preview", "units", self.project, self.imported["import_id"], selected
        )
        self.before = {
            name: (self.project / name).read_bytes()
            for name in ("project.json", "project.sqlite")
        }
        self.calls = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                if self.path != "/v1/chat/completions":
                    self.send_error(404)
                    return
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                outer.calls.append(
                    {
                        "body": body,
                        "public_credential_matches": self.headers.get("Authorization")
                        == "Bearer " + PUBLIC_SECRET,
                    }
                )
                payload = json.loads(body["messages"][1]["content"])["request"]
                candidates = json.loads(
                    (ROOT / "fixtures/extraction-gateway-v1/response.json").read_bytes()
                )
                candidates.pop("producer")
                candidates.update(
                    request_id=payload["request_id"], binding=payload["binding"]
                )
                data = json.dumps(
                    {
                        "id": "chatcmpl-noetome-test",
                        "object": "chat.completion",
                        "created": 1,
                        "model": "noetome-test-model",
                        "choices": [
                            {
                                "index": 0,
                                "finish_reason": "stop",
                                "message": {
                                    "role": "assistant",
                                    "content": json.dumps(
                                        candidates, ensure_ascii=False
                                    ),
                                    "reasoning_content": None,
                                },
                            }
                        ],
                        "usage": {
                            "prompt_tokens": 100,
                            "completion_tokens": 600,
                            "total_tokens": 700,
                            "prompt_tokens_details": {"cached_tokens": 0},
                            "completion_tokens_details": {"reasoning_tokens": 0},
                        },
                    },
                    ensure_ascii=False,
                ).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, *args):
                pass

        self.server = HTTPServer(("127.0.0.1", 0), Handler)
        self.server.timeout = 0.1
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.stop_server)
        self.config = self.work / "api.json"
        self.output = self.work / "saved API result"
        self.configuration = {
            "schema_version": 1,
            "provider": "openai_chat_completions",
            "transport_mode": "local_http_test",
            "endpoint": f"http://127.0.0.1:{self.server.server_port}/v1/chat/completions",
            "model": "noetome-test-model",
            "credential_env": "NOETOME_TEST_TEXT_CREDENTIAL",
            "required_input": "text",
            "capabilities": {"text": True, "vision": False, "structured_output": True},
            "selection": self.selection,
            "limits": {
                "max_units": 64,
                "max_input_bytes": 65536,
                "max_response_bytes": 1048576,
                "max_output_tokens": 8192,
                "timeout_ms": 5000,
            },
            "budget": {
                "max_requests": 1,
                "max_total_tokens": 65536,
                "max_cost_usd": None,
            },
        }

    def stop_server(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)

    def preserve(self):
        chosen = os.environ.get(EVIDENCE_ENV)
        if not chosen:
            return
        target = Path(chosen) / self._testMethodName
        target.mkdir(parents=True, exist_ok=False)
        for source in self.work.rglob("*"):
            if not source.is_file() or source.suffix.lower() in (".dpapi", ".exe"):
                continue
            data = source.read_bytes()
            self.assertNotIn(PUBLIC_SECRET.encode(), data)
            destination = target / source.relative_to(self.work)
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)

    def capture(self, args):
        env = os.environ.copy()
        for name in list(env):
            if name.upper() in (
                "PSMODULEPATH",
                "NOETOME_TANGMAO_API_KEY",
                "NOETOME_TEST_TEXT_CREDENTIAL",
            ):
                env.pop(name)
        result = subprocess.run(
            args, cwd=self.work, env=env, capture_output=True, timeout=35
        )
        self.assertNotIn(PUBLIC_SECRET.encode(), result.stdout)
        self.assertNotIn(PUBLIC_SECRET.encode(), result.stderr)
        (self.work / f"capture-{self.captures}.stdout.log").write_bytes(result.stdout)
        (self.work / f"capture-{self.captures}.stderr.log").write_bytes(result.stderr)
        (self.work / f"capture-{self.captures}.result.json").write_text(
            json.dumps({"exit_code": result.returncode}), encoding="utf-8", newline="\n"
        )
        self.captures += 1
        return subprocess.CompletedProcess(
            result.args,
            result.returncode,
            result.stdout.decode("utf-8", "strict"),
            result.stderr.decode("utf-8", "replace"),
        )

    def ps(self, code):
        return self.capture(
            [
                SHELL,
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                code,
            ]
        )

    def cli(self, *args, json_record=True):
        result = self.capture(
            [
                str(self.binary),
                *[winpath(arg) if isinstance(arg, Path) else arg for arg in args],
            ]
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout) if json_record else result.stdout

    def invoke(self):
        self.config.write_text(
            json.dumps(self.configuration), encoding="utf-8", newline="\n"
        )
        args = {
            "Binary": self.binary,
            "Project": self.project,
            "ImportId": self.imported["import_id"],
            "Config": self.config,
            "OutputRoot": self.output,
            "CredentialFile": self.credential,
            "UsageLedgerFile": self.ledger,
        }
        code = (
            "& "
            + quote(winpath(self.work / "invoke_text_api_credential.ps1"))
            + " "
            + " ".join(
                "-"
                + key
                + " "
                + quote(winpath(value) if isinstance(value, Path) else value)
                for key, value in args.items()
            )
            + ";exit $LASTEXITCODE"
        )
        return self.ps(code)

    def unchanged(self):
        for name, original in self.before.items():
            self.assertEqual(original, (self.project / name).read_bytes())
        self.assertEqual(
            BASELINE.read_bytes(),
            self.ledger.read_bytes(),
            "A local fake service must not consume live shared quota",
        )
        self.assertEqual(
            (ROOT / "fixtures/extraction-gateway-v1/source.txt").read_bytes(),
            self.source.read_bytes(),
        )

    def test_real_native_cli_dpapi_chat_and_preview_preserve_selected_source(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads(result.stdout)
        self.assertEqual(report["status"], "candidates_validated")
        self.assertEqual(report["usage"]["network_attempts"], 1)
        self.assertEqual(report["usage"]["model_usage"]["total_tokens"], 700)
        self.assertEqual(len(self.calls), 1)
        self.assertTrue(self.calls[0]["public_credential_matches"])
        sent = json.loads(self.calls[0]["body"]["messages"][1]["content"])["request"]
        self.assertEqual(sent["units"], self.units["units"])
        self.assertEqual(sent["binding"], self.units["binding"])
        saved = json.loads((self.output / "request.json").read_bytes())
        self.assertEqual(saved["units"], self.units["units"])
        self.assertEqual(saved["binding"], self.units["binding"])
        destination = self.work / "Obsidian 待核验"
        exported = self.cli(
            "preview",
            "export",
            self.project,
            self.imported["import_id"],
            self.output,
            destination,
        )
        self.assertEqual(exported["producer"], "extraction_preview")
        vault = destination / "vault"
        self.assertEqual(
            (vault / "attachments/source-0001.txt").read_bytes(),
            self.source.read_bytes(),
        )
        note = (vault / "待核验笔记.md").read_text(encoding="utf-8")
        self.assertIn("pending", note)
        self.assertIn("verification：unverified", note)
        self.assertIn("scientific_quality：not_run", note)
        self.assertIn("m/s", note)
        self.assertIn("20 ℃", note)
        self.unchanged()
        receipt = {
            "scope": "real_native_cli_dpapi_local_fake_http",
            "external_model_requests": 0,
            "local_http_requests": len(self.calls),
            "public_dpapi_matches": True,
            "model_usage": {"fake_declared_total_tokens": 700},
            "binary_sha256": hashlib.sha256(self.binary.read_bytes()).hexdigest(),
            "confirmed_units_equal_request": True,
            "shared_live_ledger_unchanged": True,
            "export": exported,
        }
        (self.work / "native-chain-receipt.json").write_text(
            json.dumps(receipt, ensure_ascii=False, indent=2),
            encoding="utf-8",
            newline="\n",
        )

    def test_real_native_no_request_budget_stops_before_loopback_http(self):
        self.configuration["budget"]["max_requests"] = 0
        result = self.invoke()
        self.assertNotEqual(result.returncode, 0)
        report = json.loads(result.stdout)
        self.assertEqual(report["usage"]["network_attempts"], 0)
        self.assertEqual(self.calls, [])
        self.unchanged()
