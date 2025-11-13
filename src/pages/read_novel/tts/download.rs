use crate::{hooks::UseThemeConfig, pages::read_novel::SettingItem, utils::format_bytes};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use novel_tts::NovelTTSError;
use ratatui::{text::Line, widgets::Gauge};
use ratatui_kit::prelude::*;
use std::ops::DerefMut;

#[derive(Props)]
pub struct DownloadProgressProps<T>
where
    T: DerefMut<Target = novel_tts::download::Download> + Sync + Send + 'static,
{
    pub title: String,
    pub state: State<T>,
    pub is_editing: bool,
}

#[component]
pub fn DownloadProgress<T>(
    props: &DownloadProgressProps<T>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>>
where
    T: DerefMut<Target = novel_tts::download::Download> + Sync + Send + 'static,
{
    let state = props.state;
    let theme = hooks.use_theme_config();
    let mut downloading = hooks.use_state(|| false);
    let is_downloaded = state.read().is_downloaded();
    let mut progress = hooks.use_state(|| (0usize, 0usize));
    let mut error = hooks.use_state(|| None::<novel_tts::NovelTTSError>);
    let is_editing = props.is_editing;

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
            && is_editing
        {
            match key.code {
                KeyCode::Enter => {
                    if !is_downloaded && !downloading.get() {
                        error.set(None);
                        downloading.set(true);
                        state.write().download(
                            move |downloaded, total| {
                                progress.set((downloaded as usize, total as usize));
                                if downloaded == total {
                                    downloading.set(false);
                                }
                            },
                            move |err| {
                                match err {
                                    NovelTTSError::Cancel(_) => {}
                                    _ => {
                                        error.set(Some(err));
                                    }
                                }
                                downloading.set(false);
                            },
                        );
                    }
                }
                KeyCode::Esc => {
                    state.write().cancel_download();
                }
                _ => {}
            }
        }
    });

    let top_title = if is_downloaded {
        format!("{} - 已下载", props.title)
    } else if !downloading.get() {
        format!("{} - 未下载", props.title)
    } else {
        format!("{} - 下载中", props.title)
    };

    let element = if is_downloaded {
        element!(Fragment{
            $Line::from(format!("文件地址: {}",state.read().path.display()))
        })
    } else if let Some(err) = &*error.read() {
        element!(Fragment{
            $Line::from(format!("下载失败, 按Enter重新开始下载, Error:{}",err)).style(theme.colors.error_color)
        })
    } else if !downloading.get() {
        element!(Fragment{
            $Line::from("未下载, 按Enter开始下载")
        })
    } else {
        let progress = progress.get();
        let gauge = Gauge::default()
            .ratio(if progress.1 == 0 {
                0.0
            } else {
                (progress.0 as f64 / progress.1 as f64).clamp(0.0, 1.0)
            })
            .label(format!(
                "{}/{}",
                format_bytes(progress.0),
                format_bytes(progress.1)
            ))
            .gauge_style(theme.highlight);
        element!(Fragment{
            $gauge
        })
    };

    element!(SettingItem(
        is_editing: is_editing,
        top_title: top_title,
    ){
        #(element)
    })
}
