#!/usr/bin/env python3
"""一次性生成 B07-T03 作者包的字面内容。

本脚本是作者原件的可复现来源：它只写本包内声明的基线、候选、陌生文件与 expected，
不访问网络、模型、凭据或用户 Vault。生成的字节随后由 frozen-sha256.json 固定；
脚本自身也进入清单，任何后续改动都会改变包指纹。
"""

from __future__ import annotations

import hashlib
import json
import pathlib
import struct
import zlib

ROOT = pathlib.Path(__file__).resolve().parent.parent

NOTE_A = (
    "# 力学\n\n小球在斜面上下滑。受力分解沿用上次课堂的写法。\n\n"
    "速度和加速度的关系仍按教材第 4 章。\n\n"
    "相关阅读：[光学](光学.md)\n"
)
NOTE_A_EDITED = (
    "# 力学\n\n小球在斜面上下滑。受力分解改用正交坐标，删去上次课堂的写法。\n\n"
    "速度和加速度的关系仍按教材第 4 章。\n\n"
    "相关阅读：[光学](光学.md)\n"
)
NOTE_A_UNLINKED = (
    "# 力学\n\n小球在斜面上下滑。受力分解沿用上次课堂的写法。\n\n"
    "速度和加速度的关系仍按教材第 4 章。\n"
)
NOTE_A_RELINKED = (
    "# 力学\n\n小球在斜面上下滑。受力分解沿用上次课堂的写法。\n\n"
    "速度和加速度的关系仍按教材第 4 章。\n\n"
    "相关阅读：[几何光学](几何光学.md)\n"
)
NOTE_B = "# 光学\n\n凸透镜成像公式在本节给出。\n"
NOTE_B_LINKED = "# 光学\n\n凸透镜成像公式在本节给出。\n\n相关阅读：[力学](力学.md)\n"
NOTE_B_RENAMED = "# 几何光学\n\n凸透镜成像公式在本节给出。\n"
FOREIGN_NOTE = "# 陌生笔记\n\n这份内容没有托管基线，任何候选都不能改写它。\n"
OBSIDIAN_APP = "{\n  \"alwaysUpdateLinks\": true\n}\n"


def png(width: int, height: int, rgb: tuple[int, int, int]) -> bytes:
    raw = b"".join(b"\x00" + bytes(rgb) * width for _ in range(height))

    def chunk(tag: bytes, payload: bytes) -> bytes:
        return (
            struct.pack(">I", len(payload))
            + tag
            + payload
            + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF)
        )

    header = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


PNG_BASELINE = png(4, 3, (16, 96, 176))
PNG_CANDIDATE = png(4, 3, (176, 48, 32))

FILES: dict[str, bytes] = {
    "baseline/笔记/力学.md": NOTE_A.encode("utf-8"),
    "baseline/笔记/光学.md": NOTE_B.encode("utf-8"),
    "baseline/附件/光谱.png": PNG_BASELINE,
    "candidates/edit-body/笔记/力学.md": NOTE_A_EDITED.encode("utf-8"),
    "candidates/add-link/笔记/光学.md": NOTE_B_LINKED.encode("utf-8"),
    "candidates/replace-attachment/附件/光谱.png": PNG_CANDIDATE,
    "candidates/rename-dangling/笔记/几何光学.md": NOTE_B_RENAMED.encode("utf-8"),
    "candidates/rename-linked/笔记/几何光学.md": NOTE_B_RENAMED.encode("utf-8"),
    "candidates/rename-linked/笔记/力学.md": NOTE_A_RELINKED.encode("utf-8"),
    "candidates/edit-after-approval/笔记/力学.md": NOTE_A_EDITED.encode("utf-8"),
    "candidates/delete-note/笔记/力学.md": NOTE_A_UNLINKED.encode("utf-8"),
    "candidates/foreign-touch/笔记/陌生.md": b"# replaced\n",
    "foreign/笔记/陌生.md": FOREIGN_NOTE.encode("utf-8"),
    "foreign/附件/无关.png": png(2, 2, (200, 200, 200)),
    "foreign/.obsidian/app.json": OBSIDIAN_APP.encode("utf-8"),
    "foreign/.obsidian/appearance.json": b"{\n  \"theme\": \"dark\"\n}\n",
    "originals/notes.md.txt": (
        "笔记甲（基线）：\n"
        + NOTE_A
        + "\n笔记甲（候选 edit-body / edit-after-approval）：\n"
        + NOTE_A_EDITED
        + "\n笔记甲（候选 delete-note）：\n"
        + NOTE_A_UNLINKED
        + "\n笔记甲（候选 rename-linked）：\n"
        + NOTE_A_RELINKED
        + "\n笔记乙（基线）：\n"
        + NOTE_B
        + "\n笔记乙（候选 add-link）：\n"
        + NOTE_B_LINKED
        + "\n笔记乙（候选 rename-dangling / rename-linked）：\n"
        + NOTE_B_RENAMED
        + "\n陌生笔记：\n"
        + FOREIGN_NOTE
    ).encode("utf-8"),
    "originals/obsidian-app.json.txt": OBSIDIAN_APP.encode("utf-8"),
}


def main() -> None:
    written = []
    for relative, payload in sorted(FILES.items()):
        target = ROOT / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(payload)
        written.append(
            {
                "path": relative,
                "sha256": hashlib.sha256(payload).hexdigest(),
                "byte_length": len(payload),
            }
        )
    (ROOT / "originals" / "generated-manifest.json").write_text(
        json.dumps(written, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    print(f"wrote {len(written)} files")


if __name__ == "__main__":
    main()
