"""Attempt actual symlink fixtures; report only unavailable Windows privilege."""

import os
import unittest
from pathlib import Path


def create_symlink_or_skip(
    link: Path, target: str | Path, *, target_is_directory: bool = False
) -> None:
    try:
        link.symlink_to(target, target_is_directory=target_is_directory)
    except OSError as error:
        if os.name == "nt" and getattr(error, "winerror", None) == 1314:
            kind = "directory" if target_is_directory else "file"
            raise unittest.SkipTest(
                f"Windows cannot create the {kind} symlink fixture: "
                "WinError 1314 (symbolic-link privilege unavailable)"
            ) from error
        raise
