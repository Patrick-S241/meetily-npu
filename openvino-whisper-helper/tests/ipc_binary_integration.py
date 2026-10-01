"""Exercise the helper's Windows stdin/stdout framing with raw PCM bytes."""

import json
import os
import pathlib
import struct
import subprocess
import sys


def main() -> None:
    helper = pathlib.Path(sys.argv[1])
    runtime = pathlib.Path(sys.argv[2])
    header = json.dumps(
        {
            "protocol": 1,
            "id": 42,
            "op": "transcribe",
            "sample_rate": 16000,
            "sample_count": 1,
            "task": "transcribe",
        },
        separators=(",", ":"),
    ).encode("utf-8")
    # This finite, normalized f32 includes both Ctrl+Z and CRLF. In Windows
    # text mode either byte sequence can truncate or corrupt the PCM frame.
    audio = b"\x1a\x0d\x0a\x3f"
    assert -1.1 <= struct.unpack("<f", audio)[0] <= 1.1
    frame = struct.pack("<I", len(header)) + header + audio
    environment = os.environ.copy()
    environment["PATH"] = str(runtime) + os.pathsep + environment.get("PATH", "")
    result = subprocess.run(
        [str(helper)], input=frame, capture_output=True, timeout=15, env=environment
    )
    assert result.returncode == 0, (result.returncode, result.stderr)
    assert len(result.stdout) >= 4, result.stderr
    length = struct.unpack("<I", result.stdout[:4])[0]
    assert len(result.stdout) == length + 4, result.stdout
    response = json.loads(result.stdout[4:])
    # No model is loaded. A valid framed error proves the full audio payload
    # crossed stdin intact without requiring an NPU on the CI runner.
    assert response["id"] == 42, response
    assert response["error_code"] == "TRANSCRIPTION_FAILED", response


if __name__ == "__main__":
    main()
