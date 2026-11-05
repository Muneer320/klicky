# Install from the APT repository

For Ubuntu 24.04+ and compatible Debian-based **amd64** desktops:

```bash
curl -fsSL https://klicky.muneer320.tech/install.sh | bash
```

This verifies the repository's pinned signing key, adds a dedicated APT source, installs Klicky and starts its user service. Run it as your normal user; it asks for `sudo` when needed. Use a terminal with `curl`; setup installs GnuPG if needed. For future updates, stop Klicky before replacing its package:

```bash
systemctl --user stop klicky.service
sudo apt update
sudo apt install --only-upgrade klicky
systemctl --user start klicky.service
```

Other signed-in users must stop their packaged Klicky processes too. The package refuses to replace a running binary. The current package needs glibc 2.39+, `libasound2t64`, systemd and udev. On older systems or other architectures, [build from source](INSTALLATION.md#linux-from-source). The repository's `stable` channel is a Klicky channel, not a Debian release codename.

<details>
<summary><strong>Set up the repository manually</strong></summary>

Download and inspect the key:

```bash
curl -fsSL -o /tmp/klicky-archive-keyring.gpg \
  https://muneer320.github.io/klicky/klicky-archive-keyring.gpg
gpg --show-keys --with-fingerprint /tmp/klicky-archive-keyring.gpg
```

Confirm there is exactly one primary key, matching this fingerprint, before installing it:

```text
DBB6 7AE4 78D2 FFCE C663 7B55 9899 E554 D358 0D8F
```

```bash
sudo install -d -m 0755 /etc/apt/keyrings
sudo install -m 0644 /tmp/klicky-archive-keyring.gpg /etc/apt/keyrings/klicky-archive-keyring.gpg
curl -fsSL -o /tmp/klicky.sources https://muneer320.github.io/klicky/klicky.sources
sudo install -m 0644 /tmp/klicky.sources /etc/apt/sources.list.d/klicky.sources
sudo apt update
sudo apt install klicky
systemctl --user enable --now klicky.service
```

The Deb822 source restricts this repository to amd64 and its own key through `Signed-By`.

</details>

<details>
<summary><strong>Remove Klicky or remove the repository</strong></summary>

```bash
klicky service disable
sudo apt remove klicky
sudo rm -f /etc/apt/sources.list.d/klicky.sources
sudo rm -f /etc/apt/keyrings/klicky-archive-keyring.gpg
sudo apt update
```

Your configuration and custom sounds remain in `~/.config/klicky`.

</details>

[Full installation guide](INSTALLATION.md) · [Troubleshooting](TROUBLESHOOTING.md) · [Repository maintenance](MAINTAINING_APT.md)
