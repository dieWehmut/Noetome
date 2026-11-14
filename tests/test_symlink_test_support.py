"""Capability classification must not suppress errors or rejection assertions."""

import errno
import unittest
from pathlib import Path
from unittest.mock import Mock, patch

import symlink_test_support as support


class SymlinkCapabilityTests(unittest.TestCase):
    def test_creation_attempt_preserves_target_and_directory_flag(self):
        for directory in (False, True):
            with self.subTest(directory=directory):
                link = Mock(spec=Path)
                target = Path("explicit-target")
                support.create_symlink_or_skip(link, target, target_is_directory=directory)
                link.symlink_to.assert_called_once_with(target, target_is_directory=directory)

    def test_only_windows_privilege_failure_is_an_explicit_skip(self):
        error = OSError("native symbolic-link privilege missing")
        error.winerror = 1314
        for directory in (False, True):
            with self.subTest(directory=directory):
                link = Mock(spec=Path)
                link.symlink_to.side_effect = error
                with patch.object(support.os, "name", "nt"):
                    with self.assertRaisesRegex(unittest.SkipTest, "WinError 1314") as skipped:
                        support.create_symlink_or_skip(link, "target", target_is_directory=directory)
                self.assertIs(skipped.exception.__cause__, error)
                link.symlink_to.assert_called_once_with("target", target_is_directory=directory)

    def test_other_windows_errors_are_not_capability_skips(self):
        errors = [OSError(errno.EACCES, "generic access denied"),
                  FileExistsError(errno.EEXIST, "destination already exists")]
        for code in (5, 87, 1920):
            error = OSError(f"Windows error {code}")
            error.winerror = code
            errors.append(error)
        for error in errors:
            with self.subTest(error=str(error)):
                link = Mock(spec=Path)
                link.symlink_to.side_effect = error
                with patch.object(support.os, "name", "nt"):
                    with self.assertRaises(OSError) as caught:
                        support.create_symlink_or_skip(link, "target")
                self.assertIs(caught.exception, error)

    def test_posix_error_with_same_numeric_attribute_is_not_skipped(self):
        error = OSError(errno.EPERM, "POSIX fixture creation failed")
        error.winerror = 1314
        link = Mock(spec=Path)
        link.symlink_to.side_effect = error
        with patch.object(support.os, "name", "posix"):
            with self.assertRaises(OSError) as caught:
                support.create_symlink_or_skip(link, "target")
        self.assertIs(caught.exception, error)

    def test_unimplemented_creation_is_not_misreported_as_missing_privilege(self):
        error = NotImplementedError("unexpected unsupported API")
        link = Mock(spec=Path)
        link.symlink_to.side_effect = error
        with patch.object(support.os, "name", "nt"):
            with self.assertRaises(NotImplementedError) as caught:
                support.create_symlink_or_skip(link, "target")
        self.assertIs(caught.exception, error)


if __name__ == "__main__":
    unittest.main()
