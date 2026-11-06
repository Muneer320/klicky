# AUR Submission Guide — Klicky

This document describes how to publish and maintain Klicky on the
[Arch User Repository (AUR)](https://aur.archlinux.org/).

## Prerequisites

1. **AUR account** — register at <https://aur.archlinux.org/register/>.
2. **SSH key** — add your public key to your AUR profile under
   *My Account → SSH Public Key*.
3. **Verify access**:

   `sh
   ssh -T aur@aur.archlinux.org
   # Expected: "Welcome to the AUR, <username>!"
   `

## First-time submission

The AUR uses a plain Git repository per package. Create it by pushing to a
repository named after the package (`klicky`):

`sh
# Clone the (empty) AUR remote — this creates the package automatically
git clone ssh://aur@aur.archlinux.org/klicky.git /tmp/klicky-aur
cd /tmp/klicky-aur

# Copy the three packaging files from this repository
cp <klicky-repo>/packaging/arch/PKGBUILD .
cp <klicky-repo>/packaging/arch/.SRCINFO .
cp <klicky-repo>/packaging/arch/klicky.install .

# Commit and push
git add PKGBUILD .SRCINFO klicky.install
git commit -m "feat: initial AUR submission — klicky 0.4.0"
git push
`

The package page will appear at https://aur.archlinux.org/packages/klicky
within a few minutes.

## Updating for a new release

1. Bump `pkgver` and `pkgrel` (reset to `1`) in `PKGBUILD`.
2. Update the `#tag=` in the `source` line to match the new tag.
3. Regenerate `.SRCINFO`:

   `sh
   makepkg --printsrcinfo > .SRCINFO
   `

4. Test the package locally:

   `sh
   makepkg -si
   `

5. Copy `PKGBUILD`, `.SRCINFO`, and `klicky.install` into the AUR clone and push.

## Files in this repository

| File | Purpose |
|------|---------|
| `packaging/arch/PKGBUILD` | Package recipe |
| `packaging/arch/.SRCINFO` | Machine-readable metadata (generated from PKGBUILD) |
| `packaging/arch/klicky.install` | Pre-upgrade/pre-remove hook |

> **Note:** The AUR repository must be flat — place all three files at its root,
> not in a subdirectory.

## Validation checklist

Before each AUR push, verify:

- [ ] `pkgver` and `pkgrel` match the release
- [ ] `source` tag matches the GitHub tag
- [ ] `.SRCINFO` was regenerated with `makepkg --printsrcinfo`
- [ ] `makepkg -si` succeeds in a clean environment
- [ ] `namcap PKGBUILD` reports no errors
