# TRNovel 文档站

基于 Astro/Starlight，发布到 [yexiyue.github.io/TRNovel](https://yexiyue.github.io/TRNovel)。

在 `docs/` 中使用 `package.json` 指定的 pnpm 版本：

```bash
pnpm install
pnpm dev
pnpm build
pnpm preview
```

页面位于 `src/content/docs/`，图片位于 `src/assets/`，静态文件位于 `public/`。站点配置在 `astro.config.mjs`，录屏维护说明见 [tapes/README.md](tapes/README.md)。

推送到 main 后，`.github/workflows/docs.yaml` 构建并部署 GitHub Pages；也可手动触发。
