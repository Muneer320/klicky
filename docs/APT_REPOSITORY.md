# Klicky APT repository

The Klicky APT repository publishes signed amd64 packages at:

```text
https://muneer320.github.io/klicky
```

## Install the repository

Download the public key without granting trust yet:

```bash
curl --fail --silent --show-error --location \
  --output /tmp/klicky-archive-keyring.gpg \
  https://muneer320.github.io/klicky/klicky-archive-keyring.gpg
```

Inspect its fingerprint:

```bash
gpg --show-keys --with-fingerprint /tmp/klicky-archive-keyring.gpg
```

The fingerprint must exactly match:

```text
DBB6 7AE4 78D2 FFCE C663 7B55 9899 E554 D358 0D8F
```

Do not install the key if the fingerprint differs.

Install the verified key and Deb822 source definition:

```bash
sudo install -d -m 0755 /etc/apt/keyrings
sudo install -m 0644 /tmp/klicky-archive-keyring.gpg \
  /etc/apt/keyrings/klicky-archive-keyring.gpg

curl --fail --silent --show-error --location \
  --output /tmp/klicky.sources \
  https://muneer320.github.io/klicky/klicky.sources

sudo install -m 0644 /tmp/klicky.sources \
  /etc/apt/sources.list.d/klicky.sources

sudo apt update
sudo apt install klicky
systemctl --user enable --now klicky.service
```

The source is restricted to amd64 and binds this repository to its dedicated key through `Signed-By`. It does not add the key to the global APT trust store.

## Remove the repository

```bash
sudo apt remove klicky
sudo rm -f /etc/apt/sources.list.d/klicky.sources
sudo rm -f /etc/apt/keyrings/klicky-archive-keyring.gpg
sudo apt update
```

User configuration and custom sound packs under `~/.config/klicky` are not removed.

## Maintainer setup

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

`.github/workflows/apt-repository.yml` runs when a GitHub release is published or through an explicit manual dispatch.

The workflow:

1. Downloads the release amd64 `.deb`.
2. Imports the encrypted signing subkey into an ephemeral GnuPG home.
3. Verifies the key and passphrase with a signing probe.
4. Rebuilds the complete repository from published packages.
5. Generates `InRelease`, `Release`, `Release.gpg`, and package indexes.
6. Exports only public key material.
7. Uploads only public repository files as a Pages artifact.
8. Deploys through GitHub's OIDC-backed Pages environment.

Pull requests never receive signing secrets and cannot publish the repository.

## Key rotation

Begin rotation before the current key expires:

1. Generate a new certification key and signing subkey.
2. Publish both old and new public keys during a transition release.
3. Update installation documentation with both fingerprints.
4. Sign repository metadata with the new key.
5. Release a keyring update before removing the old key.
6. Revoke the old key if it is compromised.

A routine expiry should use overlap. A compromise should use immediate revocation and a clearly announced replacement fingerprint.
