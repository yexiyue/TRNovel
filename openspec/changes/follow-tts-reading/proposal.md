# Follow TTS reading

## Why

听书已高亮实际播放范围，但窗口不会滚动，长章节的播放位置会移出屏幕。

## What Changes

默认开启并保存跟随朗读设置；实际播放片段进入底部两行或窗口外时滚动至上方三分之一。手动浏览或搜索暂时脱离，f 回到朗读位置并恢复。暂停、缓冲及模态面板期间不自动移动，自动续章保留跟随状态。

## Capabilities

### New Capabilities

- `reader-follow`: 阅读器跟随实际播放范围。

### Modified Capabilities

无。

## Impact

仅阅读器偏好、布局映射、键位与帮助；不修改 worker 协议。无 TTS 构建隐藏入口。
