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
