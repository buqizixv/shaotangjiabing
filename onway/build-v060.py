"""Reproducible native entry build; never rerun the retired 17-page generator."""
import json
from pathlib import Path

root = Path(__file__).resolve().parent
bundle = root / "bundle"
(bundle / "main.splash").write_text((root / "ui-v060.splash").read_text(encoding="utf-8"), encoding="utf-8")
manifest = json.loads((bundle / "manifest.json").read_text(encoding="utf-8"))
manifest.update(version="1.0.0", capabilities=["storage", "location", "glance"])
manifest["integrity"] = {"bundle_blake3": "", "signature": None}
(bundle / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print("Built Onway 1.0.0 native main entry. Run octo check to stamp.")
