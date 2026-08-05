import hashlib
import importlib.util
import io
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from typing import Optional
from unittest.mock import patch


SCRIPT = Path(__file__).parents[1] / "scripts" / "prepare_codebase_memory_artifact.py"
SPEC = importlib.util.spec_from_file_location("prepare_codebase_memory_artifact", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class FakeResponse(io.BytesIO):
    def __init__(
        self,
        payload: bytes,
        declared_length: int,
        status: int = 200,
        content_range: Optional[str] = None,
    ):
        super().__init__(payload)
        self.headers = {"Content-Length": str(declared_length)}
        if content_range is not None:
            self.headers["Content-Range"] = content_range
        self.status = status


class VerifiedDownloadTests(unittest.TestCase):
    def test_curl_download_is_verified_before_replacing_cache(self):
        payload = b"curl-verified-release-archive"
        expected = hashlib.sha256(payload).hexdigest()

        def complete_download(arguments, **_kwargs):
            output = Path(arguments[arguments.index("--output") + 1])
            output.write_bytes(payload)
            return SimpleNamespace(returncode=0)

        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "artifact.tar.gz"
            destination.write_bytes(b"stale")
            with (
                patch.object(MODULE.shutil, "which", return_value="/usr/bin/curl"),
                patch.object(MODULE.subprocess, "run", side_effect=complete_download),
                patch.object(MODULE.urllib.request, "urlopen") as urlopen,
            ):
                MODULE.download("https://example.invalid/artifact", destination, expected)
            self.assertEqual(destination.read_bytes(), payload)
            urlopen.assert_not_called()

    def test_verified_download_atomically_replaces_destination(self):
        payload = b"verified-codebase-memory-archive"
        expected = hashlib.sha256(payload).hexdigest()
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "artifact.tar.gz"
            destination.write_bytes(b"stale")
            with (
                patch.object(MODULE.shutil, "which", return_value=None),
                patch.object(
                    MODULE.urllib.request,
                    "urlopen",
                    return_value=FakeResponse(payload, len(payload)),
                ),
            ):
                MODULE.download("https://example.invalid/artifact", destination, expected)
            self.assertEqual(destination.read_bytes(), payload)

    def test_truncated_download_never_replaces_existing_cache(self):
        existing = b"previous-cache"
        full_payload = b"expected-complete-payload"
        truncated = full_payload[:8]
        expected = hashlib.sha256(full_payload).hexdigest()
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "artifact.tar.gz"
            destination.write_bytes(existing)
            with (
                patch.object(MODULE.shutil, "which", return_value=None),
                patch.object(
                    MODULE.urllib.request,
                    "urlopen",
                    side_effect=lambda *_args, **_kwargs: FakeResponse(
                        truncated, len(full_payload)
                    ),
                ),
                patch.object(MODULE.time, "sleep", return_value=None),
            ):
                with self.assertRaisesRegex(RuntimeError, "after 5 attempts"):
                    MODULE.download(
                        "https://example.invalid/artifact", destination, expected
                    )
            self.assertEqual(destination.read_bytes(), existing)
            self.assertEqual(list(destination.parent.glob("*.download-*")), [])

    def test_truncated_transfer_resumes_before_atomic_replace(self):
        payload = b"verified-resumable-release-archive"
        first = payload[:11]
        second = payload[11:]
        expected = hashlib.sha256(payload).hexdigest()
        responses = [
            FakeResponse(first, len(payload), status=200),
            FakeResponse(
                second,
                len(second),
                status=206,
                content_range=f"bytes 11-{len(payload) - 1}/{len(payload)}",
            ),
        ]
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "artifact.tar.gz"
            with (
                patch.object(MODULE.shutil, "which", return_value=None),
                patch.object(
                    MODULE.urllib.request,
                    "urlopen",
                    side_effect=responses,
                ) as request,
                patch.object(MODULE.time, "sleep", return_value=None),
            ):
                MODULE.download("https://example.invalid/artifact", destination, expected)
            self.assertEqual(destination.read_bytes(), payload)
            range_header = request.call_args_list[1].args[0].get_header("Range")
            self.assertEqual(range_header, "bytes=11-")


if __name__ == "__main__":
    unittest.main()
