# Klicky brand assets

Klicky's mark pairs a mechanical keycap with two sound pulses. The cutout is transparent so the mark works on light and dark surfaces. Keep the proportions and clear space; do not add glow or shadows to small application icons.

The master is [klicky.svg](../assets/branding/klicky.svg). The artwork is distributed under the project's [MIT license](../LICENSE).

| Asset | Use |
|---|---|
| [Vector mark](../assets/branding/klicky.svg) | Project pages, documentation and scalable application artwork |
| [512px PNG](../assets/branding/klicky.png) | Raster consumers |
| [Monochrome vector](../assets/branding/klicky-monochrome.svg) | Single-color interfaces; inherits `currentColor` |
| [Menu template](../assets/branding/klicky-menu-template.png) and [2x](../assets/branding/klicky-menu-template@2x.png) | macOS menu bar; AppKit applies the system appearance |
| [Social preview](../assets/branding/social-preview.png) and [vector master](../assets/branding/social-preview.svg) | 1280 × 640 repository and website sharing image; solid background, outlined lettering |
| [Windows ICO](../packaging/windows/klicky.ico) | Embedded executable, tray and installer identity; nine sizes from 16 to 256px |

## Regeneration

All platform exports come from the same SVG. Node.js and the pinned renderer are development tools only. Cargo builds consume the committed assets and do not invoke Node or download an image renderer.

```sh
npm ci --prefix tools/branding --ignore-scripts
npm run --prefix tools/branding build
```

The lockfile pins resvg-js 2.6.2 and platform packages. No system fonts, external images or network assets are needed to render the logo. Review the icon at small sizes on both light and dark backgrounds after changing the master. The macOS helper uses the monochrome image when installed and retains its keyboard-symbol fallback for older installations.

Source branding changes do not deploy the public website. Its Pages/APT publication workflow remains a separately reviewed operation; website consumers can use the shared SVG when that update is approved.

The social poster keeps its lettering as vector paths, so export does not depend on installed fonts. It is exported by the same locked renderer. Upload `assets/branding/social-preview.png` in GitHub repository Settings → Social preview when changing the poster. The website uses the PNG for Open Graph and Twitter cards.
