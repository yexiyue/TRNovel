use crate::{ThemeConfig, state::THEME};
use ratatui_kit::{Hooks, UseAtom};

pub trait UseThemeConfig {
    fn use_theme_config(&mut self) -> ThemeConfig;
}

impl UseThemeConfig for Hooks<'_, '_> {
    /// 订阅全局 `THEME` 原子并返回当前主题快照。订阅使读它的组件在主题变更时重渲染。
    /// 因 `use_atom` 是注册 waker 的 hook,本方法取 `&mut self`(故调用方需 `mut hooks`)。
    fn use_theme_config(&mut self) -> ThemeConfig {
        self.use_atom(&THEME).read().clone()
    }
}
