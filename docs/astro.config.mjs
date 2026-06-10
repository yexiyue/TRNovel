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
        starlightThemeNova({
          nav: [
            {
              label: '文档', href: '/TRNovel/guides/intro/'
            },
            {
              label: "ratatui-kit", href: "https://yexiyue.github.io/ratatui-kit-website/"
            }
          ]
        }),
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
      components: {
        // 首页自定义 Hero(覆盖 Nova 主题的 Hero,仅 splash 页带 hero frontmatter 时渲染)
        Hero: "./src/components/landing/Hero.astro",
      },
      customCss: ["./src/styles/landing.css"],
      logo: {
        light: "./src/assets/trnovel-mark-light.svg",
        dark: "./src/assets/trnovel-mark-dark.svg",
        alt: "TRNovel",
      },
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
