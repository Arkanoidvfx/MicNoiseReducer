"""Summarize paced AFX CSV timings; compare PCM from identical input sequences."""
import json
import sys
from pathlib import Path

import numpy as np


def summarize(path):
    data = np.genfromtxt(path, delimiter=",", names=True)
    run = data["run_ms"]
    worst = int(np.argmax(run))
    pcm = np.fromfile(str(path) + ".f32", dtype="<f4")
    if len(pcm) != len(run) * 480 or not np.isfinite(pcm).all():
        raise ValueError("Invalid benchmark output")
    return {
        "frames": len(run),
        "run_ms_p50_p95_p99_max": np.percentile(run, [50, 95, 99, 100]).tolist(),
        "run_over_10ms": int(np.sum(run > 10)),
        "run_over_40ms": int(np.sum(run > 40)),
        "worst_frame": worst,
        "worst_frame_cpu_ms": float(data["thread_cpu_ms"][worst]),
        "start_late_ms_p95_max": np.percentile(data["start_late_ms"], [95, 100]).tolist(),
        "thread_cpu_ms_sum": float(data["thread_cpu_ms"].sum()),
        "output_peak": float(np.max(np.abs(pcm))),
    }, pcm


if __name__ == "__main__":
    reports, reference = {}, None
    for name in sys.argv[1:]:
        path = Path(name)
        report, pcm = summarize(path)
        if reference is not None and len(reference) == len(pcm):
            report["pcm_max_abs_diff_from_first"] = float(np.max(np.abs(pcm-reference)))
        reference = pcm if reference is None else reference
        reports[str(path)] = report
    print(json.dumps(reports, indent=2))
