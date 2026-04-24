#!/usr/bin/env python3

from __future__ import annotations

import json
from datetime import datetime
from pathlib import Path
from tempfile import NamedTemporaryFile
from typing import Any

REPORTS_INDEX_NAME = "reports-index.json"
REPORTS_ROOT = Path("/reports/mach-6")
REPORTS_INDEX_PATH = REPORTS_ROOT / REPORTS_INDEX_NAME
BASE_URL = "/reports/mach-6"


def normalize_base_url(base_url: str) -> str:
    base_url = base_url.strip()
    if not base_url:
        return ""
    return base_url.rstrip("/")


def build_report_url(base_url: str, report_dir_name: str) -> str:
    if base_url:
        return f"{base_url}/{report_dir_name}/"
    return f"{report_dir_name}/"


def load_json(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as file:
        data = json.load(file)
    if not isinstance(data, dict):
        raise ValueError(f"{path} did not contain a top-level JSON object")
    return data


def metadata_sort_key(entry: dict[str, Any]) -> datetime:
    metadata = entry["metadata"]
    time_end = metadata["time_end"]
    parsed = datetime.fromisoformat(time_end.replace("Z", "+00:00"))
    return parsed


def gather_reports(reports_root: Path, base_url: str) -> list[dict[str, Any]]:
    reports: list[dict[str, Any]] = []
    for child in sorted(reports_root.iterdir()):
        if not child.is_dir():
            continue

        report_json_path = child / "report.json"
        if not report_json_path.is_file():
            continue

        report_json = load_json(report_json_path)
        metadata = report_json.get("metadata")
        if not isinstance(metadata, dict):
            continue

        reports.append({
            "url": build_report_url(base_url, child.name),
            "metadata": metadata,
        })

    reports.sort(key=metadata_sort_key, reverse=True)
    return reports


def write_json_atomically(path: Path, payload: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with NamedTemporaryFile(
        "w",
        encoding="utf-8",
        dir=path.parent,
        prefix=path.name + ".",
        suffix=".tmp",
        delete=False,
    ) as file:
        json.dump(payload, file, indent=2)
        file.write("\n")
        temp_path = Path(file.name)
    temp_path.replace(path)


def main() -> int:
    reports_root = REPORTS_ROOT.resolve()
    output = REPORTS_INDEX_PATH.resolve()
    base_url = normalize_base_url(BASE_URL)

    if not reports_root.is_dir():
        raise SystemExit(f"{reports_root} is not a directory")

    reports = gather_reports(reports_root, base_url)
    write_json_atomically(output, {"reports": reports})
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
