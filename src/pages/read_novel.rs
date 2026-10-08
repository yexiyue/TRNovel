use crate::{
    History,
    components::{KeyShortcutInfo, Loading, ShortcutInfoModal, WarningModal},
    errors::Errors,
    hooks::{DebounceOptions, UseDebounceEffect, UseInitState},
    keymap::{ReaderAction, display_keys},
    novel::{Novel, VolumeMarker},
};
use futures::FutureExt;
use ratatui::layout::Direction;
use ratatui_kit::prelude::*;
use ratatui_kit_keymap::UseKeymapHandler;
#[cfg(feature = "tts")]
mod follow;
mod search;
mod select_chapter;
pub use select_chapter::*;
mod read_content;
pub use read_content::*;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio::time::{Duration, sleep};
mod settings;
pub use settings::*;
#[cfg(feature = "tts")]
fn listening_panel(open: bool, is_editing: bool) -> AnyElement<'static> {
    element!(crate::tts::ui::TTSManager(open: open, is_editing: is_editing)).into_any()
}
#[cfg(not(feature = "tts"))]
fn listening_panel(_open: bool, _is_editing: bool) -> AnyElement<'static> {
    use ratatui::layout::Constraint;

    // An empty Fragment inherits default layout constraints and takes reader space.
    // Match the zero footprint of a closed Modal in the listening build.
    element!(View(width: Constraint::Length(0), height: Constraint::Length(0))).into_any()
}

/// 阅读页当前打开的设置浮层。两个面板都是同尺寸 `Modal` 且都监听 ←/→,
/// 同时打开会叠加渲染并争抢按键,故用一个状态表达「至多一个」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Panel {
    None,
    Tts,
    ReaderSettings,
}

#[component]
pub fn ReadNovel<T>(mut hooks: Hooks) -> impl Into<AnyElement<'static>>
where
    T: Novel + Send + Sync + Unpin + 'static,
{
    let route_state = hooks.use_route_state::<T::Args>();
    let history = *hooks.use_context::<State<Option<History>>>();
    let mut chapters = hooks.use_state(std::vec::Vec::new);
    let mut volumes = hooks.use_state(Vec::<VolumeMarker>::new);
    let mut current_chapter = hooks.use_state(|| 0usize);
    let mut content = hooks.use_state(String::default);
    let mut is_read_mode = hooks.use_state(|| false);
    let mut search = hooks.use_state(search::ChapterSearch::default);
    hooks.use_effect(
        || search.set(search::ChapterSearch::default()),
        (current_chapter.get(), is_read_mode.get()),
    );
    // 同层浮层用单一状态而非每个面板一个 bool:互斥由类型保证,不可能同时开两个,
    // 也不必在每个入口手写「关掉另一个」。信息浮层是叠在面板之上的层,保持独立。
    let mut panel = hooks.use_state(|| Panel::None);
    let (width, height) = hooks.use_terminal_size();

    let mut content_loading = hooks.use_state(|| false);
    let mut info_modal_open = hooks.use_state(|| false);
    let mut scroll_target = hooks.use_state(ScrollTarget::default);
    let follow_suspended = hooks.use_state(|| false);

    let (novel, loading, error) = hooks.use_init_state(async move {
        let args = route_state.as_ref().clone();

        tokio::spawn(async move {
            let mut res = T::init(args).await?;

            if res.get_chapters().is_none() {
                let (chapter_list, volume_list) = res.request_toc().await?;
                res.set_chapters(&chapter_list);
                res.set_volumes(volume_list);
            }

            chapters.set(
                res.get_chapters_names()?
                    .into_iter()
                    .map(ChapterName::from)
                    .collect(),
            );
            volumes.set(res.get_volumes().to_vec());

            current_chapter.set(res.current_chapter);
            content_loading.set(true);
            content.set(res.get_content().await?);
            content_loading.set(false);
            // 落盘的是单个 f64,进内存转成语义化的滚动目标(1.0 即「章末」的编码)。
            scroll_target.set(ScrollTarget::from_ratio(res.line_percent));

            Ok::<T, Errors>(res)
        })
        .await?
    });

    hooks.use_on_drop({
        let mut novel = novel.read().clone();
        let mut history = history.read().clone();

        move || {
            if let Some(novel) = novel.as_mut() {
                novel.line_percent = scroll_target.get().as_ratio();
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

    // 阅读偏好的唯一落盘点:设置面板与 v 键都只改 READER_DISPLAY atom,由这里防抖写盘。
    // 放页面级(而非 App 根)是因为该配置只在阅读页被修改,重渲染范围也就止于本子树。
    let reader_display = hooks.use_atom(&crate::state::READER_DISPLAY);
    #[cfg(feature = "tts")]
    {
        let book_id = novel.read().as_ref().map(|book| book.get_id());
        hooks.use_effect(
            move || {
                let mut state = follow_suspended;
                state.set(false);
            },
            book_id,
        );
        let enabled = reader_display.read().follow_tts;
        let mut previous = hooks.use_state(|| enabled);
        hooks.use_effect(
            move || {
                if enabled && !previous.get() {
                    let mut state = follow_suspended;
                    state.set(false);
                }
                previous.set(enabled);
            },
            enabled,
        );
    }
    // 记住已在盘上的值:防抖 effect 挂载帧必然触发一次,不比对就会在每次进阅读页时
    // 把刚 load 进来的配置原样写回一遍。
    let saved_display = hooks.use_state(|| crate::state::READER_DISPLAY.get());
    hooks.use_debounce_effect(
        move || {
            let current = *reader_display.read();
            if current == saved_display.get() {
                return;
            }
            // 同步 IO 若跑在渲染循环的任务上会卡住整个 UI(慢盘/网络盘尤其明显),
            // 配置是 Copy,挪进阻塞线程池的捕获成本为零。
            // 写盘失败(磁盘满/无权限)不打断阅读,只是不推进 saved_display —— 下次改动会连同
            // 这次一起重试;先设已存再写会把失败的那次永久吞掉。
            let mut saved_display = saved_display;
            tokio::task::spawn_blocking(move || {
                if current.save().is_ok() {
                    saved_display.set(current);
                }
            });
        },
        *reader_display.read(),
        DebounceOptions::default(),
    );
    // 卸载兜底:防抖 effect 的 future 存在 hook 里,组件卸载时是被 **drop 而非 flush** 的——
    // 没睡满 500ms 的那次改动永远不会落盘。而 READER_DISPLAY 是进程级 atom,重进阅读页时
    // `saved_display` 会用「已改但未落盘」的内存值初始化,首次比对即相等直接返回,这次改动
    // 在本进程内再也不会被写盘。故在此同步补一次(卸载路径阻塞一次写盘可接受,History 同样如此)。
    hooks.use_on_drop(move || {
        let current = *reader_display.read();
        if current != saved_display.get() {
            let _ = current.save();
        }
    });

    // 页面级 action(模式/浮层切换)在此分发;正文滚动等 action 由 ReadContent 处理。
    let reader_keymap = hooks.use_atom(&crate::state::KEYMAP).read().reader.clone();
    hooks.use_keymap_handler(
        EventScope::Current,
        EventPriority::Normal,
        reader_keymap.clone(),
        move |action, _key| match action {
            ReaderAction::ToggleReadMode => {
                is_read_mode.set(!is_read_mode.get());
                EventResult::Consumed
            }
            ReaderAction::ToggleInfo => {
                info_modal_open.set(!info_modal_open.get());
                EventResult::Consumed
            }
            ReaderAction::ToggleTts | ReaderAction::ToggleReaderSettings
                if !info_modal_open.get()
                    && (action != ReaderAction::ToggleTts || cfg!(feature = "tts")) =>
            {
                let target = if action == ReaderAction::ToggleTts {
                    Panel::Tts
                } else {
                    Panel::ReaderSettings
                };
                // 面板只在阅读模式(is_read_mode)渲染。若在章节选择模式按下入口键,直接切到
                // 阅读模式并打开,避免「状态翻转了却无 UI」的死输入,以及之后 Tab 进阅读模式时
                // 面板意外已开的状态错位。
                if is_read_mode.get() {
                    panel.set(if panel.get() == target {
                        Panel::None
                    } else {
                        target
                    });
                } else {
                    is_read_mode.set(true);
                    panel.set(target);
                }
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        },
    );

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

    // 全书边界:`on_prev`/`on_next` 在这两端只会静默 return,故下传给 ReadContent,
    // 让它别在第一章/最后一章提示「再按一次翻章」去承诺不存在的章节。
    let has_prev = current_chapter.get() > 0;
    let has_next = current_chapter.get() + 1 < chapters.read().len();

    let tts_panel = listening_panel(
        panel.get() == Panel::Tts,
        panel.get() == Panel::Tts && !info_modal_open.get(),
    );

    element!(Fragment {
        { if is_read_mode.get() {
            element!(View{
                ReadContent(
                    search: search,
                    is_scroll: panel.get() == Panel::None && !info_modal_open.get(),
                    width: width,
                    height: height,
                    content: content.read().clone(),
                    book_id: novel.read().as_ref().map(|book|book.get_id()).unwrap_or_default(),
                    chapter_index: current_chapter.get(),
                    chapter_name: chapter_name,
                    chapter_percent: chapter_percent,
                    is_loading: content_loading.get(),
                    has_prev: has_prev,
                    has_next: has_next,
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
                            content_loading.set(true);
                            current_chapter.set(new_chapter);
                            scroll_target.set(ScrollTarget::Ratio(0.0));
                        }
                    },
                    on_prev: move |is_scroll_top| {
                        // 已是第一章:无上一章可翻。
                        if current_chapter.get() == 0 {
                            return;
                        }
                        let new_chapter = current_chapter.get().saturating_sub(1);

                        if let Some(novel) = novel.write().as_mut() {
                            if let Err(e) = novel.set_chapter(new_chapter) {
                                error.write().replace(e);
                                return;
                            }
                            content_loading.set(true);
                            current_chapter.set(new_chapter);
                            // 顶部 ↑ 翻回上一章(is_scroll_top=true)→ 落到上一章末尾:承接向上连读,
                            // 也让误触跳到下一章后能原路 ↑ 找回原来读到的位置;
                            // 显式 ← / H 翻上一章 → 落到章首。
                            scroll_target.set(if is_scroll_top {
                                ScrollTarget::ChapterEnd
                            } else {
                                ScrollTarget::Ratio(0.0)
                            });
                        }
                    },
                    scroll_target: scroll_target,
                    follow_suspended: follow_suspended,
                )
                Fragment{ {tts_panel} }
                ReaderSettingsModal(
                    open: panel.get() == Panel::ReaderSettings,
                    is_editing: panel.get() == Panel::ReaderSettings && !info_modal_open.get(),
                )
                ShortcutInfoModal(
                    // 阅读页 action 的键名从 keymap 动态取(重绑后帮助随之更新);
                    // TTS/阅读设置面板内部键(组件自处理,未迁移)保持硬编码。
                    // 整张表只在浮层打开时构建 —— 它含十余次 `describe()` + 字符串拼接,
                    // 挂在 props 上会每帧求值一次,而绝大多数时间浮层是关着的。
                    key_shortcut_info: if !info_modal_open.get() {
                        KeyShortcutInfo::default()
                    } else {
                        let dk = |label: &str, action| (label.to_string(), display_keys(&reader_keymap, action));
                        let sk = |label: &str, keys: &str| (label.to_string(), keys.to_string());
                        if panel.get() == Panel::Tts {
                            KeyShortcutInfo(vec![
                                dk("切换章节选择模式", ReaderAction::ToggleReadMode),
                                dk("关闭TTS设置模式", ReaderAction::ToggleTts),
                                sk("上一项", "↑ / K"),
                                sk("下一项", "↓ / J"),
                                sk("启用模型/从头播放/释放资源（当前行）", "Enter"),
                                sk("取消下载", "Esc"),
                                sk("减小速度/音量", "← / H"),
                                sk("增大速度/音量", "→ / L"),
                                sk("切换自动播放", "← / →"),
                            ])
                        } else if panel.get() == Panel::ReaderSettings {
                            KeyShortcutInfo(vec![
                                dk("切换章节选择模式", ReaderAction::ToggleReadMode),
                                dk("关闭阅读设置", ReaderAction::ToggleReaderSettings),
                                sk("上一项", "↑ / K"),
                                sk("下一项", "↓ / J"),
                                sk("调小/关闭", "← / H"),
                                sk("调大/开启", "→ / L"),
                            ])
                        } else {
                            KeyShortcutInfo(vec![
                                dk("搜索当前章节正文", ReaderAction::SearchContent),
                                dk("下一个正文搜索结果", ReaderAction::NextSearchMatch),
                                dk("上一个正文搜索结果", ReaderAction::PrevSearchMatch),
                                dk("清除正文搜索", ReaderAction::ClearContentSearch),
                                dk("切换章节选择模式", ReaderAction::ToggleReadMode),
                                dk("隐藏/显示标题", ReaderAction::ToggleTitle),
                                dk("打开阅读设置", ReaderAction::ToggleReaderSettings),
                                dk("打开TTS设置模式", ReaderAction::ToggleTts),
                                dk("播放/暂停", ReaderAction::TogglePlay),
                                dk("回到朗读位置并恢复跟随", ReaderAction::FollowPlayback),
                                dk("增大音量", ReaderAction::VolumeUp),
                                dk("减小音量", ReaderAction::VolumeDown),
                                dk("向上滚动(章首连按翻上一章)", ReaderAction::ScrollUp),
                                dk("向下滚动(章末连按翻下一章)", ReaderAction::ScrollDown),
                                dk("上一章(章首)", ReaderAction::PrevChapter),
                                dk("下一章", ReaderAction::NextChapter),
                                dk("上一页", ReaderAction::PageUp),
                                dk("下一页", ReaderAction::PageDown),
                                dk("跳到开头", ReaderAction::GoTop),
                                dk("跳到结尾", ReaderAction::GoBottom),
                            ].into_iter().filter(|(_,keys)| !keys.is_empty()).collect())
                        }
                    },
                    open: info_modal_open.get(),
                )
            })
        }else{
            element!(View(flex_direction:Direction::Horizontal){
                SelectChapter(
                    is_editing: !info_modal_open.get(),
                    chapters: chapters.read().clone(),
                    volumes: volumes.read().clone(),
                    default_value: current_chapter.get(),
                    on_select: move |index| {
                        if let Some(novel) = novel.write().as_mut() {
                            if let Err(e)=novel.set_chapter(index){
                                error.write().replace(e);
                                return;
                            }
                            // 选中当前章只切换阅读视图：依赖章号的加载 effect 不会重新运行。
                            // 仅在切章时置 loading，避免已有正文被永久遮住。
                            if index != current_chapter.get() {
                                content_loading.set(true);
                                current_chapter.set(index);
                            }
                            // 与 on_next/on_prev 一样必须重置:否则新章沿用上一章的进度比例,
                            // 短章节里会直接落到「贴底之下」,前向键失效并误报章末。
                            scroll_target.set(ScrollTarget::Ratio(0.0));
                            is_read_mode.set(true);
                        };
                    },
                )
                ReadContent(
                    content: content.read().clone(),
                    book_id: novel.read().as_ref().map(|book|book.get_id()).unwrap_or_default(),
                    chapter_index: current_chapter.get(),
                    chapter_name: chapter_name,
                    chapter_percent: chapter_percent,
                    width: width / 2,
                    height: height,
                    is_loading: content_loading.get(),
                    scroll_target: scroll_target,
                    follow_suspended: follow_suspended,
                )
                ShortcutInfoModal(
                    // 目录导航键是 SelectChapter/TreeSelect 内部处理(未迁移),保持硬编码;
                    // 仅模式切换键动态取。同上:只在浮层打开时构建。
                    key_shortcut_info: if !info_modal_open.get() {
                        KeyShortcutInfo::default()
                    } else {
                        KeyShortcutInfo(vec![
                            ("切换阅读模式".to_string(), display_keys(&reader_keymap, ReaderAction::ToggleReadMode)),
                            ("选择上一章".to_string(), "↑ / K".to_string()),
                            ("选择下一章".to_string(), "↓ / J".to_string()),
                            ("确认选择章节".to_string(), "Enter".to_string()),
                            ("搜索章节".to_string(), "S".to_string()),
                        ])
                    },
                    open: info_modal_open.get(),
                )
            })
        } }
        WarningModal(
            tip: format!("加载失败:{:?}", error.read().as_ref()),
            is_error: error.read().is_some(),
            open: error.read().is_some(),
        )
    })
    .into_any()
}
