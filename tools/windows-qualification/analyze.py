#!/usr/bin/env python3
"""Analyze annotated, same-clock recordings. Never equate dispatch with sound.

Inputs stay local. CSV columns: event_id,reference_frame,audible_frame,accepted,reason.
An uncalibrated acoustic key impact supports a comparison, not an actuation gate.
See --help for the required provenance metadata.
"""
from __future__ import annotations

import argparse
import array
import csv
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import sys
import wave


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def finite(value, label: str) -> float:
    if isinstance(value, bool):
        raise ValueError(f"{label} must be a finite number")
    result = float(value)
    if not math.isfinite(result):
        raise ValueError(f"{label} must be a finite number")
    return result


def percentile(values: list[float], fraction: float) -> float:
    # Explicit nearest-rank selection, unlike the application's older benchmark.
    ordered = sorted(values)
    return ordered[max(0, math.ceil(len(ordered) * fraction) - 1)]


def inspect_recording(path: Path) -> dict:
    with wave.open(str(path), "rb") as stream:
        if stream.getcomptype() != "NONE" or stream.getsampwidth() != 2:
            raise ValueError("use uncompressed PCM16 WAV")
        rate, frames, channels = stream.getframerate(), stream.getnframes(), stream.getnchannels()
        peak, count = 0, 0
        while data := stream.readframes(65536):
            samples = array.array("h", data)
            if sys.byteorder != "little":
                samples.byteswap()
            count += len(samples)
            peak = max(peak, max(map(abs, samples), default=0))
        if count != frames * channels:
            raise ValueError("WAV data is truncated")
        return {"sample_rate": rate, "frames": frames, "channels": channels,
                "sample_width": 2, "peak_abs_i16": peak,
                "signal_above_quantization_dither": peak > 1}


def analyze(wav: Path, annotations: Path, metadata: Path) -> dict:
    meta = json.loads(metadata.read_text(encoding="utf-8-sig"))
    required = ("platform", "source_revision", "comparison_id", "output_route",
                "pack_sha256", "pack_config_sha256", "volume", "reference_kind", "audio_processing")
    for field in required:
        if field not in meta or meta[field] in (None, ""):
            raise ValueError(f"metadata requires {field}")
    if meta["platform"] not in ("windows-11-x64", "linux", "macos"):
        raise ValueError("platform must identify windows-11-x64, linux, or macos")
    if not re.fullmatch(r"[0-9a-fA-F]{40}", meta["source_revision"]):
        raise ValueError("source_revision must be the full tested Git revision")
    for field in ("pack_sha256", "pack_config_sha256"):
        if not re.fullmatch(r"[0-9a-fA-F]{64}", meta[field]):
            raise ValueError(f"{field} must identify the tested sound recording or timing map")
    volume = finite(meta["volume"], "volume")
    if not 0 < volume <= 1:
        raise ValueError("volume must be audible and in (0, 1]")
    kind = meta["reference_kind"]
    if kind not in ("electrical-contact", "acoustic-impact", "loopback", "synthetic"):
        raise ValueError("unknown reference_kind")
    audio = inspect_recording(wav)
    rate, frames = audio["sample_rate"], audio["frames"]

    offset = 0.0
    uncertainty = None
    calibration = meta.get("calibration")
    if calibration is not None:
        # The calibration artifact is required; a number without provenance is
        # insufficient. The operator/reviewer must still validate its content.
        evidence = metadata.parent / calibration["evidence_file"]
        if sha256(evidence) != calibration["evidence_sha256"]:
            raise ValueError("calibration evidence hash mismatch")
        offset = finite(calibration["reference_after_actuation_ms"], "calibration offset")
        uncertainty = finite(calibration["max_uncertainty_ms"], "calibration uncertainty")
        if uncertainty < 0 or offset < 0:
            raise ValueError("calibration offset and uncertainty must be nonnegative")

    delays, excluded, seen = [], [], set()
    previous_reference = -1
    with annotations.open(newline="", encoding="utf-8-sig") as stream:
        reader = csv.DictReader(stream)
        fields = {"event_id", "reference_frame", "audible_frame", "accepted", "reason"}
        if set(reader.fieldnames or ()) != fields:
            raise ValueError("annotation columns must be " + ",".join(sorted(fields)))
        for row in reader:
            event = row["event_id"].strip()
            if not event or event in seen:
                raise ValueError("annotation event IDs must be unique and nonempty")
            seen.add(event)
            if row["accepted"] not in ("yes", "no"):
                raise ValueError("accepted must be yes or no")
            if row["accepted"] == "no":
                if not row["reason"].strip():
                    raise ValueError("excluded events require a reason")
                excluded.append({"event_id": event, "reason": row["reason"]})
                continue
            reference, audible = int(row["reference_frame"]), int(row["audible_frame"])
            if not (0 <= reference < frames and 0 <= audible < frames):
                raise ValueError(f"annotation {event} lies outside the recording")
            if reference <= previous_reference:
                raise ValueError("accepted reference frames must increase")
            previous_reference = reference
            delay = (audible - reference) * 1000 / rate + offset
            if kind == "electrical-contact" and delay < 0:
                raise ValueError("audible onset precedes electrical contact")
            if kind == "acoustic-impact" and calibration is not None and delay < -uncertainty:
                raise ValueError("corrected acoustic onset precedes actuation beyond calibration uncertainty")
            delays.append(delay)
    if not delays:
        raise ValueError("at least one accepted annotated event is required")
    stats = {"count": len(delays), "mean_ms": statistics.mean(delays),
             "p50_ms": percentile(delays, .5), "p95_ms": percentile(delays, .95),
             "p99_ms": percentile(delays, .99), "min_ms": min(delays), "max_ms": max(delays)}
    reasons = []
    if not audio["signal_above_quantization_dither"]:
        reasons.append("recording is silent or contains only quantization dither")
    if kind not in ("electrical-contact", "acoustic-impact"):
        reasons.append("reference does not measure physical actuation to audible output")
    if calibration is None:
        reasons.append("actuation/reference and acquisition uncertainty are uncalibrated")
    elif uncertainty > 1:
        reasons.append("reference/acquisition uncertainty exceeds 1 ms")
    if meta["audio_processing"] != "characterized":
        reasons.append("microphone/acquisition processing delay is not characterized")
    if meta.get("source_tree_dirty", True):
        reasons.append("tested source tree is dirty or its state is unknown")
    if str(meta["comparison_id"]).startswith("replace-with-"):
        reasons.append("physical hardware/acquisition comparison identity is unspecified")
    expected = meta.get("expected_event_count")
    if type(expected) is not int or expected != len(seen):
        reasons.append("annotations do not account for an independently counted physical sequence")
    if len(delays) < 1000:
        reasons.append("fewer than 1000 accepted events on this route")
    if excluded:
        reasons.append("excluded events require review; do not hide missed or ambiguous sounds")
    gate = "not-evaluable" if reasons else (
        "pass" if stats["p95_ms"] + uncertainty <= 30 and stats["p99_ms"] + uncertainty <= 40
        else "fail")
    return {
        "schema_version": 1,
        "measurement": "same-clock annotated audio; operator-provided provenance",
        "metadata": meta, "audio": audio,
        "inputs": {"recording_sha256": sha256(wav), "annotations_sha256": sha256(annotations),
                   "metadata_sha256": sha256(metadata)},
        "latency": stats, "excluded": excluded,
        "latency_gate": {"status": gate, "reasons": reasons,
                         "p95_limit_ms": 30, "p99_limit_ms": 40,
                         "uncertainty_ms": uncertainty},
        "full_qualification_passed": False,
        "remaining_checks": ["repeat runs", "human listening", "reference platforms",
                             "input correctness", "resources", "reliability"],
    }


def compare(candidate: dict, baseline: dict) -> dict:
    for field in ("comparison_id", "output_route", "pack_sha256", "pack_config_sha256", "volume", "reference_kind"):
        if candidate["metadata"][field] != baseline["metadata"][field]:
            raise ValueError(f"baseline comparison differs in {field}")
    if candidate["metadata"]["platform"] == baseline["metadata"]["platform"]:
        raise ValueError("existing-platform comparison requires a different platform")
    if baseline["metadata"]["platform"] not in ("linux", "macos"):
        raise ValueError("baseline must be the existing Linux or macOS product")
    difference = candidate["latency"]["p95_ms"] - baseline["latency"]["p95_ms"]
    evaluable = all(report["latency_gate"]["status"] != "not-evaluable"
                    for report in (candidate, baseline))
    uncertainty = sum(report["latency_gate"]["uncertainty_ms"] or 0
                      for report in (candidate, baseline))
    return {"p95_difference_ms": difference, "limit_ms": 5,
            "combined_uncertainty_ms": uncertainty,
            "status": ("pass" if difference + uncertainty <= 5 else "fail") if evaluable else "not-evaluable",
            "note": "same label does not prove matched hardware; review acquisition and calibration"}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("recording", nargs="?", type=Path)
    parser.add_argument("annotations", nargs="?", type=Path)
    parser.add_argument("metadata", nargs="?", type=Path,
        help="JSON with platform/source_revision/comparison_id/output_route/pack_sha256/pack_config_sha256/volume/"
             "reference_kind/audio_processing; optional calibration must name hashed evidence")
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--inspect-recording", type=Path,
                        help="check PCM signal presence only; does not measure latency")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if args.inspect_recording:
        if args.recording or args.annotations or args.metadata or args.baseline:
            parser.error("signal inspection does not accept annotation or baseline inputs")
        report = {"audio": inspect_recording(args.inspect_recording), "full_qualification_passed": False}
    else:
        if not all((args.recording, args.annotations, args.metadata)):
            parser.error("recording, annotations and metadata are required for latency analysis")
        report = analyze(args.recording, args.annotations, args.metadata)
    if args.baseline:
        report["baseline_comparison"] = compare(report, json.loads(args.baseline.read_text(encoding="utf-8")))
    with args.output.open("x", encoding="utf-8") as stream:
        json.dump(report, stream, indent=2, allow_nan=False)
        stream.write("\n")
    print(json.dumps({key: report[key] for key in ("audio", "latency", "latency_gate") if key in report}, indent=2))


if __name__ == "__main__":
    main()
