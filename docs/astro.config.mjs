// @ts-check

import starlight from "@astrojs/starlight";
import { defineConfig } from "astro/config";
import icon from "astro-icon";
import starlightSidebarTopicsPlugin from "starlight-sidebar-topics";
import starlightThemeNova from "starlight-theme-nova";

// https://astro.build/config
export default defineConfig({
  base: "/TRNovel",
  integrations: [
    starlight({
      locales: {
        root: {
          label: "简体中文",
          lang: "zh-CN",
        }
      },
      plugins: [
        starlightThemeNova(),
        starlightSidebarTopicsPlugin([
          {
            label: "指南",
            link: "/guides/intro",
            icon: "open-book",
            items: [
              { label: "使用说明", autogenerate: { directory: "guides" } },
              { label: "参考", autogenerate: { directory: "reference" } },
            ],
          },
          {
            label: "书源参考",
            link: "/book-source/intro",
            icon: "document",
            items: [
              {
                label: "书源参考",
                autogenerate: {
                  directory: "book-source",
                },
              },
            ],
          },
        ]),
      ],
      title: "TRNovel",
      social: [
        {
          icon: "github",
          label: "GitHub",
          href: "https://github.com/yexiyue/TRNovel",
        },
      ],
    }),
    icon(),
  ],
  image: {
    service: {
      entrypoint: 'astro/assets/services/sharp',
      config: {
        limitInputPixels: false,
      },
    }
  }
});
