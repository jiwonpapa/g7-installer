#!/usr/bin/env python3
"""Create a deterministic CycloneDX inventory from Cargo metadata."""

import json
import pathlib
import sys


def main() -> None:
    source = pathlib.Path(sys.argv[1])
    target = pathlib.Path(sys.argv[2])
    metadata = json.loads(source.read_text(encoding="utf-8"))
    components = []
    for package in sorted(metadata["packages"], key=lambda item: (item["name"], item["version"])):
        component = {
            "type": "library",
            "bom-ref": package["id"],
            "name": package["name"],
            "version": package["version"],
            "purl": f"pkg:cargo/{package['name']}@{package['version']}",
        }
        if package.get("license"):
            component["licenses"] = [{"expression": package["license"]}]
        components.append(component)

    document = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "version": 1,
        "metadata": {"component": {"type": "application", "name": "g7-installer"}},
        "components": components,
    }
    target.write_text(
        json.dumps(document, ensure_ascii=True, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


if __name__ == "__main__":
    main()
