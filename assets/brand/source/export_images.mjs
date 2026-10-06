// Export brand formats without altering the generated artwork.
import sharp from "../../../docs/node_modules/sharp/lib/index.js";
import { fileURLToPath } from "node:url";

const brand = new URL("../", import.meta.url);
const file = (name) => fileURLToPath(new URL(name, brand));

await sharp(file("app-icon.svg"))
  .resize(512, 512)
  .png()
  .toFile(file("app-icon.png"));

await sharp(file("readme-cover.png"))
  .resize(1200, 600, { fit: "inside" })
  .png()
  .toFile(file("../../docs/public/brand/social-card.png"));
