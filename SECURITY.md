# Security policy

## Supported versions

Security fixes are currently applied to the latest commit on the default branch. Klicky has not published a stable release yet.

## Reporting a vulnerability

Please do not open a public issue for vulnerabilities involving:

- Keystroke exposure
- Input-device permissions
- Unsafe file or socket permissions
- Command execution
- Path traversal
- Malicious sound-pack handling

Use GitHub's private vulnerability reporting feature from the repository's **Security** tab. Include:

1. A concise description
2. Affected commit or version
3. Reproduction steps
4. Expected and actual behavior
5. Impact assessment
6. A suggested fix, if available

Reports will be acknowledged as soon as practical. Please allow time for a fix before public disclosure.

## Security model

Klicky listens for global physical key events, which is inherently sensitive.

The daemon receives raw global key-down events, including events generated while passwords or other sensitive text are entered. Klicky discards those events after selecting a sound, but the permission itself carries keylogging capability and should be granted only to trusted software.

### Linux

- The daemon runs as the logged-in user, never as root.
- The included udev rule uses `uaccess` only for devices tagged as keyboards.
- Normal mode does not log key names.
- Benchmark mode logs mapped key names and timing.
- No event data is stored or transmitted.
- IPC uses a Unix socket inside a mode `0700` runtime directory.
- The socket and PID file use mode `0600`.

### macOS

- Global input requires explicit Accessibility permission.
- Klicky uses listen-only event taps.
- No key data leaves the process.

### Network behavior

Klicky contains no network client and makes no network requests.

## Operational guidance

- Do not run benchmark mode while entering sensitive information.
- Install sound packs only from sources you trust.
- Review custom pack JSON before use.
- Keep the binary and configuration owned by your user.
- Do not broaden `/dev/input` permissions to world-readable mode.

## Out of scope

The following are not considered vulnerabilities by themselves:

- A local process running as the same user observing Klicky's public process state
- Sound packs with unpleasant or unexpectedly loud recordings
- Denial of service caused by a deliberately malformed local configuration owned by the same user, unless it crosses a privilege boundary
