use crate::{
    History,
    components::{Loading, ShortcutInfoModal, WarningModal},
    errors::Errors,
    hooks::UseInitState,
    novel::Novel,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use futures::FutureExt;
use ratatui::layout::Direction;
use ratatui_kit::prelude::*;
mod select_chapter;
pub use select_chapter::*;
mod read_content;
pub use read_content::*;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::{Duration, sleep};
mod tts;
pub use tts::*;

#[component]
pub fn ReadNovel<T>(mut hooks: Hooks) -> impl Into<AnyElement<'static>>
where
    T: Novel + Send + Sync + Unpin + 'static,
{
    let route_state = hooks.use_route_state::<T::Args>();
    let history = *hooks.use_context::<State<Option<History>>>();
    let mut chapters = hooks.use_state(std::vec::Vec::new);
    let mut current_chapter = hooks.use_state(|| 0usize);
    let mut content = hooks.use_state(String::default);
    let mut is_read_mode = hooks.use_state(|| false);
    let mut is_tts_open = hooks.use_state(|| false);
    let (width, height) = hooks.use_terminal_size();

    let mut content_loading = hooks.use_state(|| false);
    let mut info_modal_open = hooks.use_state(|| false);

    let (novel, loading, error) = hooks.use_init_state(async move {
        let args = route_state.as_ref().clone();

        tokio::spawn(async move {
            let mut res = T::init(args).await?;

            if res.get_chapters().is_none() {
                let chapters = res.request_chapters().await?;
                res.set_chapters(&chapters);
            }

            chapters.set(
                res.get_chapters_names()?
                    .into_iter()
                    .map(ChapterName::from)
                    .collect(),
            );

            current_chapter.set(res.current_chapter);
            content_loading.set(true);
            content.set(res.get_content().await?);
            content_loading.set(false);

            Ok::<T, Errors>(res)
        })
        .await?
    });

    let mut line_percent = hooks.use_state(|| {
        novel
            .read()
            .as_ref()
            .map(|n| n.line_percent)
            .unwrap_or_default()
    });

    hooks.use_on_drop({
        let mut novel = novel.read().clone();
        let mut history = history.read().clone();

        move || {
            if let Some(novel) = novel.as_mut() {
                novel.line_percent = line_percent.get();
                novel.current_chapter = current_chapter.get();

                if let Some(history) = history.as_mut() {
                    let history_item = novel.to_history_item().expect("to_history_item failed");
                    history.add(&novel.get_id(), history_item);
                    history.save().expect("save history failed");
                }
            }
        }
    });

    hooks.use_async_effect(
        async move {
            let notify = Arc::new(Notify::new());
            let notify_clone = notify.clone();
            // 启动定时器，200ms后如果还在加载则显示loading
            let show_loading_handle = tokio::spawn(async move {
                sleep(Duration::from_millis(200)).await;
                // 如果notify还没被唤醒，说明内容还在加载
                if notify_clone.notified().now_or_never().is_none() {
                    content_loading.set(true);
                }
            });

            let novel = novel.read().clone();
            let content_result = novel.map(|n| tokio::spawn(async move { n.get_content().await }));

            if let Some(fut) = content_result {
                match fut.await {
                    Ok(c) => match c {
                        Ok(c) => {
                            content.set(c);
                        }
                        Err(e) => {
                            error.write().replace(e);
                        }
                    },
                    Err(e) => {
                        error.write().replace(e.into());
                    }
                }
            }

            notify.notify_one();
            content_loading.set(false);
            let _ = show_loading_handle.await;
        },
        current_chapter.get(),
    );

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Tab => {
                    is_read_mode.set(!is_read_mode.get());
                }
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    info_modal_open.set(!info_modal_open.get());
                }
                KeyCode::Char('t') | KeyCode::Char('T') => {
                    is_tts_open.set(!is_tts_open.get());
                }
                _ => {}
            }
        }
    });

    if loading.get() {
        return element!(Loading(tip:"加载小说中...")).into_any();
    }

    let chapter_name = novel
        .read()
        .as_ref()
        .and_then(|n| n.get_current_chapter_name().ok())
        .unwrap_or_default();

    let chapter_percent = novel
        .read()
        .as_ref()
        .and_then(|n| n.chapter_percent().ok())
        .unwrap_or_default();

    element!(Fragment {
        #(if is_read_mode.get() {
            element!(View{
                ReadContent(
                    is_scroll: !is_tts_open.get(),
                    width: width,
                    height: height,
                    content: content.read().clone(),
                    chapter_name: chapter_name,
                    chapter_percent: chapter_percent,
                    is_loading: content_loading.get(),
                    on_next: move |_| {
                        let new_chapter=current_chapter.get() + 1;
                        if let Some(novel) = novel.write().as_mut() {
                            if new_chapter >= chapters.read().len() {
                                return;
                            }
                            if let Err(e) = novel.set_chapter(new_chapter) {
                                error.write().replace(e);
                                return;
                            }
                            current_chapter.set(new_chapter);
                            line_percent.set(0.0);
                        }
                    },
                    on_prev: move |is_scroll_top| {
                        if current_chapter.get() == 0 || is_scroll_top {
                            return;
                        }
                        let new_chapter = current_chapter.get().saturating_sub(1);

                        if let Some(novel) = novel.write().as_mut() {

                            if let Err(e) = novel.set_chapter(new_chapter) {
                                error.write().replace(e);
                                return;
                            }
                            current_chapter.set(new_chapter);
                            // if is_scroll_top {
                            //     line_percent.set(1.0);
                            // } else {
                            //     line_percent.set(0.0);
                            // }
                            line_percent.set(0.0);
                        }
                    },
                    line_percent: line_percent,
                )
                TTSManager(
                    open: is_tts_open.get(),
                    is_editing: is_tts_open.get() && !info_modal_open.get(),
                )
                ShortcutInfoModal(
                    key_shortcut_info: {
                        if is_tts_open.get() {
                            vec![
                                ("切换章节选择模式", "Tab"),
                                ("关闭TTS设置模式", "T"),
                                ("上一项", "↑ / K"),
                                ("下一项", "↓ / J"),
                                ("确认/开始下载", "Enter"),
                                ("取消下载", "Esc"),
                                ("减小速度/音量", "← / H"),
                                ("增大速度/音量", "→ / L"),
                                ("切换自动播放", "← / →"),
                            ]
                        } else {
                            vec![
                                ("切换章节选择模式", "Tab"),
                                ("打开TTS设置模式", "T"),
                                ("播放/暂停", "P"),
                                ("增大音量", "+"),
                                ("减小音量", "-"),
                                ("向上滚动", "↑ / K"),
                                ("向下滚动", "↓ / J"),
                                ("上一章", "← / H"),
                                ("下一章", "→ / L"),
                                ("上一页", "PageUp"),
                                ("下一页", "PageDown"),
                                ("跳到开头", "Home"),
                                ("跳到结尾", "End"),
                            ]
                        }
                    },
                    open: info_modal_open.get(),
                )
            })
        }else{
            element!(View(flex_direction:Direction::Horizontal){
                SelectChapter(
                    chapters: chapters.read().clone(),
                    default_value: current_chapter.get(),
                    on_select: move |index| {
                        if let Some(novel) = novel.write().as_mut() {
                            if let Err(e)=novel.set_chapter(index){
                                error.write().replace(e);
                                return;
                            }
                            current_chapter.set(index);
                            is_read_mode.set(true);
                        };
                    },
                )
                ReadContent(
                    content: content.read().clone(),
                    chapter_name: chapter_name,
                    chapter_percent: chapter_percent,
                    width: width / 2,
                    height: height,
                    is_loading: content_loading.get(),
                    line_percent: line_percent,
                )
                ShortcutInfoModal(
                    key_shortcut_info: vec![
                        ("切换阅读模式", "Tab"),
                        ("选择上一章", "↑ / K"),
                        ("选择下一章", "↓ / J"),
                        ("确认选择章节", "Enter"),
                        ("搜索章节", "S"),
                    ],
                    open: info_modal_open.get(),
                )
            })
        })
        WarningModal(
            tip: format!("加载失败:{:?}", error.read().as_ref()),
            is_error: error.read().is_some(),
            open: error.read().is_some(),
        )
    })
    .into_any()
}
