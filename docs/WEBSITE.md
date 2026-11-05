# Website maintenance

The public site and signed APT archive share `https://muneer320.github.io/klicky/`.
The custom domain [klicky.muneer320.tech](https://klicky.muneer320.tech/) serves the same Pages deployment. Its `klicky` CNAME must point to `muneer320.github.io` in the zone served by the domain's delegated nameservers; a record saved at an inactive DNS provider has no effect. The custom domain and HTTPS also need to be configured in GitHub Pages settings.
The page is rendered from [the HTML template](../packaging/apt/index.html) by
[the repository builder](../scripts/build-apt-repository.py). There is no frontend
framework, JavaScript bundle, external font, or production dependency to install.

## Design and behavior

The keycap, spring, and waveform are an original conceptual SVG illustration, not
a screenshot, measured key-travel plot, or audio demo. Native `details` elements
provide keyboard-accessible key and pack disclosures. CSS animates only the
requested keypress; reduced-motion mode removes animation and transitions.
The page supports light and dark color schemes.

The builder replaces `PUBLIC_URL`, `LATEST_VERSION`, `VERSIONS`, and `FINGERPRINT`
template markers, escaping dynamic metadata. Keep the four relative archive links
intact: `klicky.sources`, both public key formats, and `dists/stable/InRelease`.
Do not point them at the domain root: GitHub Pages serves them under `/klicky/`.

The builder's `render_sources` function is the authoritative Deb822 definition;
`klicky.sources` is generated into the publication output, not copied from a
second static file.

## Local checks

For an interactive local preview with Python alone:

```bash
python -B scripts/check-website.py --serve
```

Open the printed local `/klicky/` URL. Stop with Ctrl+C. The preview serves the
page only; use the published site to inspect archive downloads.

From the repository root:

```bash
PYTHONUTF8=1 python -B scripts/check-docs.py
PYTHONUTF8=1 python -B -m unittest scripts/test_apt_repository.py
```

On PowerShell, set `$env:PYTHONUTF8 = '1'` first and run the Python commands
without the environment prefix.

Optional browser checks require the Python Playwright package and its Chromium:

```bash
python -m pip install playwright
python -m playwright install chromium
python -B scripts/check-website.py
```

The browser check renders the actual template in memory and serves it locally at
`/klicky/`. It checks viewport overflow, fragment links, keyboard disclosure
controls, focus visibility, reduced motion, and console errors. Screenshots are
written under ignored `target/website-review/`. It does not sign an archive,
deploy anything, or substitute dummy archive downloads. Unit tests check the
archive-link contract; the Linux APT integration test checks the actual builder.

If a local axe-core browser bundle is supplied at
`target/website-review/axe.min.js`, the browser check also runs its WCAG A/AA
rules. The audit used axe-core 4.10.3. It is a local test tool only and is never
included in the published page. Automated checks do not replace screen-reader
and visual review.

For a formatting check without adding project dependencies:

```bash
npx --yes prettier@3.6.2 --check packaging/apt/index.html
```

There is no separate TypeScript, frontend build, or application lint task. The
production build is `build-apt-repository.py`; use the existing Linux integration
test with a real `.deb` when changing the template or builder.

## Publication is a separate action

Changing this template does not deploy the page. The existing APT workflow builds
and deploys the entire archive, including this page. See the
[APT publication guide](APT_REPOSITORY.md#publication) before an explicitly
approved deployment. Do not dispatch it merely to preview a design: it replaces
the public repository with the selected release's package.

At the next release, review the page's explicit v0.3.0 links, source-versus-release
callout, and installation guidance alongside the README. Dynamic package metadata
does not automatically update this editorial copy.

The live page and current template were compared on 5 November 2026 and matched after the four documented substitutions. The signed archive index advertises both `0.3.0-1` and `0.4.0-1`; the older editorial copy is separate from that index. Website editorial updates, including Windows preview links, require a reviewed Pages/APT publication. Updating platform Markdown does not authorize that production deployment.
