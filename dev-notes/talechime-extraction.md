# Talechime 独立仓库迁移

日期：2026-10-08。

听书项目从 TRNovel 拆为 [Talechime · 叙铃](https://github.com/yexiyue/talechime)，
本地独立检出位于 `../talechime`，阅读器通过 `vendor/talechime` Git 子模块固定源码版本。
原始来源为 TRNovel commit `cd8d71051840d063cdfb6f57bbaae4f387c6a3fa`；旧历史保留在原仓库，
新仓库用提取提交开始，并保留第三方来源、许可证和数值夹具。

## 范围

- novel-tts / core / backends / protocol 分别改包名为 talechime / talechime-core / talechime-backends / talechime-protocol。
- 模型计算库、Candle 平台层、VoxCPM 原生开发对照、TTS 工具和验收资料迁入独立 workspace。
- 原仓库的 1,642 个 TTS 源码、工具与夹具文件已逐一核对新仓库 Git 跟踪状态；MOSS 的 19 KiB Safetensors 是合成数值夹具，须与模型权重忽略规则分开。
- 根 workspace 只包含阅读器及 parse-book-source；Rust 协议依赖别名仍是 tts_protocol。
- 首次克隆及所有 Cargo CI 检出须初始化子模块；生成的 cargo-dist workflow 已使用 recursive submodules，未手工修改。
- 基础/听书/加速/musl 发行脚本从子模块单独构建 TTS，继续打包 novel-tts 兼容入口与所选组件许可证。
- 独立仓库提供品牌 PNG、README、架构/许可/开发说明、CPU CI、CUDA/Metal 编译 workflow 及独立制品脚本。

## 行为与兼容

新品牌入口 talechime 与 novel-tts 兼容入口运行相同源码。协议主版本仍为 5。
配置保留 `~/.novel/tts_config.json`，检查点保留 `~/.novel/tts/checkpoints/`，
模型、音色和缓存保留 `~/.novel-tts/`。不升级外部依赖、不重新下载、不迁移用户文件。

多角色连续朗读仍是后续任务。当前后端支持逐请求 voice/style，但会话生产者使用整章固定音色；
不能用逐句 update_config 替代朗读计划，因为它会取消会话并清空缓冲。

下次 TRNovel crates.io 发布前须先发布所需的 talechime-protocol 版本，Cargo 打包后的 version
依赖必须可解析。当前迁移不发布新的 crates.io 包或发行 tag。

## 验证

迁移验证涵盖独立工作区测试、示例、Clippy、严格 rustdoc、新旧 CLI 握手和只读状态；
阅读器测试、编译、依赖树边界、格式、Clippy、rustdoc 及文档站构建也分别检查。
工作区编译通过不代替新一轮真实模型音质或 GPU 性能验收。

发行 Python 语法和 workflow YAML 已检查；源代码提取和协议握手不会下载模型。
新仓库发布 workflow 仍需通过实际平台制品验收，README 只声明源码可用。
