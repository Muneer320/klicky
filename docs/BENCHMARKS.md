# Benchmarks

Klicky reports input-to-dispatch latency when started with `--benchmark`:

```bash
klicky start --benchmark
```

Each mapped keypress prints the elapsed time between the platform input callback and the call that adds the cached audio source to the persistent mixer.

## Current Linux result

Test environment:

| Property | Value |
|---|---|
| Operating system | Arch Linux |
| Kernel | 7.2.3-arch1-3 |
| Desktop | Hyprland on Wayland |
| Audio server | PipeWire 1.6.8 |
| Output route | PipeWire ALSA compatibility path |
| Keyboard devices | AT Translated Set 2 keyboard, ITE Device(8176) Keyboard |
| Sound pack | `eg-oreo` |
| Klicky volume | 0.8 |
| Sample count | 55 physical keypresses |

Results:

| Metric | Time |
|---|---:|
| Average | 0.241 ms |
| Minimum | 0.01 ms |
| p50 | 0.21 ms |
| p95 | 0.56 ms |
| p99 | 0.57 ms |
| Maximum | 0.57 ms |

The summary sorts the samples and selects zero-based indices `n / 2`, `floor(0.95 * n)`, and `floor(0.99 * n)`. This is an index-based selection, not an interpolated percentile or the standard nearest-rank definition.

A real excerpt from that run:

```text
[ShiftLeft] 0.56ms  (avg: 0.56ms, min: 0.56ms, max: 0.56ms, n=1)
[KeyH] 0.10ms  (avg: 0.33ms, min: 0.10ms, max: 0.56ms, n=2)
[KeyE] 0.10ms  (avg: 0.25ms, min: 0.10ms, max: 0.56ms, n=3)
[KeyL] 0.21ms  (avg: 0.24ms, min: 0.10ms, max: 0.56ms, n=4)
[Space] 0.22ms  (avg: 0.25ms, min: 0.04ms, max: 0.57ms, n=21)
[ControlLeft] 0.04ms  (avg: 0.24ms, min: 0.01ms, max: 0.57ms, n=44)
[Return] 0.25ms  (avg: 0.24ms, min: 0.01ms, max: 0.57ms, n=55)
```

## Audio buffering

The original Linux prototype negotiated:

```text
1102 frames / 44100 Hz = about 25 ms
```

Users could feel that delay even though dispatch stayed below 1 ms.

After moving to Rodio 0.22 and requesting a fixed low-latency buffer, the tested PipeWire node negotiated:

```text
256 frames / 44100 Hz = about 5.8 ms
```

The same hardware test confirmed that the audible lag was gone or clearly reduced. The 256-frame stream remained clean during normal typing. Klicky falls back to 512, 1024, and finally a device-selected configuration when a backend rejects 256 frames.

## Loudness investigation

The original `eg-oreo` key slices were much quieter than several Cherry MX packs:

| Slice | Mean level | Peak level |
|---|---:|---:|
| `KeyA` | -40.9 dB | -16.2 dB |
| `Space` | -35.4 dB | -14.5 dB |
| `Return` | -37.0 dB | -18.1 dB |

For comparison, `cherrymx-blue-pbt` KeyA measured -30.7 dB mean and -6.7 dB peak. Klicky now normalizes each quiet key slice once at startup, targeting an 85 percent peak with a maximum 6x gain. A real-hardware listening test confirmed improved volume without audible clipping.

## What these numbers do not prove

Input-to-dispatch timing excludes:

- Kernel and hardware time before the callback
- Audio buffer duration
- PipeWire or CoreAudio scheduling
- DAC conversion
- Speaker propagation

A rigorous physical-to-audible test requires an external microphone or loopback setup that records the physical key impact and generated sound on one timeline. Until that test exists, Klicky reports dispatch and negotiated buffer measurements separately.

## Reproducing the dispatch test

1. Install and start Klicky normally once.
2. Stop the service if it is active:

   ```bash
   systemctl --user stop klicky.service
   ```

3. Run the benchmark in a terminal:

   ```bash
   klicky start --benchmark
   ```

4. Type a representative sequence with letters, modifiers, Space, Enter, Backspace, and media keys.
5. Stop it from another terminal:

   ```bash
   klicky stop
   ```

6. Restart the user service if desired:

   ```bash
   systemctl --user start klicky.service
   ```

Benchmark mode prints mapped key names. Do not use it while entering passwords or other sensitive text.
