# Design

ReaderDisplayConfig 增加 follow_tts，camelCase 持久化，serde 缺省 true。临时自由浏览标志由 ReadNovel 持有并传给正文，跨章节与视图切换保留，书籍身份变化清除。启用设置或 f 明确恢复。

复用 ContentLayout::match_line 映射原文 UTF-8 起始范围。目标为起始行减 floor(view/3)，钳制到 end_scroll；当前起始行在视口最后两行、上方或下方时触发，且目标不同才修改 scroll_target。只在匹配当前来源与正文摘要的 Playing 范围上自动跟随；缓冲/暂停/loading/目录预览/搜索编辑/面板隐藏正文输入时禁止。尺寸或段落间距变化重算映射。f 可以在暂停/缓冲时定位最后可靠播放范围，没有范围则等待下一次实际播放。

所有手动正文导航及搜索动作先标记自由浏览，包括章首/章末和翻章；自动续章不走该手动入口。搜索关闭不自动恢复。新增 ReaderAction::FollowPlayback，仅 TTS 构建绑定 f。面板增加跟随朗读条目且高度按编译入口数计算；帮助与底栏采用实际键位。
