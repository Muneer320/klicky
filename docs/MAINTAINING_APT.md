# Maintaining the signed APT repository

For user installation, see [APT setup](APT_REPOSITORY.md). This page covers signing, publication and repository maintenance.

## Signing-key setup

Run the signing-key setup script from a trusted local machine with GnuPG pinentry available:

```bash
./scripts/setup-apt-signing-key.sh
```

The script creates:

- RSA 4096 certification key with a two-year expiry
- RSA 4096 signing subkey with a two-year expiry
- Passphrase-protected offline private-key backup
- Public binary and armored keys
- Revocation certificate
- GitHub Actions secrets for the signing subkey, passphrase, and fingerprint

The setup script refuses to replace an existing key directory.

If setup stops after key generation or only some GitHub secrets are written, resume safely with:

```bash
./scripts/setup-apt-signing-key.sh --resume
```

Resume mode verifies the existing signing subkey and all offline backup files before restoring GitHub secrets. It never generates or replaces key material.

After the public repository is verified:

1. Move `offline-backup/` to encrypted removable storage.
2. Verify the copied files can be read from that storage.
3. Remove the local online GnuPG home under `~/.local/share/klicky/apt-signing/gnupg`.
4. Retain at least two encrypted offline copies in separate locations.

Never commit private keys, passphrases, or revocation certificates.

## Publication

[The APT workflow](../.github/workflows/apt-repository.yml) publishes through the existing GitHub Pages artifact and deploy job. It runs for a published stable GitHub release, a manual dispatch, or a push to the workflow's listed `master` paths (including the website, public media and install/uninstall scripts). Drafts and prereleases are excluded. Pull requests do not receive signing secrets or deploy.

**The tag-driven release workflow does not automatically chain into APT publication.** [The release workflow](../.github/workflows/release.yml) publishes using `GITHUB_TOKEN`; GitHub suppresses subsequent release-event workflow runs from that token. See [GitHub's workflow-trigger rules](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow#triggering-a-workflow-from-a-workflow).

The release tag is resolved from the release event for `release: published`, from the required `tag` input for a manual dispatch, and from GitHub's latest non-draft, non-prerelease release for a qualifying push to `master`. Each path then downloads the `.deb` and `SHA256SUMS` for that exact tag, requires exactly one amd64 package and exactly one checksum entry for it, verifies the checksum, and checks that its Debian version matches the tag.

After the release workflow has finished uploading the `.deb` and `SHA256SUMS`, a maintainer can also dispatch publication for that release:

```bash
gh workflow run apt-repository.yml --repo Muneer320/klicky \
  --ref master -f tag=v0.4.0
```

Replace the tag with the intended published release. The workflow accepts `vMAJOR.MINOR.PATCH`, requires exactly one matching amd64 `.deb`, verifies its checksum, and requires its Debian version to be `MAJOR.MINOR.PATCH-1`.

Publication prerequisites:

- GitHub Pages configured to deploy through Actions, with the `github-pages` environment permitting deployment.
- Secrets `APT_GPG_PRIVATE_KEY`, `APT_GPG_PASSPHRASE`, and `APT_GPG_FINGERPRINT`, configured by the signing-key setup script.
- A signing key whose fingerprint matches the value pinned in the workflow and the installation instructions. Key rotation must update these together.
- Permission to dispatch the workflow; its deployment job uses `pages: write` and `id-token: write`.

Before building, the workflow exports only the public key for the pinned signing fingerprint and configures an isolated APT source using that key via `Signed-By`. `apt-get update` must verify the live `InRelease`; the workflow then downloads every `klicky` version in the authenticated package index. A signature, index, package-download, or checksum failure stops the job before artifact upload. There is no fallback to a single-package repository.

The builder recursively imports `.deb` files from `--existing-root` and the selected release package. A same-named incoming package replaces that file; differently versioned packages are retained. `reprepro` builds the package pool while keeping superseded files; `dpkg-scanpackages --multiversion` then generates the final index containing every package version. `apt-ftparchive` regenerates Release checksums from those final indexes, and GnuPG signs `Release` and `InRelease` with the same pinned key. The metadata and signature bytes are regenerated, not copied byte-for-byte.

The workflow:

1. Downloads and verifies the selected release's amd64 `.deb` and checksums.
2. Imports the encrypted signing subkey into an ephemeral GnuPG home.
3. Verifies the key and passphrase with a signing probe.
4. Rebuilds the Pages artifact from the complete set of retained packages plus the selected release package.
5. Generates and verifies `InRelease`, `Release`, `Release.gpg`, and package indexes.
6. Renders the redesigned homepage and includes its install instructions, the public key files, repository metadata, every indexed package, and the configured `CNAME`.
7. Validates that exact artifact with the local APT integration test before upload.
8. Uploads only public repository files as a Pages artifact and deploys through GitHub's OIDC-backed Pages environment.

Pull requests never receive signing secrets and cannot publish the repository.

The integration validator checks for the redesigned homepage and install instructions, public key files, all required APT metadata, every package file and its indexed size and SHA-256, and equality between the input package versions and the generated index. It then runs `apt-get update` against the built artifact using its exported keyring, checks that APT discovers every version, and confirms the correct Debian candidate. A manual dispatch of an older release does not remove newer packages; the repository index still selects the highest Debian version.

## Validation coverage

[Package CI](../.github/workflows/packages.yml) runs the builder's unit tests and a Linux integration test with a temporary signing key, a synthetic historical package, and a local `file:` repository. The Pages workflow runs the same integration validator against the exact output directory, using the already-checked production signing key and the live package set. Both paths verify generated signatures and APT discovery before deployment. There is no automated post-deployment installation check against the public URL; a successful workflow does not itself prove the public DNS/CDN has finished serving the new artifact.

## Key rotation

Begin rotation before the current key expires:

1. Generate a new certification key and signing subkey.
2. Publish both old and new public keys during a transition release.
3. Update installation documentation with both fingerprints.
4. Sign repository metadata with the new key.
5. Release a keyring update before removing the old key.
6. Revoke the old key if it is compromised.

A routine expiry should use overlap. A compromise should use immediate revocation and a clearly announced replacement fingerprint.
