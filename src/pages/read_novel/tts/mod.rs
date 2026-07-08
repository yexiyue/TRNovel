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
                index.set((index.get() + 1).min(5));
                EventResult::Consumed
            }
            KeyCode::Char('k') | KeyCode::Up => {
                index.set(index.get().saturating_sub(1));
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    element!(Modal(
        width: Constraint::Percentage(80),
        height: Constraint::Percentage(80),
        open: is_open,
        // 非阻塞浮层:本面板的 j/k(本组件 root handler)、T/Tab/i(父 ReadNovel root handler)都需在
        // TTS 开启时仍可用;子设置项的 h/l/Enter 在 Modal 子树层。背景 ReadContent 已用 `is_scroll =
        // !is_tts_open` 门控、TTS 开时不抢键,故非阻塞不引入冲突。默认 blocks_lower=true 会截断 root →
        // j/k 失灵、T 关不掉 TTS。
        blocks_lower: false,
        margin: Margin::new(1, 1),
        style:Style::default().dim(),
    ) {
        View(margin:Margin::new(1,1)){
            ScrollView(
                block: Block::bordered().border_style(theme.border.not_dim()).title_top(Line::from("听书设置").centered().style(theme.title)),
                active: is_editing,
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
