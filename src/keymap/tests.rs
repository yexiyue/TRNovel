use super::*;
use ratatui_kit_keymap::crokey::key;

/// 默认表必须能构建(无冲突、键位全部合法)—— build 内有断言,构建成功即验证。
#[test]
fn reader_defaults_build_and_match_legacy_keys() {
    let km = reader_defaults();
    // 逐键抽查与迁移前硬编码一致的代表键。
    assert_eq!(km.action_for(key!(k)), Some(ReaderAction::ScrollUp));
    assert_eq!(km.action_for(key!(down)), Some(ReaderAction::ScrollDown));
    assert_eq!(km.action_for(key!(pagedown)), Some(ReaderAction::PageDown));
    assert_eq!(km.action_for(key!(home)), Some(ReaderAction::GoTop));
    assert_eq!(km.action_for(key!('+')), Some(ReaderAction::VolumeUp));
    assert_eq!(km.action_for(key!('-')), Some(ReaderAction::VolumeDown));
    assert_eq!(km.action_for(key!(tab)), Some(ReaderAction::ToggleReadMode));
    // i/I、t/T 大小写双绑定(I = shift-i)。
    assert_eq!(km.action_for(key!(i)), Some(ReaderAction::ToggleInfo));
    assert_eq!(
        km.action_for(key!(shift - i)),
        Some(ReaderAction::ToggleInfo)
    );
    assert_eq!(
        km.action_for(key!(shift - t)),
        Some(ReaderAction::ToggleTts)
    );
}

/// 用户覆盖经 [reader] 表合并后生效,未覆盖的 action 不受影响。
#[test]
fn reader_override_merges() {
    let mut km = reader_defaults();
    let table: ratatui_kit_keymap::toml::Table =
        ratatui_kit_keymap::toml::from_str("page_down = [\"ctrl-d\"]").unwrap();
    let warnings = km.merge_toml_table(table);
    assert!(warnings.is_empty());
    assert_eq!(km.action_for(key!(ctrl - d)), Some(ReaderAction::PageDown));
    assert_eq!(km.action_for(key!(pagedown)), None);
    assert_eq!(km.action_for(key!(k)), Some(ReaderAction::ScrollUp));
}

/// 显示层沿用「↑ / K」视觉习惯:方向键箭头、单字符大写、覆盖后显示新键。
#[test]
fn display_keys_follow_project_style() {
    let mut km = reader_defaults();
    assert_eq!(display_keys(&km, ReaderAction::ScrollUp), "↑ / K");
    assert_eq!(display_keys(&km, ReaderAction::VolumeDown), "-");
    // t 与 shift-t 双绑定折叠为单个「T」(同迁移前帮助显示)。
    assert_eq!(display_keys(&km, ReaderAction::ToggleTts), "T");
    let table: ratatui_kit_keymap::toml::Table =
        ratatui_kit_keymap::toml::from_str("page_down = [\"ctrl-d\"]").unwrap();
    km.merge_toml_table(table);
    assert_eq!(display_keys(&km, ReaderAction::PageDown), "Ctrl-d");
}

/// 告警渲染为中文且逐条对应。
#[test]
fn warnings_render_in_chinese() {
    let mut km = reader_defaults();
    let table: ratatui_kit_keymap::toml::Table =
        ratatui_kit_keymap::toml::from_str("page_down = \"ctrl-\"\ntypo = \"x\"\ntoggle_play = 3")
            .unwrap();
    let messages: Vec<String> = km
        .merge_toml_table(table)
        .iter()
        .map(render_warning)
        .collect();
    assert_eq!(messages.len(), 3);
    assert!(messages.iter().any(|m| m.contains("无法解析")));
    assert!(messages.iter().any(|m| m.contains("未知操作")));
    assert!(messages.iter().any(|m| m.contains("值类型不对")));
    // 全部回退/忽略后默认键完好。
    assert_eq!(km.action_for(key!(pagedown)), Some(ReaderAction::PageDown));
}
