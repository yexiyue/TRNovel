mod download;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use download::*;
use novel_tts::{CheckpointModel, NovelTTS, VoicesData};
use ratatui::{
    layout::{Constraint, Margin},
    style::Style,
    text::Line,
    widgets::Block,
};
use ratatui_kit::prelude::*;
mod settings;
use crate::theme::AppChromeTheme;
pub use settings::*;
mod voice_select;
pub use voice_select::*;

/// 面板条目数(索引上界):检查点模型 / 语音数据 / 音色 / 速度 / 音量 / 自动播放。
/// 新增条目时同步改这里与下方渲染的索引判断。
const ITEM_COUNT: usize = 6;

#[derive(Props, Default)]
pub struct TTSManagerProps {
    pub open: bool,
    pub is_editing: bool,
}

#[component]
pub fn TTSManager(props: &TTSManagerProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let checkpoint_model = hooks.use_state(CheckpointModel::default);
    let voices_data = hooks.use_state(VoicesData::default);

    let mut novel_tts = hooks.use_atom(&crate::state::NOVEL_TTS);
    let is_editing = props.is_editing;

    hooks.use_async_effect(
        {
            let checkpoint_model = checkpoint_model.read().clone();
            let voices_data = voices_data.read().clone();
            async move {
                if checkpoint_model.is_downloaded()
                    && voices_data.is_downloaded()
                    && novel_tts.read().is_none()
                {
                    let tts = NovelTTS::new(&checkpoint_model, &voices_data).await.ok();
                    novel_tts.set(tts);
                }
            }
        },
        (
            checkpoint_model.read().is_downloaded(),
            voices_data.read().is_downloaded(),
            novel_tts.read().is_none(),
        ),
    );

    let theme = hooks.use_component_theme::<AppChromeTheme>();

    let mut index = hooks.use_state(|| 0usize);
    let is_open = props.open;
    // 面板自己管焦点,滚动只负责跟随焦点(见下方 ScrollView 的 active: false)。
    let scroll_state = hooks.use_state(ScrollViewState::default);

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        if !is_open || !is_editing {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                index.set((index.get() + 1).min(ITEM_COUNT - 1));
                EventResult::Consumed
            }
            KeyCode::Char('k') | KeyCode::Up => {
                index.set(index.get().saturating_sub(1));
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    // 焦点移动后把选中项滚进视口——这是 ScrollView 为「选中项联动滚动」提供的 primitive,
    // 用它换掉内置滚动后,面板在终端过矮、条目超出一屏时依然能看到当前项。
    // deps 带上终端高度:视口变矮后 offset 只会被钳位、不会把焦点项拉回可见区,
    // 不重算的话焦点会滚出屏幕(界面上看不到任何高亮项)。
    let (_, term_height) = hooks.use_terminal_size();
    hooks.use_effect(
        move || scroll_state.write().scroll_to_index(index.get()),
        (index.get(), term_height),
    );

    element!(Modal(
        width: Constraint::Percentage(80),
        height: Constraint::Percentage(80),
        open: is_open,
        // 非阻塞浮层:本面板的 j/k(本组件 root handler)、T/Tab/i(父 ReadNovel root handler)都需在
        // TTS 开启时仍可用;子设置项的 h/l/Enter 在 Modal 子树层。背景 ReadContent 已用
        // `is_scroll = panel == Panel::None` 门控、面板开时不抢键,故非阻塞不引入冲突。
        // 默认 blocks_lower=true 会截断 root → j/k 失灵、T 关不掉 TTS。
        blocks_lower: false,
        margin: Margin::new(1, 1),
        style:Style::default().dim(),
    ) {
        View(margin:Margin::new(1,1)){
            ScrollView(
                block: Block::bordered().border_style(theme.border.not_dim()).title_top(Line::from("听书设置").centered().style(theme.title)),
                // 必须关掉内置键鼠滚动:它以 Current/Normal 注册在本组件子树里,而
                // `ScrollViewState::handle_event` 对 j/k/h/l 是「match 命中即 Consumed」——
                // 无论能否真的滚动都会吞键,于是面板的焦点导航(j/k)与各条目的调值(h/l)、
                // 下载确认全部收不到事件。视口改由上面的 scroll_to_index 跟随焦点。
                active: false,
                state: scroll_state,
            ){
                View(height:Constraint::Length(3)){
                    DownloadProgress::<CheckpointModel>(..DownloadProgressProps {
                        title: "检查点模型".to_string(),
                        state: checkpoint_model,
                        is_editing: index.get() == 0 && is_editing,
                    })
                }
                View(height:Constraint::Length(3)){
                    DownloadProgress::<VoicesData>(..DownloadProgressProps {
                        title: "语音数据".to_string(),
                        state: voices_data,
                        is_editing: index.get() == 1 && is_editing,
                    })
                }
                View(height:Constraint::Length(3)){
                    VoiceSelect(
                        is_editing: index.get() == 2 && is_editing,
                    )
                }
                View(height:Constraint::Length(3)){
                    SpeedSetting(
                        is_editing: index.get() == 3 && is_editing,
                    )
                }
                View(height:Constraint::Length(3)){
                    VolumeSetting(
                        is_editing: index.get() == 4 && is_editing,
                    )
                }
                View(height:Constraint::Length(3)){
                    AutoPlaySetting(
                        is_editing: index.get() == 5 && is_editing,
                    )
                }
            }

        }

    })
}
