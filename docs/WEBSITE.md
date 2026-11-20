# Website maintenance

Klicky's product site, field guide and signed APT archive share GitHub Pages at [klicky.muneer320.tech](https://klicky.muneer320.tech/). The archive also uses `https://muneer320.github.io/klicky/`.

## Source map

| Source | Purpose |
|---|---|
| [Landing template](../packaging/apt/index.html) | Mascot-led introduction, sound room, OS setup and archive links |
| [Field guide](../packaging/apt/guide.html) | On-site installation, CLI/configuration, architecture and troubleshooting |
| [Privacy policy](../packaging/apt/privacy.html) | Input handling, local data, diagnostics and hosting |
| [Styles](../assets/website/site.css) | Shared responsive light/dark design and reduced-motion behavior |
| [Small enhancement script](../assets/website/site.js) | Copy buttons and one-preview-at-a-time audio |
| [Website media](../assets/website) | Optimized illustrations, sound previews and locally hosted Nunito font/license |
| [Brand assets](../assets/branding) | Final mascot art, platform marks and social poster |
| [Archive builder](../scripts/build-apt-repository.py) | Signed archive, pages, media and install/remove endpoints |
| [Browser checker](../scripts/check-website.py) | Local preview, responsive checks, axe audit and screenshots |

There is no frontend framework, remote font service, account, analytics or global keyboard listener. The small script is progressive enhancement: commands remain visible and native audio/disclosures work without JavaScript. Audio never autoplays. The mascot's greeting is finite, with all animation disabled under reduced motion.

Keep the archive template markers `PUBLIC_URL`, `LATEST_VERSION`, `VERSIONS` and `FINGERPRINT`. The builder escapes dynamic metadata. Preserve relative links to `klicky.sources`, both signing-key formats and `dists/stable/InRelease`; the site must work at `/` and `/klicky/`.

`copy_site_assets` publishes the curated website/branding folders, field guide, privacy policy and four install/remove scripts. The preferred social image is `/assets/social-preview.jpg`; `/assets/social-preview.png` remains compatible with older links. Both are 1280 × 640 and under 1 MB. The builder publishes only the curated asset directories.

## Artwork

The [asset library](../assets/branding) contains the shared SVG/PNG marks, mascot illustrations, wordmarks and social poster. [Meet Klicky](CHARACTER.md) describes the character. Optimized site images live in `assets/website/mascot`; native application icons are committed under `assets/branding` and `packaging/windows`.

To regenerate PNG/ICO and macOS template icons after changing their SVG sources:

```sh
npm ci --prefix tools/branding --ignore-scripts
npm run --prefix tools/branding build
```

The pinned resvg exporter uses the colour mark for PNG/ICO and the monochrome mark for macOS templates. The branding workflow checks generated files against Git. Normal Cargo builds use the committed icons and do not require Node. Keep font and demo attributions with their assets; see [acknowledgements](../ACKNOWLEDGEMENTS.md).

## Preview and validate

```bash
python -B scripts/check-website.py --serve
```

Open the printed URL. It serves real pages/assets/scripts, not installers or placeholder package downloads. Stop with Ctrl+C.

```bash
python -B scripts/check-docs.py
python -B -m unittest scripts/test_apt_repository.py
```

For browser QA, install Python Playwright and Chromium once:

```bash
python -m pip install playwright
python -m playwright install chromium
python -B scripts/check-website.py
```

The checker exercises landing/guide at 320, 360, 768 and 1440px in light/dark mode: overflow, fragment links, keyboard focus, disclosures, real image loading, native audio and reduced motion. Screenshots go to ignored `target/website-review`. A local axe-core bundle at `target/website-review/axe.min.js` enables the WCAG audit. The tool is not shipped with the website.

To rebuild sound examples:

```bash
python tools/website/export-previews.py
```

Use `--ffmpeg /path/to/ffmpeg` if needed. Examples are offline sample sequences with the app's normalization target/gain cap, not physical typing measurements. No microphone recording is made.

## Publication

The Pages/APT workflow rebuilds the complete signed archive, verifies retained package history and keeps CNAME. Do not deploy the landing HTML alone or replace this with a generic static-site workflow. See [APT maintenance](MAINTAINING_APT.md).

The custom-domain CNAME points to `muneer320.github.io`; GitHub Pages must retain the same domain and HTTPS setting. Installation endpoints are `/install.sh` and `/install.ps1`; removal uses `/uninstall.sh` and `/uninstall.ps1`.

Publishing website changes does not publish application releases or update installed icons. Keep version/download wording aligned with real release assets. The APT release-event path skips prereleases, and candidate builds do not publish a release.

## Search metadata

The pages contain canonical URLs, a sitemap, crawlable links and SoftwareApplication/Person structured data. Keep these consistent with visible product and installation information. Use the shared social poster and relative asset paths so links work on both the custom domain and GitHub Pages.

Google's [search and AI-feature guidance](https://developers.google.com/search/docs/appearance/ai-features) covers crawlability and helpful content; no special AI schema is required. Keep ratings, performance figures and package-manager availability grounded in the released product.
