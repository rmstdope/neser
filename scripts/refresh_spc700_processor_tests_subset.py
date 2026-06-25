"""Deterministically build a committed SPC700 ProcessorTests subset."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any

REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_FULL_ROOT = (
    REPO_ROOT / "roms/snes/automated_tests/processor_tests/spc700/full/v1"
)
DEFAULT_SUBSET_ROOT = REPO_ROOT / "roms/snes/automated_tests/processor_tests/spc700/v1"
DEFAULT_REPORT_JSON = (
    REPO_ROOT
    / "roms/snes/automated_tests/processor_tests/spc700/subset_coverage_report.json"
)

VECTOR_FILE_RE = re.compile(r"^(?P<opcode>[0-9a-f]{2})\.json$")

FAMILY_ORDER = (
    "system_control",
    "branch",
    "stack",
    "load_store",
    "alu",
    "shift_rotate",
    "flag_control",
    "bit_ops",
)

OPCODE_FAMILY: dict[int, str] = {
    0x00: "system_control",
    0xEF: "system_control",
    0xFF: "system_control",
    0x2F: "branch",
    0x10: "branch",
    0x30: "branch",
    0x50: "branch",
    0x70: "branch",
    0x90: "branch",
    0xB0: "branch",
    0xD0: "branch",
    0xF0: "branch",
    0x0D: "stack",
    0x2D: "stack",
    0x4D: "stack",
    0x6D: "stack",
    0x8E: "stack",
    0xAE: "stack",
    0x8D: "load_store",
    0xCD: "load_store",
    0xE8: "load_store",
    0x7D: "load_store",
    0xDD: "load_store",
    0xFD: "load_store",
    0xE4: "load_store",
    0xC4: "load_store",
    0x69: "alu",
    0x88: "alu",
    0xA8: "alu",
    0xC8: "alu",
    0x7C: "shift_rotate",
    0x3C: "shift_rotate",
    0x5C: "shift_rotate",
    0x1C: "shift_rotate",
    0xE0: "flag_control",
    0x20: "flag_control",
    0x40: "flag_control",
    0x60: "flag_control",
    0x0B: "bit_ops",
    0x2B: "bit_ops",
    0x4B: "bit_ops",
    0x6B: "bit_ops",
}


@dataclass(frozen=True)
class VectorFile:
    opcode: int
    path: Path

    @property
    def filename(self) -> str:
        return self.path.name


def _family_for_opcode(opcode: int) -> str:
    return OPCODE_FAMILY.get(opcode, "alu")


def discover_vector_files(root: Path) -> list[VectorFile]:
    files: list[VectorFile] = []
    if not root.exists():
        return files

    for path in sorted(root.glob("*.json")):
        match = VECTOR_FILE_RE.fullmatch(path.name)
        if match is None:
            continue

        files.append(VectorFile(opcode=int(match.group("opcode"), 16), path=path))

    return files


def select_subset_files(
    files: list[VectorFile], opcodes_per_family: int
) -> list[VectorFile]:
    if opcodes_per_family <= 0:
        raise ValueError("opcodes_per_family must be > 0")

    by_family: dict[str, list[VectorFile]] = defaultdict(list)
    for item in files:
        family = _family_for_opcode(item.opcode)
        by_family[family].append(item)

    selected: list[VectorFile] = []
    for family in FAMILY_ORDER:
        candidates = sorted(
            by_family.get(family, []),
            key=lambda item: (item.path.stat().st_size, item.opcode),
        )
        selected.extend(candidates[:opcodes_per_family])

    return selected


def _materialize_payload(item: VectorFile, max_vectors_per_file: int | None) -> bytes:
    payload = item.path.read_bytes()
    if max_vectors_per_file is None or max_vectors_per_file == 0:
        return payload

    if max_vectors_per_file < 0:
        raise ValueError("max_vectors_per_file must be >= 0")

    vectors = json.loads(payload)
    if not isinstance(vectors, list):
        raise ValueError(f"vector file is not a JSON list: {item.path}")

    truncated = vectors[:max_vectors_per_file]
    return (json.dumps(truncated, separators=(",", ":")) + "\n").encode("utf-8")


def write_subset(
    selected: list[VectorFile],
    subset_root: Path,
    max_vectors_per_file: int | None = None,
) -> None:
    subset_root.mkdir(parents=True, exist_ok=True)

    for existing in subset_root.glob("*.json"):
        existing.unlink()

    for item in selected:
        payload = _materialize_payload(item, max_vectors_per_file)
        (subset_root / item.filename).write_bytes(payload)


def build_report(
    selected: list[VectorFile],
    max_vectors_per_file: int | None = None,
) -> dict[str, Any]:
    # Build family coverage and compute payload integrity in one pass
    families: dict[str, dict[str, Any]] = {}
    per_file_records: list[str] = []
    total_size_bytes = 0

    sorted_selected = sorted(selected, key=lambda item: item.filename)
    for item in sorted_selected:
        payload = _materialize_payload(item, max_vectors_per_file)
        total_size_bytes += len(payload)
        per_file_records.append(
            f"{hashlib.sha256(payload).hexdigest()}  {item.filename}"
        )

        family = _family_for_opcode(item.opcode)
        bucket = families.setdefault(
            family,
            {
                "opcodes": set(),
                "files": [],
            },
        )
        bucket["opcodes"].add(f"{item.opcode:02x}")
        bucket["files"].append(item.filename)

    # Normalize family data for JSON serialization
    normalized_families: dict[str, dict[str, Any]] = {}
    for family, data in sorted(families.items()):
        normalized_families[family] = {
            "opcode_count": len(data["opcodes"]),
            "opcodes": sorted(data["opcodes"]),
            "files": sorted(data["files"]),
        }

    tree_payload = "\n".join(per_file_records) + "\n"

    return {
        "selected_file_count": len(selected),
        "selected_files": [item.filename for item in selected],
        "integrity": {
            "kind": "tree_sha256",
            "sha256": hashlib.sha256(tree_payload.encode("utf-8")).hexdigest(),
            "file_count": len(selected),
            "total_size_bytes": total_size_bytes,
        },
        "family_coverage": normalized_families,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--full-root", type=Path, default=DEFAULT_FULL_ROOT)
    parser.add_argument("--subset-root", type=Path, default=DEFAULT_SUBSET_ROOT)
    parser.add_argument("--report-json", type=Path, default=DEFAULT_REPORT_JSON)
    parser.add_argument("--opcodes-per-family", type=int, default=1)
    parser.add_argument("--max-vectors-per-file", type=int, default=32)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    files = discover_vector_files(args.full_root)
    if not files:
        print(f"error: no SPC700 vectors found in {args.full_root}")
        return 1

    selected = select_subset_files(files, args.opcodes_per_family)
    if not selected:
        print("error: selection produced no files")
        return 1

    report = build_report(selected, max_vectors_per_file=args.max_vectors_per_file)
    print(json.dumps(report, indent=2, sort_keys=True))

    if args.report_json is not None and not args.dry_run:
        args.report_json.parent.mkdir(parents=True, exist_ok=True)
        args.report_json.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )

    if not args.dry_run:
        write_subset(
            selected,
            args.subset_root,
            max_vectors_per_file=args.max_vectors_per_file,
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
