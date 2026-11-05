# Release checklist

This guide covers preparing and publishing Klicky. For installation, use the [user guide](INSTALLATION.md); for package layouts and build commands, see [packaging](PACKAGING.md).

## Published channels

- [v0.4.0](https://github.com/Muneer320/klicky/releases/tag/v0.4.0) is the latest stable Linux package release, with Debian/Arch assets and SHA-256 checksums.
- [Windows November preview](https://github.com/Muneer320/klicky/releases/tag/windows-preview-2026-11-06) provides an unsigned Windows 11 x64 installer and portable ZIP. It is a prerelease, separate from stable Linux publication.
- macOS uses the source installer and optional menu bar helper. The one-command installer includes the helper by default.

Check the actual release assets before changing download instructions. A source commit, CI artifact and published release are different distribution states.

## Before approval

1. Work on a dedicated branch and link its issue/PR. Follow existing commit conventions and preserve the platform backends.
2. Choose a semantic version and synchronize Cargo/package metadata, changelog, help text, direct download links and installer selection. Adding a supported platform belongs in the next feature release; a dated preview need not replace the stable channel.
3. Pass applicable formatting, build, tests, Clippy, script, package and asset checks. Validate Linux/macOS regressions for shared changes and the Windows bundle for Windows packaging changes.
4. Exercise the affected user lifecycle: first install, running/paused update, settings/custom-pack preservation, startup controls and removal. Check failure/recovery paths when changing installers. Keep test data separate from a daily installation.
5. For input/audio changes, use appropriate listening and physical input-to-audible checks. Internal dispatch timing and requested buffer sizes cannot establish acoustic latency. Record the output route and limits of the evidence.
6. Verify bundled sounds, dependency notices/source obligations, exact source identity and payload hashes. Keep private notes, recordings, credentials and local test fixtures out of release assets.
7. Complete review, resolve comments and obtain explicit release authorization. Merging code does not by itself authorize a new release.

## Linux stable packages

The [release workflow](../.github/workflows/release.yml) runs on `v*` tag pushes and requires the exact `vMAJOR.MINOR.PATCH` version from Cargo metadata. It builds Debian/Arch packages, uploads `SHA256SUMS`, and publishes a non-prerelease marked latest.

**Do not use a `v*` tag for an experimental preview.** The current workflow is a stable Linux publisher, not a generic prerelease workflow. Push an approved stable tag only after the preparation PR is merged and release checks pass.

After the assets finish uploading, publish the signed APT archive using the [APT maintenance procedure](MAINTAINING_APT.md#publication). Its GitHub-token release event does not automatically chain into another workflow. Preserve every package version in the authenticated live index; do not replace the archive with a single-package upload.

## Windows preview packages

Use the [Windows package workflow](../.github/workflows/windows-portable.yml) or the identified local build scripts described in the [Windows guide](WINDOWS.md). Confirm both executables, packs, notices, embedded source identity and installer/ZIP SHA-256 sidecars.

An approved platform preview uses a distinct non-`v*` tag, is marked **prerelease**, and is **not latest**. Attach the identified installer, complete portable ZIP and their exact sidecars. The APT workflow excludes prereleases so this does not replace stable Linux packages. CI artifacts alone are temporary downloads, not permanent release links.

The Windows helper selects an identified published installer/checksum pair. Verify its real GitHub release response and download/hash behavior before announcing the command; avoid launching a second app during network-only checks. Signing status and supported Windows architecture belong in the release notes.

## After publication

- Verify public asset names, sizes and checksums, plus the correct stable/prerelease/latest state.
- Confirm the website's installation/removal commands and direct downloads match published assets. Check the README, Windows guide, changelog and social image for drift.
- For Pages/APT changes, verify the deployed page, installer scripts, sound previews, signed metadata, retained package versions and custom domain. Use [website checks](WEBSITE.md#preview-and-check); the page and archive deploy together.
- Record release evidence in the PR or approved internal continuity notes. Keep recovery information until the update is confirmed, then remove merged branches according to the project's workflow.

Publication to additional package managers or public launch channels is a separate step with its own versioned manifests, review and authorization.
