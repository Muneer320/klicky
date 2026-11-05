# Website maintenance

Klicky's product page and signed APT archive share GitHub Pages. The public address is [klicky.muneer320.tech](https://klicky.muneer320.tech/); the archive also uses `https://muneer320.github.io/klicky/`.

## Where things live

| Source | Purpose |
|---|---|
| [HTML template](../packaging/apt/index.html) | Product content, styles and native disclosures |
| [Public media](../assets/website/audio) | Ten short sound-pack previews |
| [Preview exporter](../tools/website/export-previews.py) | Rebuild listening examples with FFmpeg |
| [Brand assets](../assets/branding) | Shared mark and social image |
| [Repository builder](../scripts/build-apt-repository.py) | Signed archive, page, media and installer entry points |
| [Preview/check tool](../scripts/check-website.py) | Local browser preview and responsive checks |

There is no frontend framework, JavaScript bundle, external font or analytics service. Audio uses native browser controls, loads on demand and never autoplays. The animated keycap is a silent conceptual illustration. Reduced-motion mode removes animation; light and dark appearances follow the system.

The builder escapes its four template markers: `PUBLIC_URL`, `LATEST_VERSION`, `VERSIONS` and `FINGERPRINT`. Keep the relative archive links intact: `klicky.sources`, both public-key formats and `dists/stable/InRelease`. They work under both the custom domain and the `/klicky/` project path.

`render_sources` owns the Deb822 definition. `copy_site_assets` publishes only the website asset directory, social PNG and the two installer scripts. Do not copy build outputs, planning files or unrelated repository directories into the public site.

## Preview and check

```bash
python -B scripts/check-website.py --serve
```

Open the printed URL. The preview serves the real template, media and installer script downloads. It does not run installers or create placeholder APT packages. Stop with Ctrl+C.

Basic checks:

```bash
python -B scripts/check-docs.py
python -B -m unittest scripts/test_apt_repository.py
```

For browser checks, install Python Playwright and Chromium once:

```bash
python -m pip install playwright
python -m playwright install chromium
python -B scripts/check-website.py
```

This checks phone/tablet/desktop layouts in both appearances, fragment links, focus, native disclosures, reduced motion and console errors. Screenshots go to ignored `target/website-review`. Supplying a local axe-core bundle at `target/website-review/axe.min.js` also enables its accessibility audit. The tool is never shipped with the site.

To rebuild sound examples:

```bash
python tools/website/export-previews.py
```

Use `--ffmpeg /path/to/ffmpeg` if needed. These are offline sample sequences, not physical typing measurements. The source clips remain in `sounds`; the examples use the same 85% normalization target, 6x gain ceiling and a demonstration volume of 80%. No recordings are made.

## Publishing safely

The Pages/APT workflow runs after reviewed changes to its listed master-branch paths. It verifies published package history, rebuilds the complete signed archive and retains `CNAME`. Do not deploy the HTML alone or replace the archive with a generic static-site workflow. See [APT maintenance](MAINTAINING_APT.md) for signing and recovery.

The custom-domain CNAME must point to `muneer320.github.io` at the domain's active DNS provider, with the same domain configured in GitHub Pages and HTTPS enabled.

The installer endpoints are `https://klicky.muneer320.tech/install.sh` and `/install.ps1`. Publishing these scripts does not publish application releases: the Windows helper needs an identified public installer/checksum pair. Keep download status and install instructions synchronized when a release is approved. The APT workflow skips prereleases, so a platform preview does not replace the stable Linux archive.
