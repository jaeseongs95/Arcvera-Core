# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
"""Retired CRT staging entry point (Arcvera 2026-11-13).

Kept so older build commands fail before network or file writes.
Microsoft runtimes must be obtained and installed separately by the user.
"""

from distribution_policy import OFFICIAL_RUNTIME_URL


def stage(root, target, helper):
    raise ValueError('Microsoft CRT staging is disabled by distribution policy. '
                     'Install the official runtime separately: ' + OFFICIAL_RUNTIME_URL)


if __name__ == '__main__':
    raise SystemExit('Microsoft CRT staging is disabled. See ' + OFFICIAL_RUNTIME_URL)
