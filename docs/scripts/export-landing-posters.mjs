import sharp from "sharp";
import { mkdir } from "node:fs/promises";

// Chosen frames show usable interfaces after terminal launch and navigation.
const frames = {
  read: 180,
  "network-read": 540,
  theme: 140,
  login: 180,
  "local-select": 120,
};
const assets = new URL("../src/assets/", import.meta.url);
await mkdir(new URL("landing/", assets), { recursive: true });
for (const [name, page] of Object.entries(frames)) {
  await sharp(new URL(`guides/${name}.gif`, assets).pathname, { page })
    .webp({ quality: 90 })
    .toFile(new URL(`landing/${name}.webp`, assets).pathname);
}
