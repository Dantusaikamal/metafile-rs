#!/usr/bin/env python3
"""Losslessly extract validated OfficeArt image BLIPs from legacy DOC/PPT files.

Requires the small `olefile` Python package. The script never invokes Office or
converts image data: compressed WMF/EMF BLIPs are inflated to their original
metafile bytes, while PNG/JPEG BLIPs are copied byte-for-byte.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import zlib
from pathlib import Path

import olefile


BLIP_TYPES = {
    0xF01A: ("emf", "emf"),
    0xF01B: ("wmf", "wmf"),
    0xF01D: ("jpeg", "jpg"),
    0xF01E: ("png", "png"),
    0xF01F: ("dib", "dib"),
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def valid_metafile(kind: str, data: bytes) -> bool:
    if kind == "emf":
        return len(data) >= 88 and data[:4] == b"\x01\0\0\0" and data[40:44] == b" EMF"
    return len(data) >= 18 and (
        data[:4] == b"\xd7\xcd\xc6\x9a"
        or (data[:2] in (b"\x01\0", b"\x02\0") and data[2:4] == b"\x09\0")
    )


def decode_metafile_blip(kind: str, payload: bytes) -> bytes | None:
    # A second 16-byte UID is present for the alternate OfficeArt BLIP forms.
    for header_at in (16, 32):
        if header_at + 34 > len(payload):
            continue
        original_size = struct.unpack_from("<I", payload, header_at)[0]
        saved_size = struct.unpack_from("<I", payload, header_at + 28)[0]
        compression = payload[header_at + 32]
        encoded = payload[header_at + 34 : header_at + 34 + saved_size]
        if not original_size or len(encoded) != saved_size:
            continue
        try:
            decoded = zlib.decompress(encoded) if compression == 0 else encoded
        except zlib.error:
            continue
        if len(decoded) == original_size and valid_metafile(kind, decoded):
            return decoded
    return None


def decode_bitmap_blip(kind: str, payload: bytes) -> bytes | None:
    signatures = {
        "jpeg": (b"\xff\xd8", b"\xff\xd9"),
        "png": (b"\x89PNG\r\n\x1a\n", b"IEND\xaeB`\x82"),
    }
    if kind == "dib":
        return payload[17:] if len(payload) > 17 else None
    start_signature, end_signature = signatures[kind]
    for start in (17, 33):
        data = payload[start:]
        if data.startswith(start_signature) and data.endswith(end_signature):
            return data
    return None


def extract(source: Path, output: Path) -> list[dict[str, object]]:
    output.mkdir(parents=True, exist_ok=True)
    found: list[dict[str, object]] = []
    seen_records: set[tuple[str, int]] = set()
    counters: dict[str, int] = {}
    with olefile.OleFileIO(source) as container:
        for stream_parts in container.listdir(streams=True, storages=False):
            stream_name = "/".join(stream_parts)
            stream = container.openstream(stream_parts).read()
            for offset in range(max(0, len(stream) - 7)):
                options, record_type, size = struct.unpack_from("<HHI", stream, offset)
                if record_type not in BLIP_TYPES or offset + 8 + size > len(stream):
                    continue
                key = (stream_name, offset)
                if key in seen_records:
                    continue
                kind, extension = BLIP_TYPES[record_type]
                payload = stream[offset + 8 : offset + 8 + size]
                data = (
                    decode_metafile_blip(kind, payload)
                    if kind in ("emf", "wmf")
                    else decode_bitmap_blip(kind, payload)
                )
                if data is None:
                    continue
                seen_records.add(key)
                counters[kind] = counters.get(kind, 0) + 1
                name = f"officeart-{counters[kind]:03d}.{extension}"
                target = output / name
                if target.exists() and target.read_bytes() != data:
                    raise FileExistsError(f"refusing to overwrite different data: {target}")
                target.write_bytes(data)
                found.append(
                    {
                        "file": name,
                        "format": kind,
                        "bytes": len(data),
                        "sha256": sha256(data),
                        "oleStream": stream_name,
                        "streamOffset": offset,
                        "officeArtRecord": f"0x{record_type:04X}",
                        "officeArtOptions": f"0x{options:04X}",
                    }
                )
    return found


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--manifest", type=Path)
    args = parser.parse_args()
    source = args.source.resolve(strict=True)
    result = extract(source, args.output.resolve())
    manifest = {
        "source": source.name,
        "sourceBytes": source.stat().st_size,
        "sourceSha256": sha256(source.read_bytes()),
        "images": result,
    }
    if args.manifest:
        args.manifest.parent.mkdir(parents=True, exist_ok=True)
        args.manifest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(manifest, indent=2))


if __name__ == "__main__":
    main()
