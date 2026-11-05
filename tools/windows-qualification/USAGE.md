# Windows qualification tools

This development tool exercises Space-key feedback on Windows 11 x64. It is not a Windows Klicky release and does not install a background service. Production Linux/macOS code remains unchanged.

## Build

Use Rust 1.87 or newer and the normal Windows MSVC development prerequisites:

```powershell
$env:CARGO_TARGET_DIR = "$PWD/target/windows-qualification"
$env:KLICKY_QUALIFICATION_REVISION = git rev-parse HEAD
$env:KLICKY_QUALIFICATION_TREE_STATE = if (git status --porcelain=v1) { 'dirty' } else { 'clean' }
cargo build --locked --release --manifest-path tools/windows-qualification/Cargo.toml
```

The standalone crate reuses the production sound loader and player directly. `baseline` uses the current production buffer policy. `device-default` explicitly requests CPAL's default buffer policy for comparison; neither request is a measured endpoint buffer.

## Controlled Windows recording

Choose the built-in speakers and a quiet interval. The helper records microphone audio only when explicitly run. It requires PowerShell 7, Python 3.11 or newer, and FFmpeg. List capture devices without recording:

```powershell
ffmpeg -list_devices true -f dshow -i dummy
```

Run the helper with the microphone name returned above:

```powershell
pwsh -NoProfile -File tools/windows-qualification/capture.ps1 -Microphone 'YOUR MICROPHONE NAME'
```

After READY, press and release Space about once per second. Background application input is preserved. Hold Space briefly to check repeat suppression; Escape ends playback. Recording stops after the sound tail. The helper stores the WAV, prepared Space reference, internal dispatch data, metadata and empty annotation CSV beneath your local `KlickyResearch/qualification-sessions` directory. It never uploads them. Keep conversations and other private activity out of a controlled recording.

The helper rejects a silent/dither-only recording and preserves it for diagnosis. Check microphone mute and input settings before retrying. Detecting a signal does not establish that both onsets can be measured.

Options include `-Buffer device-default`, `-Seconds`, `-Volume`, `-Pack`, and `-OutputDirectory`. Output must be outside tracked source or beneath ignored `target/`; existing sessions are never overwritten. Do not compare different sound packs, volume settings or routes as though they were matched conditions.

To exercise startup/shutdown without recording a microphone:

```powershell
target/windows-qualification/release/klicky-qualify.exe capture --pack sounds/eg-oreo --seconds 2 --output YOUR_PRIVATE_DIRECTORY/smoke.json
```

This check is not physical-input or audible-latency evidence. The tool observes only the declared Space trigger and Escape control, emits no typed text/key-name event log, and bounds its runtime. Its output always says full qualification has not passed.

## Annotate and analyze

Open `annotate.html` locally in a browser, select the controlled `recording.wav`, and inspect its waveform/audio. The page reads PCM16 samples locally without resampling frame indices. No network calls are permitted by its content policy. Select key-impact/reference and generated-onset markers, add accepted pairs or explicitly excluded events, then save the CSV in the private session directory. Use the prepared `reference.wav` to help distinguish generated sound from the keyboard's physical impact. Review ambiguous pairs rather than guessing onsets.

Complete the metadata's comparison identity and independently counted event total. Preserve original acquisition conditions and hashes. Analyze:

```powershell
python -B tools/windows-qualification/analyze.py recording.wav annotations.csv metadata.json --output analysis.json
```

An uncalibrated acoustic impact does not establish electrical actuation time. It can produce descriptive acoustic differences, but the physical latency gate remains not evaluable. Loopback, synthetic timestamps, internal dispatch, small samples, unknown source state and incomplete sequences also cannot pass it. Calibration must identify hashed evidence, reference-after-actuation offset and uncertainty; a reviewer must assess that evidence, not merely trust a metadata number. The analyzer does not grant full qualification even when one latency gate passes.

The per-route latency limits are p95 30 ms and p99 40 ms with at least 1,000 accepted events and reviewed acquisition uncertainty. Repeat runs, listening, input correctness, resources, recovery and existing-platform comparisons remain separate requirements. Exclusions require explicit review.

## Existing-platform baseline

Use the existing Linux/macOS product, not the Windows harness, for baseline playback. Preserve the exact source revision and settings, same hardware/output route, microphone/acquisition method, pack, volume, and onset definition. Capture in normal mode; `start --benchmark` prints key names and adds timing overhead, so it is not the preferred audible-baseline mode.

For Linux systems with FFmpeg PulseAudio input and a selected microphone:

```sh
ffmpeg -f pulse -i default -t 35 -ac 1 -ar 48000 -c:a pcm_s16le -n /YOUR/PRIVATE/recording.wav
```

Start Klicky normally before the controlled sequence; record physical Space presses and use the same annotation procedure. Confirm `default` is the microphone you intend to record. Store results outside the checkout. Prepare equivalent metadata with `platform` set to `linux` or `macos`; do not claim a platform baseline that was not physically tested.

`--baseline baseline-analysis.json` compares matching route/recording/timing-map/volume/reference identities and includes both acquisitions' uncertainty when evaluating the 5 ms p95 regression limit. Matching labels alone do not establish matched hardware; inspect provenance. Microphone processing and acoustic actuation offsets remain limitations even on a dual-boot machine.

Hosted CI checks compilation and tool behavior. It has no physical keyboard/audio qualification role. Recordings, calibration files, local review artifacts and machine-specific results must not be committed or automatically published.
