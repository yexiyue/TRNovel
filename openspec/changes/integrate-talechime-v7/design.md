# Design

## Context

见 proposal.md。现有有界 actor、进程 transport、实例/会话过滤和父页输入层保留。v7 UpdateConfig 的模型/设备变更会结束 worker 会话，不能在浏览设置时发送。

## Goals / Non-Goals

以协议 crate 为唯一 worker 依赖；UI 与 native 推理隔离。不引入多角色、角色识别、预生成或新的对齐器。

## Decisions

- 阅读器 ChapterRequest 只携带来源/正文/恢复选项；actor 启动时生成固定单 VoiceSpan、sealed、Streaming 的 PlanRequest，避免 UI 构造执行配置。
- config.toml 的可选 tts.sound 保存阅读器的下一次声音偏好；worker 的配置仍通过协议读取/修改，不读取其 JSON 文件。相比立即 UpdateConfig，此设计不会打断当前声音，也不会丢失跨重启的待用选择。
- Snapshot 保留 worker config、待用 sound 和活动 playback_sound。仅准备或启动时提交待用声音；速度/音量/续章仍即时提交。模型改变后使用目标默认音色并清除不兼容风格/设备。
- SegmentFinished 和 GetProgress.played_end 作为完成边界，验证 UTF-8 且不倒退；显式应用前查询进度，停止旧 session，再按新快照准备/启动，暂停态随后恢复。不能用 SegmentStarted.end 或 generated_end 推断播放完成。
- 听书、声音、高级、列表在同一个面板中逐级进入；列表独占更高输入层，使用本地搜索框，避免全局 header 搜索与面板输入竞争。模型列表按 backend 排序分组，音色展示目录名称并支持 ID 搜索。

## Risks / Trade-offs

非 TUI 快速配置使用 clap 的 tts show/list/setup。show 只读取阅读器 ConfigStore；list/setup 复用 Client 的有界握手和关闭，只有 Hello/GetConfig/Shutdown，不准备、不播放、不提交 worker UpdateConfig。setup 仅保存阅读器待用声音；路径规范化为绝对路径，失败不写配置。模型切换使用目录默认音色并清除风格，不兼容设备回 Auto；显式无效值报错。保存事务中检查声音/路径未被并行修改，避免覆盖已验证快照以外的变化。

- 旧 worker 不兼容 → 提示独立升级 Talechime，普通阅读不受影响。
- 模型准备耗时 → 目录操作不下载，准备展示进度并可取消。
- native 音频与跨平台验收 → mock worker 验证协议/竞态，本机真实 worker 验证目录而不下载模型；真实试听不以编译结果代替。

## Migration Plan

升级协议依赖并更新文档。tts.sound 缺省从 worker 配置初始化；旧 alignment 未知字段不在 UI 暴露。回退应用代码可忽略新增 TOML 子节。用户在 2026-10-14 授权自动迁移后，正常启动在 clap 解析后显式运行 `migration.rs`，复制旧阅读数据并将偏好转换到 ConfigStore TOML；路径解析保持纯函数。成功标记和 clear 分类抑制标记分别控制重试与显式清理，不覆盖已有新配置，不删除旧数据。
