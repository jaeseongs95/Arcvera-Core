#!/usr/bin/env python3
"""Prepare an explicitly named hooks JSON file; default to a read-only preview.

Only Stop handler additionalContextLimit members are removed. No hook is run.
Windows apply uses an exclusive file handle and preserves a separate original
backup. A crash during the in-place write may require restoring that backup.
"""

import argparse
import codecs
import ctypes
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import uuid


def _unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON member; refusing ambiguous input")
        result[key] = value
    return result


def _reject_constant(_value):
    raise ValueError("non-standard JSON constant")


def _normalize(raw):
    encoding = json.detect_encoding(raw)
    bom = b""
    for prefix, codec in (
        (codecs.BOM_UTF32_LE, "utf-32-le"),
        (codecs.BOM_UTF32_BE, "utf-32-be"),
        (codecs.BOM_UTF8, "utf-8"),
        (codecs.BOM_UTF16_LE, "utf-16-le"),
        (codecs.BOM_UTF16_BE, "utf-16-be"),
    ):
        if raw.startswith(prefix):
            bom, encoding = prefix, codec
            break
    text = raw[len(bom):].decode(encoding)
    data = json.loads(
        text, object_pairs_hook=_unique_object, parse_constant=_reject_constant
    )
    if not isinstance(data, dict):
        raise ValueError("hooks JSON root must be an object")
    events = data.get("hooks", {})
    if not isinstance(events, dict):
        raise ValueError("hooks must be an object")
    groups = events.get("Stop", [])
    if not isinstance(groups, list):
        raise ValueError("hooks.Stop must be an array")
    targets = set()
    for group_index, group in enumerate(groups):
        if not isinstance(group, dict) or not isinstance(group.get("hooks", []), list):
            raise ValueError("Stop groups must contain a hooks array")
        for handler_index, handler in enumerate(group.get("hooks", [])):
            if not isinstance(handler, dict):
                raise ValueError("Stop handlers must be objects")
            if "additionalContextLimit" in handler:
                targets.add(("hooks", "Stop", group_index, "hooks", handler_index))
    if not targets:
        return raw, 0

    # Use the stdlib decoder for values; retain raw member spans instead of
    # reserializing unrelated values, whitespace, escape sequences or numbers.
    decoder = json.JSONDecoder()
    removed = []

    def skip_space(index):
        while index < len(text) and text[index] in " \t\r\n":
            index += 1
        return index

    def visit(index, path):
        index = skip_space(index)
        if text[index] == "{":
            index = skip_space(index + 1)
            previous_comma = None
            while text[index] != "}":
                start = index
                key, index = decoder.raw_decode(text, index)
                value_start = skip_space(skip_space(index) + 1)
                end = visit(value_start, path + (key,))
                index = skip_space(end)
                following_comma = index if text[index] == "," else None
                if path in targets and key == "additionalContextLimit":
                    removed.append((start, end))
                    comma = following_comma if following_comma is not None else previous_comma
                    if comma is not None:
                        removed.append((comma, comma + 1))
                if following_comma is None:
                    break
                previous_comma = following_comma
                index = skip_space(index + 1)
            return index + 1
        if text[index] == "[":
            index = skip_space(index + 1)
            item = 0
            while text[index] != "]":
                index = skip_space(visit(index, path + (item,)))
                item += 1
                if text[index] != ",":
                    break
                index = skip_space(index + 1)
            return index + 1
        return decoder.raw_decode(text, index)[1]

    visit(0, ())
    for start, end in sorted(removed, reverse=True):
        text = text[:start] + text[end:]
    # Refuse an invalid edit before any backup or write.
    json.loads(text, object_pairs_hook=_unique_object, parse_constant=_reject_constant)
    return bom + text.encode(encoding), len(targets)


def _fingerprint(info):
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns, info.st_ctime_ns)


def _open_for_update(path):
    if os.name != "nt":
        raise OSError("apply requires Windows exclusive file sharing")
    import msvcrt
    from ctypes import wintypes

    create = ctypes.WinDLL("kernel32", use_last_error=True).CreateFileW
    create.argtypes = (
        wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
        wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE,
    )
    create.restype = wintypes.HANDLE
    handle = create(str(path), 0xC0000000, 0, None, 3, 0x00200080, None)
    if handle == ctypes.c_void_p(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    try:
        fd = msvcrt.open_osfhandle(handle, os.O_RDWR | os.O_BINARY)
    except BaseException:
        close = ctypes.WinDLL("kernel32", use_last_error=True).CloseHandle
        close.argtypes = (wintypes.HANDLE,)
        close(handle)
        raise
    return os.fdopen(fd, "r+b")


def _write_backup(path, raw):
    backup = path.with_name(f"{path.name}.r38-stop-hook.{uuid.uuid4().hex}.bak")
    with backup.open("xb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    return backup


def normalize_stop_hook_config(path, *, apply=False, expected_sha256=None):
    """Return metadata only; apply explicitly, optionally binding a prior preview.

    Parsing/backup/conflict errors propagate without changing the configuration.
    Backup files are retained, including a partial backup after a backup I/O error.
    """
    path = Path(path).absolute()
    if path.is_symlink():
        raise ValueError("symbolic links are not supported")
    with path.open("rb") as source:
        info = os.fstat(source.fileno())
        if not stat.S_ISREG(info.st_mode):
            raise ValueError("configuration must be a regular file")
        raw = source.read()
    digest = hashlib.sha256(raw).hexdigest()
    if expected_sha256 is not None and digest != expected_sha256:
        raise ValueError("configuration changed since preview")
    normalized, count = _normalize(raw)
    result = {"removed": count, "applied": False, "sha256": digest, "backup": None}
    if not apply or not count:
        return result
    with _open_for_update(path) as target:
        current = os.fstat(target.fileno())
        if (
            getattr(current, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT
            or _fingerprint(current) != _fingerprint(info)
            or target.read() != raw
        ):
            raise ValueError("configuration changed before apply")
        backup = _write_backup(path, raw)
        target.seek(0)
        target.write(normalized)
        target.truncate()
        target.flush()
        os.fsync(target.fileno())
    result.update(applied=True, backup=str(backup))
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=Path, help="Explicit hooks JSON path")
    parser.add_argument("--apply", action="store_true", help="Write with an original backup")
    parser.add_argument("--expected-sha256", help="Require the digest from an earlier preview")
    args = parser.parse_args(argv)
    try:
        result = normalize_stop_hook_config(
            args.path, apply=args.apply, expected_sha256=args.expected_sha256
        )
    except (OSError, ValueError, RecursionError) as error:
        # Do not echo JSON content, command strings or environment values.
        code = getattr(error, "winerror", None) or getattr(error, "errno", None)
        detail = "invalid configuration encoding" if isinstance(error, UnicodeError) else (
            str(error) if isinstance(error, ValueError) else "configuration not normalized"
        )
        print(f"{type(error).__name__}: {detail} (code={code})", file=sys.stderr)
        return 1
    print(json.dumps(result))
    return 0


if __name__ == "__main__":
    sys.exit(main())
