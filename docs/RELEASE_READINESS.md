# Release checklist

Klicky v0.5.0 brings Linux, macOS and Windows 11 x64 into one release. Linux ships Debian/Arch packages; Windows ships an unsigned per-user installer and portable ZIP; macOS uses the versioned source installer, with its menu helper included by default.

This source tree prepares v0.5.0. A candidate build is not a published release. Keep the release-preparation PR open until approval to transition the public installation commands and website. The older Windows preview remains a historical download; new Windows users should use the stable installer after publication.

## Build a candidate without publishing

The [release workflow](../.github/workflows/release.yml) accepts a manual run on a reviewed branch or a `v*` tag. Tag runs verify the Cargo version. It has read-only repository permissions and **does not publish**, mark a release latest or deploy APT.

```bash
gh workflow run release.yml --ref YOUR_REVIEWED_BRANCH
gh run list --workflow release.yml
gh run download RUN_ID --name klicky-0.5.0-release-candidate --dir candidate
```

The workflow reuses Linux/macOS CI, macOS Intel/Apple Silicon installation/removal, Windows core checks, Debian/Arch packaging and signed APT integration checks. It builds the Windows packages using the pinned toolchain and Inno compiler, archives the exact source, then verifies the combined asset set. `RELEASE-MANIFEST.json` records source revision/tree, signing status, sizes and hashes; `SHA256SUMS` also covers the manifest.

Expected payloads:

- `klicky_0.5.0-1_amd64.deb`
- `klicky-0.5.0-1-x86_64.pkg.tar.zst`
- `klicky-0.5.0-windows-x64-setup.exe` and its `.sha256`
- `klicky-0.5.0-windows-x64.zip` and its `.sha256`
- `klicky-0.5.0-source.tar.gz`

## Before approval

1. Use focused issues/PRs and dedicated branches. Complete independent review and resolve valid findings. Verify the candidate source matches the intended merged tree.
2. Synchronize Cargo/lockfile, Debian/Arch/resource versions, changelog, installer selection, README, guides and website. Check the source archive retains executable script modes.
3. Pass all candidate jobs. Verify bundled sounds, dependency notices/source obligations, payload hashes and exact source identity. Exclude personal configuration, recordings, credentials and local fixtures.
4. Exercise Windows first install, running/paused update, startup, configuration/custom-pack preservation, normal removal and `-Purge` with disposable data. Verify the missing-CLI recovery path, modified/foreign-file refusal and active-bundle interlock. Keep a recoverable backup when testing a daily installation.
5. Exercise Linux package/source lifecycle and native macOS setup/removal. Containers and hosted runners cover packaging and control behavior; they do not provide physical keyboards or audible output.
6. Scan the exact Windows files with current antivirus protection enabled. A prior Microsoft determination applies to the submitted file, not all later builds. Keep signatures, hashes and results with the candidate evidence.
7. Check user-facing sound timing and recovery on the available desktop/output routes. Retain measured results separately from listening feedback. Record unavailable physical-latency/baseline and additional-hardware checks as deferred, never as passed. Review their release impact explicitly.
8. Obtain explicit approval for the concrete version, assets, unsigned Windows status and any deferred validation. A merge, green CI or candidate artifact does not authorize publication.

## Approved publication

Only after approval, merge the preparation PR, create the version tag on the approved source and run the candidate workflow for that tag. Verify/download the resulting final assets and repeat exact-byte security/hash checks. Do not overwrite published assets with a rebuild.

The Pages/APT workflow holds the current site when a source-version bump is merged before its matching stable release exists. Publication of the matching release allows the new installation commands and site to deploy together; a manual run with a mismatched source version is refused.

Create the GitHub release with the approved notes, attach the five payloads, Windows sidecars, manifest and `SHA256SUMS`, mark it non-prerelease and latest, then verify public downloads. Windows code signing is separate from versioning; describe the unsigned installer plainly.

Publish the signed APT archive using the [maintenance procedure](MAINTAINING_APT.md#publication), retaining every authenticated existing package version. Dispatch its workflow on `master`: the current Pages environment rejects deployments from version tags, even when their release-event build succeeds. A release created with GitHub's workflow token does not automatically trigger another workflow. Confirm the archive and Pages deployment completed before announcing the updated one-line commands.

## After publication

- Verify public asset names/sizes/hashes and stable/latest state. Run the downloaded Windows installer, not just a local build with similar source.
- Check website install/removal endpoints, README/download links, source checksums, sound previews and custom domain. Verify APT signatures, candidate version and retained historical packages.
- Record exact release evidence in the PR or private continuity notes. Preserve unresolved recovery material and remove merged branches when no longer needed.
- Package-manager submissions such as WinGet or Homebrew are separate reviewed publications. Do not advertise a command until that channel is actually available.
