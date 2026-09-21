"""Synthetic live-sidecar check; never captures the microphone or plays audio."""
import array
import json
import math
import time
import urllib.error
import urllib.request

BASE = "http://127.0.0.1:18889"


def convert(chunk, pitch=0, slot=0, index=0):
    signal = array.array("f", (0.08 * math.sin(2 * math.pi * 220 * i / 48000)
                              for i in range(chunk * 48)))
    req = urllib.request.Request(
        f"{BASE}/mnr/convert?slot={slot}&pitch={pitch}&index={index}&chunk_ms={chunk}",
        signal.tobytes(), {"Content-Type": "application/octet-stream"})
    started = time.perf_counter()
    with urllib.request.urlopen(req, timeout=15) as response:
        output = array.array("f")
        output.frombytes(response.read())
    elapsed = (time.perf_counter() - started) * 1000
    assert len(output) == len(signal), (len(output), len(signal))
    assert all(math.isfinite(v) and abs(v) <= 1 for v in output)
    assert sum((a-b)**2 for a, b in zip(signal, output)) > 0.01, "Dry bypass"
    return elapsed, output


if __name__ == "__main__":
    with urllib.request.urlopen(BASE + "/mnr/ready", timeout=3) as response:
        print("Ready:", json.load(response))
    for chunk in (100, 150, 200, 300, 500):
        timings = [convert(chunk)[0] for _ in range(6)]
        print(f"chunk={chunk} ms mean_warm={sum(timings[1:])/5:.1f} ms max={max(timings):.1f} ms")
    _, low = convert(200, -12)
    for _ in range(3):
        _, high = convert(200, 12)
    assert sum((a-b)**2 for a, b in zip(low, high)) > 0.01, "Pitch had no effect"
    for pitch, slot in ((25, 0), (0, 65535)):
        try:
            convert(200, pitch, slot)
            raise AssertionError("Invalid parameter accepted")
        except urllib.error.HTTPError as error:
            assert error.code == 400, error.code
    convert(200)
    print("PASS: frame sizes, finite output, model conversion, pitch changes, invalid settings")
