# Microsoft Store packaging

Klicky's Store package is a Windows 11 x64, full-trust desktop MSIX. It uses the
same Rust input and audio engine as the EXE/ZIP build. The Store listing is not
public until Microsoft certifies it and publication is approved.

## Build

Use Windows x64, PowerShell 7, Git, the pinned Rust/MSVC toolchain, Visual Studio
C++ tools and a Windows SDK containing `makeappx.exe`. Commit the source first:
the portable archive and MSIX must identify the same source revision and tree.

```powershell
./scripts/build-windows-portable.ps1
./scripts/build-windows-msix.ps1 `
  -PortableArchive ./target/windows-dist/klicky-0.5.0-windows-x64.zip `
  -Development
```

Development packages use `Klicky.Development`, the display name **Klicky
Development**, and the CLI alias `klicky-dev.exe`. They are unsigned and are not
Store upload candidates. Building does not install a certificate, register the
package, enable startup or publish anything. CI validates the package and
round-trips its payload through MakeAppx without disabling semantic validation.

For a Store candidate, replace `-Development` with the exact `-IdentityName`,
`-Publisher` and `-PublisherDisplayName` values from **Partner Center → Klicky →
View product identity**. Do not use development identities for submission.
`-PackageVersion` accepts four numeric components; the last must be zero because
the Store reserves it. Keep the identity stable across upgrades.

The package includes all ten sound packs, copyright notices, dependency notices
and source archives required by the portable distribution. It also includes the
empty bundle lease marker because installed MSIX files are read-only.

## Runtime integration

The Start menu opens `klicky-desktop.exe`. The package declares a CLI execution
alias and a `KlickyStartup` login task. Startup is disabled initially; users can
enable it from the tray or CLI. Packaged copies use Windows' startup-task API
instead of creating a shortcut into a versioned WindowsApps directory. A task
disabled by the user or policy remains disabled until Windows permits it.

Existing EXE/ZIP installations continue using their owned startup shortcuts and
installer reader leases. They do not acquire package identity from an environment
variable or a file placed beside the executable.

## Qualification before submission

1. Validate the exact source build and package with MakeAppx and the Windows App
   Certification Kit. Review every applicable failure; do not suppress validation.
2. Test a packaged copy on Windows 11 x64: global input, physical typing and
   audible feedback, all packs, volume, pause/resume, audio recovery and lock/unlock.
3. Test CLI activation, login startup, Windows-disabled startup and tray state.
4. Check configuration and custom-pack paths under package virtualization,
   migration from an existing installation, and prevention of duplicate listeners.
5. Test an upgrade while running, package repair and removal, including settings
   retention/removal behavior. Development-layout registration alone does not
   prove installed MSIX lifecycle behavior.
6. Repeat existing Windows core/package checks and Linux/macOS CI checks.

Keep credentials, certificates, test recordings and account-specific submission
notes outside the repository. Store publication remains a separate approval step.

## Store submission

Prepare real application screenshots, a Store logo, support and privacy-policy
URLs, the age-rating questionnaire and accurate capability declarations. Explain
that keyboard input is used locally to play sounds, without storing typed text
or transmitting it in normal operation. Declare `runFullTrust`; do not add
unnecessary network, microphone, elevation or input-injection capabilities.

Microsoft signs MSIX packages after certification. That signature does not sign
the separate GitHub EXE/ZIP distribution or exempt binaries from malware scanning.
The existing unsigned downloads and their security guidance remain separate.

[MSIX packaging](https://learn.microsoft.com/en-us/windows/msix/package/create-app-package-with-makeappx-tool)
· [Store submissions](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/create-app-submission?pivots=store-installer-msix)
· [Signing options](https://learn.microsoft.com/windows/apps/package-and-deploy/code-signing-options)
