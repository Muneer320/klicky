import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve, dirname } from 'node:path';
import { Resvg } from '@resvg/resvg-js';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const brand = resolve(root, 'assets/branding');
const source = readFileSync(resolve(brand, 'klicky.svg'), 'utf8');
const mono = source.replaceAll('#2563EB', '#000000');
const render = (svg, size) => new Resvg(svg, {
  fitTo: { mode: 'width', value: size },
  font: { loadSystemFonts: false },
}).render().asPng();
writeFileSync(resolve(brand, 'klicky.png'), render(source, 512));
writeFileSync(resolve(brand, 'social-preview.png'), render(readFileSync(resolve(brand, 'social-preview.svg'), 'utf8'), 1280));
writeFileSync(resolve(brand, 'klicky-monochrome.svg'), mono.replaceAll('#000000', 'currentColor'));
writeFileSync(resolve(brand, 'klicky-menu-template.png'), render(mono, 18));
writeFileSync(resolve(brand, 'klicky-menu-template@2x.png'), render(mono, 36));
const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const images = sizes.map(size => render(source, size));
const directory = Buffer.alloc(6 + sizes.length * 16);
directory.writeUInt16LE(1, 2);
directory.writeUInt16LE(sizes.length, 4);
let offset = directory.length;
sizes.forEach((size, i) => {
  const entry = 6 + i * 16;
  directory[entry] = size % 256;
  directory[entry + 1] = size % 256;
  directory.writeUInt16LE(1, entry + 4);
  directory.writeUInt16LE(32, entry + 6);
  directory.writeUInt32LE(images[i].length, entry + 8);
  directory.writeUInt32LE(offset, entry + 12);
  offset += images[i].length;
});
writeFileSync(resolve(root, 'packaging/windows/klicky.ico'), Buffer.concat([directory, ...images]));
console.log('Exported shared PNG, monochrome menu assets and nine-size Windows ICO from klicky.svg.');
