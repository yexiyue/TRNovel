use super::{BLANK_ROW, GLOBAL_HINT_ROW, TOC_ROW, adjust_config, describe_setting};
use crate::ReaderDisplayConfig;
use crate::components::settings::{Control, Row};
pub fn rows(config: &ReaderDisplayConfig) -> Vec<Row<usize>> {
    let mut fields = vec![
        (4, "排版", "0 表示跟随窗口宽度"),
        (5, "排版", "正文左右各留白的终端列数"),
        (3, "排版", "只缩进段落第一行，不改原文"),
        (2, "排版", "段落之间额外补充的空行数"),
        (6, "排版", "选择阅读正文边框样式"),
        (TOC_ROW, "排版", "目录栏占阅读区域的比例"),
        (0, "翻页", "翻页时保留上一屏末尾的行数"),
        (9, "翻页", "方向键按整页翻动，j / k 仍逐行移动"),
        (BLANK_ROW, "翻页", "关闭时末页贴底，不产生多余空白"),
        (1, "界面", "控制界面标题，不删除 TXT 原文中的标题"),
        (7, "界面", "开启时将标题放在边框上，并显示边框听书状态"),
        (8, "界面", "显示阅读页提示与设置页操作说明"),
        (
            GLOBAL_HINT_ROW,
            "界面",
            "显示其它页面最底部的返回、菜单与退出提示",
        ),
    ];
    if cfg!(feature = "tts") {
        fields.push((10, "听书", "跟随实际播放位置自动滚动正文"));
    }
    fields
        .into_iter()
        .map(|(id, group, detail)| {
            let (label, value) = describe_setting(config, id, 0);
            let control = match id {
                0 | 2 | 3 | 4 | 5 => {
                    let mut lower = *config;
                    adjust_config(&mut lower, id, -1);
                    let mut upper = *config;
                    adjust_config(&mut upper, id, 1);
                    Control::Number {
                        decrease: lower != *config,
                        increase: upper != *config,
                    }
                }
                6 | TOC_ROW => Control::Choice,
                _ => Control::Toggle(match id {
                    1 => config.show_title,
                    7 => config.show_frameline_title,
                    8 => config.show_hint_bar,
                    9 => config.arrows_page,
                    GLOBAL_HINT_ROW => config.show_global_hint,
                    BLANK_ROW => config.allow_page_blank,
                    _ => config.follow_tts,
                }),
            };
            let value = value.split('(').next().unwrap_or(&value);
            row(
                group,
                id,
                label.trim_end_matches(':'),
                value,
                detail,
                control,
            )
        })
        .collect()
}

fn row(
    group: &'static str,
    key: usize,
    label: &str,
    value: &str,
    detail: &str,
    control: Control,
) -> Row<usize> {
    Row {
        group,
        key,
        label: label.to_owned(),
        value: value.to_owned(),
        detail: detail.to_owned(),
        control,
    }
}
