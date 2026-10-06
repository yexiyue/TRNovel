# 听书窗口跟随验收（2026-10-06）

## 行为与边界

阅读偏好 `followTts` 默认 true，旧配置缺少字段也为 true，继续使用 READER_DISPLAY atom 的页面级防抖保存。跟随暂停状态由 ReadNovel 页面持有，正文与目录预览共享，自动续章和暂停恢复不会重置；打开另一书按已保存偏好初始化。

仅使用当前请求对应的实际播放范围，将 UTF-8 起点映射到现有 ContentLayout 显示行。起点进入视口最后两行或位于视口外，才定位到视口上方约三分之一，目标不超过章末。长片段仅使用起点，未估计句中位置。此变更不增加 worker 协议。

手动滚动、翻页、章首/章末、手动切章和正文搜索暂停跟随。配置键 `follow_playback` 默认 f，恢复时开启偏好并清除暂停状态；已有播放范围即使 Paused/Buffering 也可显式定位，没有范围则等待。自动滚动必须处于 Playing，且正文可操作、无加载和搜索输入；浮层与目录预览不可触发。底栏显示实际配置键。

## 自动检查

- workspace all-features lib/tests/examples：287 项通过。
- Clippy all-targets/all-features warnings deny、rustfmt、rustdoc warnings deny 通过。
- basic 无 TTS 构建和 51 项 reader 测试通过。
- 文档 19 页构建通过；OpenSpec strict 校验通过。
- 新测试覆盖安全区域、底部/视口外触发、显式定位、章末钳制和小窗口；UTF-8/中文/英文/CRLF/换行/段落间距在不同宽度下使用真实 ContentLayout；旧偏好缺字段与显式 false 往返；默认键与重映射、无 TTS 隐藏键。

日志位于 `target/follow-{tests,clippy,rustdoc,no-tts,basic-tests,basic-build,docs-build}.log`。这些检查包含此前未提交的 Qwen 与缓冲变更，没有为本次功能调整模型或运行库。

## VHS 与实际播放

M4 Pro，真实 release Qwen/Metal worker、福叔音色、对齐关闭，真实声卡消费；隔离 HOME，复用已校验模型和设备校准缓存。使用自编两章短书，避免第三方文本与旧检查点影响。

`target/follow-acceptance/follow.gif` 检查手动翻页后的“自由浏览”、f 恢复后的“跟随中”、搜索的自由浏览提示和自动进入第二章；真实播放检查点同步推进。`settings.gif` 检查“跟随朗读：开”，`basic.gif` 检查无 TTS 设置仅三项、正文保持完整高度。对应 PNG 已逐张检查。

`follow-small.gif` 使用 1000×420 窗口再次录制：手动翻页为 16/18 行且显示自由浏览；f 恢复至 9/18 行，使“山风轻拂”高亮可见；正文搜索“树影”定位 18/18 行并保持自由浏览；恢复跟随后实际朗读推进到“鸟鸣清脆”，窗口滚动至 18/18 行，锚点位于上方约三分之一（章末受到钳制）；暂停定位序列之后恢复播放，自动进入第二章。小窗口底栏保留完整的跟随/恢复提示，较长的速度与缓冲信息会在左侧截断，故不能仅凭这组静态截图读取暂停状态。

可复用脚本为 `docs/tapes/follow-tts.tape`，准备方式见同目录 README。首次下载和设备校准必须先完成；其他硬件需要根据实际状态调整等待时间。

人工听音与屏幕逐句对应仍待用户确认。VHS 与播放事件证明范围驱动的滚动和可见高亮，不能证明模型逐字完整性，也不声称长片段内部已有逐句跟随。
