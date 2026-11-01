"""Generate committed desktop icon assets from the explicitly chosen local image.

Requires Pillow for this authoring step only. Builds use the generated assets and
do not read sample/ or require Python/Pillow.
"""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", nargs="?", type=Path, default=ROOT / "sample/icon.png")
    args = parser.parse_args()
    raw = args.source.read_bytes()
    with Image.open(args.source) as image:
        if image.width != image.height:
            parser.error("应用图标必须是正方形；请提供正方形 PNG。")
        source_size = image.size
        image = image.convert("RGBA")
        icons = ROOT / "crates/noetome-desktop/icons"
        public = ROOT / "apps/desktop/public"
        icons.mkdir(parents=True, exist_ok=True)
        public.mkdir(parents=True, exist_ok=True)
        outputs = []
        for name, size in [("32x32.png", 32), ("128x128.png", 128), ("128x128@2x.png", 256)]:
            path = icons / name
            image.resize((size, size), Image.Resampling.LANCZOS).save(path)
            outputs.append(path)
        ico = icons / "icon.ico"
        image.resize((256, 256), Image.Resampling.LANCZOS).save(
            ico, format="ICO", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]
        )
        outputs.append(ico)
        frontend = public / "app-icon.png"
        image.resize((256, 256), Image.Resampling.LANCZOS).save(frontend)
        outputs.append(frontend)
    manifest = {
        "source": "user-selected icon.png",
        "source_sha256": hashlib.sha256(raw).hexdigest(),
        "source_dimensions": list(source_size),
        "conversion": "RGBA, preserve square composition, Pillow LANCZOS",
        "pillow_version": Image.__version__,
        "assets": [
            {"path": p.relative_to(ROOT).as_posix(), "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
            for p in outputs
        ],
    }
    (icons / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Generated {len(outputs)} assets; source SHA256: {manifest['source_sha256']}")


if __name__ == "__main__":
    main()
