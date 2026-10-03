#!/usr/bin/env bash
set -euo pipefail

repo="Muneer320/klicky"
identity="Klicky Package Repository <muneer.alam320@gmail.com>"
key_home="${XDG_DATA_HOME:-$HOME/.local/share}/klicky/apt-signing"
gnupg_home="$key_home/gnupg"
backup_dir="$key_home/offline-backup"

for command in gpg gh; do
  if ! command -v "$command" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$command" >&2
    exit 1
  fi
done

if [[ $(gh api user --jq .login) != Muneer320 ]]; then
  printf 'GitHub CLI must be authenticated as Muneer320.\n' >&2
  exit 1
fi

if [[ -e $gnupg_home/private-keys-v1.d ]]; then
  printf 'Signing key directory already exists: %s\n' "$gnupg_home" >&2
  printf 'Refusing to replace existing key material.\n' >&2
  exit 1
fi

mkdir -p "$gnupg_home" "$backup_dir"
chmod 700 "$key_home" "$gnupg_home" "$backup_dir"
export GNUPGHOME="$gnupg_home"

printf 'GnuPG will request a new passphrase for the offline key.\n'
gpg --quick-generate-key "$identity" rsa4096 cert 2y

fingerprint=""
while IFS=: read -r record _ _ _ _ _ _ _ _ value _; do
  if [[ $record == fpr ]]; then
    fingerprint=$value
    break
  fi
done < <(gpg --batch --with-colons --list-secret-keys "$identity")

if [[ ! $fingerprint =~ ^[0-9A-F]{40}$ ]]; then
  printf 'Unable to resolve the generated key fingerprint.\n' >&2
  exit 1
fi

gpg --quick-add-key "$fingerprint" rsa4096 sign 2y

gpg --armor --export "$fingerprint" > "$backup_dir/klicky-archive-keyring.asc"
gpg --export "$fingerprint" > "$backup_dir/klicky-archive-keyring.gpg"
gpg --armor --export-secret-keys "$fingerprint" > "$backup_dir/klicky-primary-private.asc"
cp "$gnupg_home/openpgp-revocs.d/$fingerprint.rev" \
  "$backup_dir/klicky-revocation-certificate.rev"
chmod 600 "$backup_dir"/*

printf 'Re-enter the key passphrase for GitHub Actions: '
IFS= read -rs passphrase
printf '\n'
if [[ -z $passphrase ]]; then
  printf 'Passphrase cannot be empty.\n' >&2
  exit 1
fi

probe="$key_home/signing-probe"
printf 'Klicky package signing probe\n' > "$probe"
printf '%s' "$passphrase" | \
  gpg --batch --yes --pinentry-mode loopback --passphrase-fd 0 \
    --local-user "$fingerprint" --output "$probe.sig" --detach-sign "$probe"
gpg --batch --verify "$probe.sig" "$probe"
rm -f "$probe" "$probe.sig"

printf '%s' "$passphrase" | gh secret set APT_GPG_PASSPHRASE --repo "$repo"
unset passphrase
gpg --armor --export-secret-subkeys "$fingerprint" | \
  gh secret set APT_GPG_PRIVATE_KEY --repo "$repo"
gh secret set APT_GPG_FINGERPRINT --repo "$repo" --body "$fingerprint"

printf '\nAPT signing key configured.\n'
printf 'Fingerprint: %s\n' "$fingerprint"
printf 'Offline backup: %s\n' "$backup_dir"
printf '\nMove the offline backup to encrypted removable storage.\n'
printf 'Do not delete the backup until the public repository is verified.\n'
