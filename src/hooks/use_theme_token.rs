use crate::ThemeConfig;
use ratatui_kit::{Hooks, State, UseContext};

// pub trait UseThemeToken {
//     fn use_theme_token(&self) -> Ref<'_, ThemeConfig>;
// }

// impl UseThemeToken for Hooks<'_, '_> {
//     fn use_theme_token(&self) -> Ref<'_, ThemeConfig> {
//         Ref::map(self.use_context::<State<ThemeConfig>>(), |s| {
//              这里的transmute使用是安全的，因为：
//              1. s.read() 返回的是一个 ReadGuard<'_, ThemeConfig>
//              2. ReadGuard 实现了 Deref<Target = ThemeConfig>，所以 s.read().deref() 返回 &ThemeConfig
//              3. transmute::<&ThemeConfig, &ThemeConfig> 在类型层面是完全相同的转换
//              4. 这实际上是一个 noop 操作，只是为了满足 Ref::map 的类型要求
//             unsafe { std::mem::transmute::<&ThemeConfig, &ThemeConfig>(&s.read()) }
//         })
//     }
// }

pub trait UseThemeConfig {
    fn use_theme_config(&self) -> ThemeConfig;
}

impl UseThemeConfig for Hooks<'_, '_> {
    fn use_theme_config(&self) -> ThemeConfig {
        self.try_use_context::<State<ThemeConfig>>()
            .map(|s| s.read().clone())
            .unwrap_or_default()
    }
}
