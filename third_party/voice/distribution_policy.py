# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
"""Arcvera distribution policy (2026-11-13); see MODIFICATIONS.md.

Microsoft CRTs and their installers are external prerequisites, irrespective of
whether a particular distributor could obtain redistribution permission.
"""

from pathlib import PurePosixPath
import re

OFFICIAL_RUNTIME_URL = "https://learn.microsoft.com/en-us/cpp/windows/latest-supported-vc-redist"


def is_microsoft_runtime(path: str) -> bool:
    name = PurePosixPath(path.replace("\\", "/")).name.casefold()
    return bool(re.fullmatch(
        r"(?:(?:vcruntime|msvcp|msvcr|concrt|vcomp)\d[^/]*\.dll|"
        r"ucrtbase(?:d)?\.dll|api-ms-win-crt-[^/]+\.dll|"
        r"(?:vc_redist|vcredist)[^/]*\.(?:exe|msi|cab|zip|vsix))", name))


def require_distributable(path: str) -> None:
    if is_microsoft_runtime(path):
        raise ValueError(f"Microsoft runtime must be installed separately: {path}; {OFFICIAL_RUNTIME_URL}")
