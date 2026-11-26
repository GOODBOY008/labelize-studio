"""Expose the compiled Studio languages and permission prompt to macOS."""
import json
import plistlib
import sys
from pathlib import Path

contents = Path(sys.argv[1]) / "Contents"
plist_path = contents / "Info.plist"
info = plistlib.loads(plist_path.read_bytes())
locales = Path(__file__).resolve().parents[2] / "crates/desktop/locales"
tags = []
for path in sorted(locales.glob("*.json")):
    tag = path.stem
    tags.append(tag)
    message = json.loads(path.read_text(encoding="utf-8"))["messages"]["NativeDocumentsUsage"]
    if tag == "en":
        info["NSDocumentsFolderUsageDescription"] = message
    resource = contents / "Resources" / (tag + ".lproj") / "InfoPlist.strings"
    resource.parent.mkdir(parents=True, exist_ok=True)
    resource.write_bytes(plistlib.dumps({"NSDocumentsFolderUsageDescription": message}))

info["CFBundleDevelopmentRegion"] = "en"
info["CFBundleLocalizations"] = tags
plist_path.write_bytes(plistlib.dumps(info))
