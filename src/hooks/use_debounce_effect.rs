use ratatui_kit::{Hooks, UseEffect, UseState};
use std::{hash::Hash, time::Duration};

/// 防抖选项配置结构体
///
/// 用于控制防抖行为的参数配置
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct DebounceOptions {
    /// 等待时间，在此期间内不会执行回调
    pub wait: Duration,
    /// 是否在开始时立即执行一次回调
    pub leading: bool,
    /// 是否在等待期结束后执行回调
    pub trailing: bool,
}

impl Default for DebounceOptions {
    fn default() -> Self {
        Self {
            wait: Duration::from_millis(500),
            leading: false,
            trailing: true,
        }
    }
}

impl DebounceOptions {
    /// 设置 trailing 模式为 true
    /// 在等待期结束后执行回调
    pub fn trailing(mut self) -> Self {
        self.trailing = true;
        self
    }

    /// 设置 leading 模式为 true
    /// 在开始时立即执行一次回调
    pub fn leading(mut self) -> Self {
        self.leading = true;
        self
    }

    /// 设置等待时间
    pub fn wait(mut self, wait: Duration) -> Self {
        self.wait = wait;
        self
    }
}

/// 防抖效果钩子 trait
///
/// 提供防抖功能的钩子接口，避免频繁触发回调函数
pub trait UseDebounceEffect {
    /// 使用防抖效果
    ///
    /// # 参数
    /// * `callback` - 需要防抖执行的回调函数
    /// * `deps` - 依赖项，当依赖项变化时重新设置防抖
    /// * `options` - 防抖选项配置
    fn use_debounce_effect<F, D>(&mut self, callback: F, deps: D, options: DebounceOptions)
    where
        F: FnMut() + Send + 'static,
        D: Hash;
}

impl UseDebounceEffect for Hooks<'_, '_> {
    fn use_debounce_effect<F, D>(&mut self, mut callback: F, deps: D, options: DebounceOptions)
    where
        F: FnMut() + Send + 'static,
        D: Hash,
    {
        // 跟踪是否已经执行过 leading 回调
        let mut has_leading = self.use_state(|| false);

        // 使用异步效果实现防抖逻辑
        self.use_async_effect(
            async move {
                // 如果启用了 leading 模式且尚未执行过 leading 回调
                if options.leading && !has_leading.get() {
                    has_leading.set(true);
                    callback();
                }
                // 等待指定的时间
                tokio::time::sleep(options.wait).await;
                // 如果启用了 trailing 模式，则执行回调
                if options.trailing {
                    callback();
                }
                // 重置 leading 状态
                has_leading.set(false);
            },
            (deps, options),
        );
    }
}
