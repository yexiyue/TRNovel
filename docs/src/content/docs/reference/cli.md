---
title: CLI 参考
lastUpdated: 2025-10-29
sidebar:
    order: 1
---


TRNovel 提供了多种命令行选项来直接访问不同的功能模块。

## 基本用法

```bash
trnovel [OPTIONS] [SUBCOMMAND]
```

## 选项

### `-h, --help`

显示帮助信息

### `-V, --version`

显示版本信息

## 子命令

### `quick` (`-q`)

快速模式，接着上一次阅读的位置继续阅读

用法:

```bash
trnovel -q
```

### `clear` (`-c`)

清空历史记录和小说缓存

用法:

```bash
trnovel -c
```

### `network` (`-n`)

网络模式，使用网络小说源

用法:

```bash
trnovel -n
```

### `local` (`-l`)

本地小说模式，可选指定小说文件夹路径

用法:

```bash
trnovel -l [PATH]
```

参数:

- `PATH`: 小说文件夹路径（可选）

### `history` (`-H`)

历史记录模式，查看阅读记录

用法:

```bash
trnovel -H
```

### `doctor` (`-d`)

体检书源：对书源 JSON 跑完整流程（配置 / 浏览 / 书详情 / 目录 / 正文 / 搜索），逐项报告 ✓/✗/○。
用于校验 AI 生成的书源是否可用；被反爬挑战拦截的端点会给出精确提示而非笼统失败。

用法:

```bash
trnovel doctor <书源.json>
```

参数:

- `<书源.json>`: 待校验的书源 JSON 文件路径

### `import` (`-i`)

导入书源：把书源 JSON（本地文件或 URL）写入 `~/.novel/book_sources.json`，使其在网络小说（`-n`）里可直接选用。
按 `url` + `name` 去重（同名覆盖，便于反复迭代同一书源）。是「AI 生成 → `doctor` 校验 → 导入即用」闭环的最后一步。

用法:

```bash
trnovel import <书源.json>          # 本地文件
trnovel import https://…/source.json   # 也支持 URL
```

参数:

- `<source>`: 书源 JSON 文件路径或 URL
