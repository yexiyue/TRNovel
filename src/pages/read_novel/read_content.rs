use super::search::{ChapterSearch, ContentLayout};
use crate::{
    components::Loading,
    hooks::UseScrollbar,
    keymap::{ReaderAction, display_first_key},
    theme::ReaderTheme,
};
use ratatui::{
    layout::{Constraint, Direction, Flex, Margin},
    text::Line,
    widgets::Paragraph,
};
#[cfg(test)]
use ratatui::{style::Style, text::Span};
use ratatui_kit::prelude::*;
use ratatui_kit_keymap::UseKeymapHandler;
use std::sync::Arc;
use std::time::Duration;

/// 章节边界的「再按一次」确认态,防止读到章末/章首时误触 ↓/↑ 直接跳章。
/// 到边界的首次 ↓/↑ 只武装并在底部状态栏提示,连续第二次才真正翻章;
/// 任何滚动/翻页/显式翻章键都会解除武装。
///
/// `AtFirst`/`AtLast` 是**全书边界**(无上/下一章可翻):只提示、不武装,再按也不翻章
/// —— 否则会承诺一个不存在的章节,而第二次按下只能静默 no-op。
#[derive(Clone, Copy, PartialEq)]
enum Edge {
    None,
    Prev,
    Next,
    AtFirst,
    AtLast,
}

/// 正文可见行数:组件高度扣掉上下边框与底部状态栏共 3 行,至少 1 行。
///
/// 翻页步长、章末判定与设置面板里展示的「每页滚动 M 行」都由它派生,故只此一处定义。
pub fn visible_lines(height: u16) -> usize {
    (height as usize).saturating_sub(3).max(1)
}

/// 章内滚动位置。**意图**与**比例**是两回事,故用变体而非在 f64 里塞哨兵值:
///
/// - `Ratio(r)` —— 精确位置,`r = current_line / total`。分母是内容总行数、与视口无关,
///   所以换终端尺寸/字号后同一个值仍指向同一行(这正是它取代旧的
///   `current_line / (total - view)` 的原因)。解析时**钳到贴底位置**,保证任何视口下都合法。
/// - `ChapterEnd` —— 「读到本章末尾」这个**意图**(End 键、顶部 ↑ 翻回上一章都发它),
///   由 `ReadContent` 在拿到 `total`/`view` 后解析成贴底位置。用 `Ratio(1.0)` 表达会
///   得到越界的 `total`,钳到 `total - 1` 则只剩最后一行加满屏空白。
/// - `Overscroll` —— 末尾留白(翻页跨过贴底位置,见 issue #63)。**这是视口局部的视觉状态**:
///   留白多少行由当时的 `total`/`view` 决定,视口一变就失去意义,故记下产生它的 `end_scroll`,
///   不匹配时退化为贴底。
///
/// 落盘仍是单个 f64(`ChapterEnd` 编码为 1.0,`Overscroll` 存其比例),转换只发生在持久化边界。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollTarget {
    Ratio(f64),
    ChapterEnd,
    Overscroll { ratio: f64, end_scroll: usize },
}

impl ScrollTarget {
    /// 从落盘的百分比还原。1.0 是 `ChapterEnd` 的编码,其余是精确比例。
    /// 不还原 `Overscroll`——留白是视口局部状态,重开时回到合法位置即可。
    pub fn from_ratio(ratio: f64) -> Self {
        if ratio >= 1.0 {
            Self::ChapterEnd
        } else {
            Self::Ratio(ratio)
        }
    }

    /// 落盘用的百分比。
    pub fn as_ratio(self) -> f64 {
        match self {
            Self::Ratio(ratio) | Self::Overscroll { ratio, .. } => ratio,
            Self::ChapterEnd => 1.0,
        }
    }

    /// 解析成实际行偏移。`end_scroll` 是贴底位置(`total - view`)。
    ///
    /// **除留白外一律钳到 `end_scroll`**:`Ratio` 是与视口无关的比例,换终端尺寸后
    /// `ratio * total` 可能落在贴底位置之下——那样屏幕会停在「最后一屏再往下」,
    /// 前向滚动/翻页当场失效(`current_line < end_scroll` 恒假)并误报章末,连按还会跳章。
    fn resolve(self, total: usize, end_scroll: usize) -> usize {
        let line_of = |ratio: f64| (ratio * total as f64).round() as usize;
        match self {
            Self::ChapterEnd => end_scroll,
            Self::Ratio(ratio) => line_of(ratio).min(end_scroll),
            // 仅在产生它的那个视口里保留留白;视口变了留白量已无意义 → 贴底。
            Self::Overscroll {
                ratio,
                end_scroll: at,
            } if at == end_scroll => line_of(ratio).min(total.saturating_sub(1)),
            Self::Overscroll { .. } => end_scroll,
        }
    }
}

impl Default for ScrollTarget {
    fn default() -> Self {
        Self::Ratio(0.0)
    }
}

#[derive(Default, Props)]
pub struct ReadContentProps {
    pub content: String,
    pub book_id: String,
    pub chapter_index: usize,
    pub search: Option<State<ChapterSearch>>,
    pub is_scroll: bool,
    pub is_loading: bool,
    pub width: u16,
    pub height: u16,
    pub on_prev: Handler<'static, bool>,
    pub on_next: Handler<'static, ()>,
    pub chapter_name: String,
    pub chapter_percent: f64,
    pub scroll_target: Option<State<ScrollTarget>>,
    pub follow_suspended: Option<State<bool>>,
    /// 全书是否还有上一章/下一章。边界提示与「再按一次」武装都要看它,
    /// 否则会在第一章/最后一章承诺不存在的章节(`on_prev`/`on_next` 那时只会静默 no-op)。
    pub has_prev: bool,
    pub has_next: bool,
}

#[component]
pub fn ReadContent(
    props: &mut ReadContentProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<ReaderTheme>();
    let mut reader_display = hooks.use_atom(&crate::state::READER_DISPLAY);
    let mut on_prev = props.on_prev.take();
    let mut on_next = props.on_next.take();
    let mut edge = hooks.use_state(|| Edge::None);
    #[cfg(feature = "tts")]
    let listening = hooks.use_context::<crate::tts::TtsContext>().clone();
    #[cfg(feature = "tts")]
    let request = hooks.use_memo(
        || {
            Arc::new(tts_protocol::StartRequest {
                source: tts_protocol::SourceId {
                    namespace: "reader".into(),
                    book: props.book_id.clone(),
                    chapter: props.chapter_index.to_string(),
                },
                text_hash: tts_protocol::text_hash(&props.content),
                text: props.content.clone(),
                resume_byte: None,
                restore_checkpoint: true,
            })
        },
        (
            props.content.clone(),
            props.book_id.clone(),
            props.chapter_index,
        ),
    );
    #[cfg(feature = "tts")]
    let snapshot = listening.snapshot.read().clone();
    #[cfg(feature = "tts")]
    let mut seen_terminal = hooks.use_state(|| snapshot.terminal_revision);
    #[cfg(feature = "tts")]
    let mut continue_source = hooks.use_state(|| None::<tts_protocol::SourceId>);
    #[cfg(feature = "tts")]
    if seen_terminal.get() != snapshot.terminal_revision {
        seen_terminal.set(snapshot.terminal_revision);
        if snapshot.should_continue(&request, props.has_next) {
            let mut next = request.source.clone();
            next.chapter = (props.chapter_index + 1).to_string();
            continue_source.set(Some(next));
            on_next(());
        }
    }
    #[cfg(feature = "tts")]
    hooks.use_effect(
        {
            let handle = listening.handle.clone();
            let request = request.clone();
            let loading = props.is_loading;
            move || {
                if loading {
                    handle.stop();
                } else {
                    let play = continue_source.read().as_ref() == Some(&request.source);
                    continue_source.set(None);
                    handle.chapter((*request).clone(), play);
                }
            }
        },
        (
            props.content.clone(),
            props.book_id.clone(),
            props.chapter_index,
            props.is_loading,
        ),
    );
    #[cfg(feature = "tts")]
    hooks.use_on_drop({
        let handle = listening.handle.clone();
        move || handle.stop()
    });
    #[cfg(feature = "tts")]
    let is_listening = snapshot.matches(&request)
        && matches!(
            snapshot.state,
            tts_protocol::SessionState::Playing
                | tts_protocol::SessionState::Buffering
                | tts_protocol::SessionState::Generating
                | tts_protocol::SessionState::Paused
        );
    #[cfg(not(feature = "tts"))]
    let is_listening = false;

    // 先取成值再进闭包:memo 的 deps 必须是值,传 atom 句柄比较的是「当前值跟自己比」,
    // 恒等 → 切换开关后排版会冻结在首帧。
    let paragraph_spacing = reader_display.read().paragraph_spacing;

    let local_search = hooks.use_state(ChapterSearch::default);
    let mut search = props.search.unwrap_or(local_search);
    hooks.use_effect(
        || {
            let query = search.read().query.clone();
            if !query.is_empty() {
                search.write().submit(&props.content, query);
            }
        },
        props.content.clone(),
    );
    let editing = search.read().editing && props.is_scroll && !props.is_loading;
    let layout = hooks.use_memo(
        || {
            Arc::new(ContentLayout::new(
                &props.content,
                props.width.saturating_sub(2) as usize,
                paragraph_spacing,
            ))
        },
        (props.content.clone(), props.width, paragraph_spacing),
    );
    let matches = search.read().matches.clone();
    let selected = search.read().selected;
    #[cfg(feature = "tts")]
    let tts_range = snapshot
        .range
        .filter(|range| snapshot.matches(&request) && range.is_valid(&props.content))
        .map(|range| range.start..range.end);
    #[cfg(not(feature = "tts"))]
    let tts_range: Option<std::ops::Range<usize>> = None;
    let paragraph = hooks.use_memo(
        || {
            TextParagraph::from(Paragraph::new(
                layout.styled(
                    &matches,
                    selected,
                    theme.search_highlight,
                    tts_range
                        .as_ref()
                        .filter(|_| is_listening)
                        .map(|range| (range, theme.tts_highlight)),
                ),
            ))
        },
        (
            props.content.clone(),
            props.width,
            paragraph_spacing,
            search.read().revision,
            selected,
            tts_range.clone(),
            is_listening,
            theme.search_highlight,
            theme.tts_highlight,
        ),
    );

    let scroll_target = hooks.use_state(ScrollTarget::default);
    let mut scroll_target = props.scroll_target.unwrap_or(scroll_target);

    #[cfg(feature = "tts")]
    let mut follow_suspended = {
        let local = hooks.use_state(|| false);
        props.follow_suspended.unwrap_or(local)
    };
    let is_scroll = props.is_scroll;
    // 滚动坐标系:以「内容总行数」为唯一绝对基准,三个量各司其职——
    //   total      正文总行数,落盘进度的分母(与视口无关,换终端尺寸后仍指向同一行)
    //   view       可见行数(height - 3:上下边框 + 底部状态栏)
    //   end_scroll 贴底位置(最后一行正好落在屏幕底部)= 章末判定线
    // 旧实现只有一个 `line_count`(实为 end_scroll)同时兼任滚动上限、进度分母与行号分母,
    // 分母含视口高度 → 进度随终端尺寸漂移,且末尾无法留白。
    let total = paragraph.line_count(props.width.saturating_sub(2));
    let view = visible_lines(props.height.saturating_sub(u16::from(editing)));
    let end_scroll = total.saturating_sub(view);

    hooks.use_effect(
        || {
            edge.set(Edge::None);
            if !editing && let Some(target) = matches.get(selected) {
                let line = layout.match_line(target).min(end_scroll);
                scroll_target.set(ScrollTarget::Ratio(line as f64 / total.max(1) as f64));
            }
        },
        (
            search.read().revision,
            props.width,
            paragraph_spacing,
            props.height,
        ),
    );

    let mut current_line = hooks.use_memo(
        || scroll_target.get().resolve(total, end_scroll),
        (total, view, scroll_target.get()),
    );
    #[cfg(feature = "tts")]
    {
        let enabled = reader_display.read().follow_tts;
        let deps = (
            (
                enabled,
                follow_suspended.get(),
                is_scroll,
                editing,
                props.is_loading,
            ),
            (
                snapshot.state,
                tts_range.clone(),
                props.width,
                props.height,
                paragraph_spacing,
            ),
            (total, view, scroll_target.get()),
        );
        hooks.use_effect(
            || {
                if enabled
                    && !follow_suspended.get()
                    && is_scroll
                    && !editing
                    && !props.is_loading
                    && snapshot.state == tts_protocol::SessionState::Playing
                    && let Some(range) = &tts_range
                    && let Some(line) = super::follow::target(
                        layout.match_line(range),
                        current_line,
                        view,
                        end_scroll,
                        false,
                    )
                {
                    scroll_target.set(ScrollTarget::Ratio(line as f64 / total.max(1) as f64));
                    edge.set(Edge::None);
                }
            },
            deps,
        );
    }
    let mut current_time = hooks.use_state(String::default);

    hooks.use_future(async move {
        current_time.set(chrono::Local::now().format("%H:%M").to_string());
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    // 滚动条的 content_length 要传**最大滚动位置**而非总行数:`use_scrollbar` 内部把两个
    // 入参同除以组件高度,传 total 会让 position 恒小于 content_len、滑块永远到不了底。
    hooks.use_scrollbar(end_scroll, Some(current_line));

    let can_search = props.is_scroll && !props.is_loading;
    let layer = hooks.use_input_layer(editing && can_search, true);
    let input_content = props.content.clone();
    hooks.use_event_handler(
        EventScope::Layer(layer),
        EventPriority::High,
        move |event| {
            if !can_search || !search.read().editing {
                return EventResult::Ignored;
            }
            search.write().handle_input(&event, &input_content);
            EventResult::Consumed
        },
    );

    let has_prev = props.has_prev;
    let has_next = props.has_next;
    // 翻页步长:整屏减去用户配置的重叠行数,让上一屏末尾的若干行留在新一屏开头作视觉锚点
    // (issue #63)。公式在 ReaderDisplayConfig 上,设置面板展示「每页滚动 M 行」时调同一个。
    let step = reader_display.read().page_step(view);
    // 按语义 action 分发(键位可经 ~/.novel/keybindings.toml 自定义);
    // 页面级 action(模式/浮层切换)不在本组件处理,Ignored 交给上层。
    let reader_keymap = hooks.use_atom(&crate::state::KEYMAP).read().reader.clone();
    #[cfg(feature = "tts")]
    let playback_request = request.clone();
    #[cfg(feature = "tts")]
    let playback_line = tts_range.as_ref().map(|range| layout.match_line(range));
    hooks.use_keymap_handler(
        EventScope::Current,
        EventPriority::Normal,
        reader_keymap.clone(),
        move |action, _key| {
            if !is_scroll {
                return EventResult::Ignored;
            }
            #[cfg(feature = "tts")]
            if matches!(
                action,
                ReaderAction::ScrollUp
                    | ReaderAction::ScrollDown
                    | ReaderAction::PageUp
                    | ReaderAction::PageDown
                    | ReaderAction::GoTop
                    | ReaderAction::GoBottom
                    | ReaderAction::PrevChapter
                    | ReaderAction::NextChapter
                    | ReaderAction::SearchContent
                    | ReaderAction::NextSearchMatch
                    | ReaderAction::PrevSearchMatch
            ) {
                follow_suspended.set(true);
            }
            // 逐行滚动与整页翻页只差这一个步长,边界行为(全书边界只提示 / 章内边界
            // 「再按一次」武装 / 二次确认翻章)完全一致,故各自合并为一支单一实现:
            // 分开写就是把边界语义复制两份,将来加第三种步长(半页)还要再复制一次。
            let delta = match action {
                ReaderAction::PageUp | ReaderAction::PageDown => step,
                _ => 1,
            };
            match action {
                ReaderAction::SearchContent if can_search => {
                    let query = search.read().query.clone();
                    let mut state = search.write();
                    state.draft = tui_input::Input::new(query);
                    state.editing = true;
                    EventResult::Consumed
                }
                ReaderAction::NextSearchMatch | ReaderAction::PrevSearchMatch if can_search => {
                    search
                        .write()
                        .advance(action == ReaderAction::PrevSearchMatch);
                    EventResult::Consumed
                }
                ReaderAction::ClearContentSearch if can_search => {
                    search.set(ChapterSearch::default());
                    EventResult::Consumed
                }
                ReaderAction::ScrollUp | ReaderAction::PageUp => {
                    if current_line > 0 {
                        current_line = current_line.saturating_sub(delta);
                        scroll_target.set(ScrollTarget::Ratio(current_line as f64 / total as f64));
                        edge.set(Edge::None);
                    } else if !has_prev {
                        // 全书第一章的章首:没有上一章可翻,只提示、不武装(再按也不会翻)。
                        edge.set(Edge::AtFirst);
                    } else if edge.get() == Edge::Prev {
                        // 已在章首且已武装 → 第二次 ↑ 才翻上一章(落到上一章末尾)。
                        edge.set(Edge::None);
                        on_prev(true);
                    } else {
                        // 首次到章首:只武装并在底部提示,不翻章(防误触)。
                        edge.set(Edge::Prev);
                    }
                    EventResult::Consumed
                }
                ReaderAction::ScrollDown | ReaderAction::PageDown => {
                    // 章末判定线是 end_scroll(最后一行已可见)。翻页可以跨过它落进
                    // 「末尾留白」区,保证每次步长绝对一致;落点上界
                    // (end_scroll-1)+step ≤ total-1,天然不越界,无需额外钳位。
                    // 逐行滚动 delta=1 永远停在 end_scroll,贴底行为与旧版逐帧一致。
                    if current_line < end_scroll {
                        current_line += delta;
                        let ratio = current_line as f64 / total as f64;
                        // 跨过贴底位置 = 进入末尾留白,记下当时的 end_scroll:留白量是视口相关的,
                        // 换终端尺寸后该位置要退化为贴底而不是原样保留(否则前向键会失效)。
                        scroll_target.set(if current_line > end_scroll {
                            ScrollTarget::Overscroll { ratio, end_scroll }
                        } else {
                            ScrollTarget::Ratio(ratio)
                        });
                        edge.set(Edge::None);
                    } else if !has_next {
                        // 全书最后一章的章末:没有下一章可翻,只提示、不武装(再按也不会翻)。
                        edge.set(Edge::AtLast);
                    } else if edge.get() == Edge::Next {
                        // 已在章末且已武装 → 第二次 ↓ 才翻下一章。
                        edge.set(Edge::None);
                        on_next(());
                    } else {
                        // 首次到章末:只武装并在底部提示,不翻章(防误触)。
                        edge.set(Edge::Next);
                    }
                    EventResult::Consumed
                }
                ReaderAction::PrevChapter => {
                    edge.set(Edge::None);
                    on_prev(false);
                    EventResult::Consumed
                }
                ReaderAction::NextChapter => {
                    edge.set(Edge::None);
                    on_next(());
                    EventResult::Consumed
                }
                ReaderAction::GoTop => {
                    scroll_target.set(ScrollTarget::Ratio(0.0));
                    edge.set(Edge::None);
                    EventResult::Consumed
                }
                ReaderAction::GoBottom => {
                    scroll_target.set(ScrollTarget::ChapterEnd);
                    edge.set(Edge::None);
                    EventResult::Consumed
                }
                #[cfg(feature = "tts")]
                ReaderAction::VolumeUp | ReaderAction::VolumeDown => {
                    if let Some(config) = listening.snapshot.read().config.as_ref() {
                        let delta = if action == ReaderAction::VolumeUp {
                            0.1
                        } else {
                            -0.1
                        };
                        listening.handle.update(tts_protocol::ConfigPatch {
                            volume: Some((config.volume + delta).clamp(0.0, 10.0)),
                            ..Default::default()
                        });
                    }
                    EventResult::Consumed
                }
                #[cfg(feature = "tts")]
                ReaderAction::FollowPlayback if can_search => {
                    reader_display.write().follow_tts = true;
                    follow_suspended.set(false);
                    if let Some(line) = playback_line.and_then(|line| {
                        super::follow::target(line, current_line, view, end_scroll, true)
                    }) {
                        scroll_target.set(ScrollTarget::Ratio(line as f64 / total.max(1) as f64));
                    }
                    edge.set(Edge::None);
                    EventResult::Consumed
                }
                #[cfg(feature = "tts")]
                ReaderAction::TogglePlay if can_search => {
                    listening.handle.toggle((*playback_request).clone());
                    EventResult::Consumed
                }
                ReaderAction::ToggleTitle => {
                    let mut display = *reader_display.read();
                    display.show_title = !display.show_title;
                    // 只改 atom;落盘由 `ReadNovel` 的防抖 effect 统一负责(单一存盘点,
                    // 与设置面板走同一条路径,避免两套写盘时序)。
                    reader_display.set(display);
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        },
    );

    let footer_status = match edge.get() {
        Edge::Next => format!(
            "● 已到本章末尾 · 再按 {} 进入下一章",
            display_first_key(&reader_keymap, ReaderAction::ScrollDown)
        ),
        Edge::Prev => format!(
            "● 已到本章开头 · 再按 {} 返回上一章",
            display_first_key(&reader_keymap, ReaderAction::ScrollUp)
        ),
        Edge::AtLast => "● 已是全书最后一章".to_string(),
        Edge::AtFirst => "● 已是第一章".to_string(),
        Edge::None => {
            let state = search.read();
            if state.query.is_empty() || props.width < 110 {
                state.status()
            } else {
                format!(
                    "{} · {} / {} 切换 · {} 清除",
                    state.status(),
                    display_first_key(&reader_keymap, ReaderAction::NextSearchMatch),
                    display_first_key(&reader_keymap, ReaderAction::PrevSearchMatch),
                    display_first_key(&reader_keymap, ReaderAction::ClearContentSearch)
                )
            }
        }
    };

    let show_title = reader_display.read().show_title;

    #[cfg(feature = "tts")]
    let listening_footer = if let Some(error) = &snapshot.error {
        format!("听书: {error}")
    } else if is_listening
        || snapshot.matches(&request) && snapshot.state == tts_protocol::SessionState::Paused
    {
        let config = snapshot.config.as_ref();
        format!(
            "{}: 速度{:.1} / 音量{:.1} · {} · {} · {}",
            if snapshot.state == tts_protocol::SessionState::Buffering {
                "缓冲中".into()
            } else {
                format!("{:?}", snapshot.state)
            },
            config.map_or(1.0, |value| value.speed),
            config.map_or(1.0, |value| value.volume),
            snapshot.alignment,
            snapshot.buffer,
            if reader_display.read().follow_tts && !follow_suspended.get() {
                "跟随中".into()
            } else {
                format!(
                    "自由浏览 · {} 回到朗读位置",
                    display_first_key(&reader_keymap, ReaderAction::FollowPlayback)
                )
            }
        )
    } else {
        format!(
            "按 {} 播放/暂停 · {} 搜索正文",
            display_first_key(&reader_keymap, ReaderAction::TogglePlay),
            display_first_key(&reader_keymap, ReaderAction::SearchContent)
        )
    };
    #[cfg(not(feature = "tts"))]
    let listening_footer = format!(
        "按 {} 搜索正文",
        display_first_key(&reader_keymap, ReaderAction::SearchContent)
    );

    element!(Border(
        border_style: theme.border,
        top_title: if show_title {
            Some(Line::from(props.chapter_name.to_string()).style(theme.chapter).centered())
        }else{
            None
        },
        bottom_title: if show_title {
           Some(Line::from(listening_footer).style(theme.footer).centered())
        }else{
            None
        },
    ){
        { if props.is_loading {
            element!(Loading(tip:"加载内容中...")).into_any()
        }else{
            element!(Text(
                text: paragraph,
                style:theme.content,
                scroll: (current_line as u16,0))
            ).into_any()
        } }
        { if editing && can_search {
            element!(View(height: Constraint::Length(1), margin: Margin::new(1,0)) {
                Input(input: search.read().draft.clone(), placeholder: "搜索本章正文 · Enter 提交 / Esc 取消")
            }).into_any()
        } else { element!(View(height: Constraint::Length(0))).into_any() } }
        View(
            flex_direction: Direction::Horizontal,
            justify_content: Flex::SpaceBetween,
            height: Constraint::Length(1),
            margin: Margin::new(1,0),
        ){
            // 分子取「最后可见行」而非首行:读到章末时显示 200/200 而不是 170/200。
            View(width: Constraint::Length(12)) {
                widget(Line::from(format!("{}/{} 行", (current_line + view).min(total), total)).style(theme.footer))
            }
            // 章末/章首「再按一次」确认提示(仅武装时显示;accent+bold 醒目;
            // 键名从 keymap 动态取,重绑后提示的就是新键)。
            View(width: Constraint::Fill(1)) {
                widget(Line::from(footer_status).style(theme.chapter).centered())
            }
            View(width: Constraint::Length(13)) {
                widget(Line::from(format!("{:.2}% {}",props.chapter_percent, current_time.read().clone())).style(theme.progress).right_aligned())
            }
        }
    })
}

/// 高亮范围的哨兵标记。
///
/// `textwrap` 换行会重排文本、原始字节偏移随之失效,所以把范围用字符嵌进文本
/// 里让它跟着文本走。必须选**正文里不可能出现**的字符 —— 这里用 ASCII 的
/// 记录分隔符与单元分隔符。
///
/// 历史坑:结束标记一度写成 `\u{002E}`,而那就是普通的英文句点 `.` —— 正文里
/// 一个「3.14」或「Mr.」就会把高亮提前截断。改动这两个常量前先确认新字符不会
/// 出现在小说正文中。
#[cfg(test)]
const HIGHLIGHT_START: char = '\u{001E}';
#[cfg(test)]
const HIGHLIGHT_END: char = '\u{001F}';

#[cfg(test)]
pub fn highlight(
    text: &str,
    range: &std::ops::Range<usize>,
    width: usize,
    highlight_style: Style,
    spacing: bool,
) -> Vec<Line<'static>> {
    let valid = text.get(range.clone()).map(|_| range);
    ContentLayout::new(text, width, spacing).styled(
        &[],
        0,
        Style::default(),
        valid.map(|range| (range, highlight_style)),
    )
}

/// 按原始文本行排版正文,`spacing` 决定逻辑段落之间是否补一个终端空行。
///
/// 小说正文通常以换行分隔段落,而不是以空行分隔。若直接对整章调用
/// `textwrap::fill`,这些段落会首尾相接,阅读时视觉上过于紧凑;但补空行会让
/// 总行数近乎翻倍,小屏下并非人人都要 —— 故由阅读偏好开关控制。
#[cfg(test)]
pub(super) fn wrap_content(text: &str, width: usize, spacing: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for line in text.lines() {
        append_wrapped_line(&mut lines, line, width, None, spacing);
    }
    lines
}

#[cfg(test)]
fn append_wrapped_line(
    lines: &mut Vec<Line<'static>>,
    line: &str,
    width: usize,
    highlight_style: Option<Style>,
    spacing: bool,
) {
    let line = line.trim_end_matches('\r');
    let is_blank = line.trim().is_empty();

    // 两类空行合并在一处判断:原文自带的空行**始终**保留(作者用它分隔场景,
    // 开关管的是「自动补」而不是「过滤原文」),段落间的空行则只在开关开启时补。
    // 两种情况都不叠加到已有空行之上,避免出现连续两个空行。
    if (is_blank || spacing)
        && !lines.is_empty()
        && !lines.last().is_some_and(|line| line.spans.is_empty())
    {
        lines.push(Line::default());
    }

    if is_blank {
        return;
    }

    let wrapped = textwrap::fill(line, width);
    if let Some(style) = highlight_style {
        lines.extend(highlight_text(&wrapped, style));
    } else {
        lines.extend(wrapped.lines().map(|line| Line::from(line.to_string())));
    }
}

/// 把带哨兵标记的文本切成带样式的行。
///
/// `in_highlight` 跨行保持:一个高亮段经 `textwrap` 换行后会横跨多个显示行,
/// 起止标记分别落在首行与末行,中间几行整行都是高亮。
#[cfg(test)]
fn highlight_text(text: &str, highlight_style: Style) -> Vec<Line<'static>> {
    let mut lines = vec![];
    let mut in_highlight = false;

    for line in text.lines() {
        let mut spans = vec![];
        let mut rest = line;

        if let Some((before, after)) = line.split_once(HIGHLIGHT_START) {
            spans.push(Span::from(before.to_string()));
            rest = after;
            in_highlight = true;
        }

        if in_highlight {
            match rest.split_once(HIGHLIGHT_END) {
                Some((highlighted, after)) => {
                    in_highlight = false;
                    spans.push(Span::from(highlighted.to_string()).style(highlight_style));
                    spans.push(Span::from(after.to_string()));
                }
                None => spans.push(Span::from(rest.to_string()).style(highlight_style)),
            }
        } else {
            spans.push(Span::from(rest.to_string()));
        }

        lines.push(Line::from(spans));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 取某一行里带高亮样式的文本。
    fn highlighted_of(line: &Line<'static>, style: Style) -> String {
        line.spans
            .iter()
            .filter(|span| span.style == style)
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn segment(text: &str, start: usize) -> std::ops::Range<usize> {
        start..start + text.len()
    }

    /// 高亮段里含英文句点时不能被截断。
    ///
    /// 回归:结束哨兵一度用 `\u{002E}`,而那就是普通句点 —— 「圆周率是 3.14。」
    /// 这种句子只会高亮到「3」为止,后半截失色。
    #[test]
    fn highlight_survives_ascii_dots_in_the_segment() {
        let style = Style::default().bold();
        let text = "圆周率约等于 3.14,记作 Mr. Pi。";

        let lines = highlight(text, &segment(text, 0), 200, style, false);

        assert_eq!(lines.len(), 1);
        assert_eq!(highlighted_of(&lines[0], style), text);
    }

    /// 只有 segment 覆盖的那一段被高亮,同行其余部分保持原样。
    #[test]
    fn highlight_covers_only_the_segment() {
        let style = Style::default().bold();
        let text = "前半句。后半句。";
        let target = "后半句。";
        let start = text.find(target).unwrap();

        let lines = highlight(text, &segment(target, start), 200, style, false);

        assert_eq!(highlighted_of(&lines[0], style), target);
        assert_eq!(texts(&lines), [text]);
    }

    /// TTS 的 segment 文本是 trim 过的,`start` 却指向 trim 之前的位置 ——
    /// 所以要从 start 起搜一次,不能拿 start 直接切片。
    #[test]
    fn highlight_uses_original_range_after_indentation() {
        let style = Style::default().bold();
        let text = "开头。   缩进的一句。";
        let target = "缩进的一句。";
        // start 故意指向空白处,模拟 preprocess_text 的 trim 行为。
        let start = text.find(target).unwrap();

        let lines = highlight(text, &segment(target, start), 200, style, false);

        assert_eq!(highlighted_of(&lines[0], style), target);
    }

    /// 换章瞬间 highlight_range 可能还指向上一章的偏移 —— 越界不得 panic,
    /// 退化成无高亮的正常排版即可(渲染路径上 panic 会掀掉整个 TUI)。
    #[test]
    fn highlight_with_stale_offset_falls_back_instead_of_panicking() {
        let style = Style::default().bold();
        let text = "短短一句。";

        let out_of_range = highlight(text, &segment("不存在的内容", 9999), 200, style, false);
        assert_eq!(texts(&out_of_range), [text]);

        // 落在多字节字符中间的偏移同样不能 panic。
        let mid_char = highlight(text, &segment("一句", 1), 200, style, false);
        assert_eq!(texts(&mid_char), [text]);
    }

    /// 取每行的纯文本,便于断言排版结果。
    fn texts(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    /// 开关是排版的唯一分歧点:同一段正文,开则段间有空行,关则首尾相接。
    #[test]
    fn paragraph_spacing_toggles_blank_lines_between_paragraphs() {
        let text = "第一段。\n第二段。\n第三段。";

        assert_eq!(
            texts(&wrap_content(text, 40, false)),
            ["第一段。", "第二段。", "第三段。"]
        );
        assert_eq!(
            texts(&wrap_content(text, 40, true)),
            ["第一段。", "", "第二段。", "", "第三段。"]
        );
    }

    /// 开关管的是「自动补」,不是「过滤原文」:作者用来分隔场景的空行两种模式下都得留着,
    /// 且开启时不能和自动补的空行叠成两行。
    #[test]
    fn blank_lines_in_source_survive_both_modes_without_doubling() {
        let text = "上一场景。\n\n下一场景。";

        assert_eq!(
            texts(&wrap_content(text, 40, false)),
            ["上一场景。", "", "下一场景。"]
        );
        assert_eq!(
            texts(&wrap_content(text, 40, true)),
            ["上一场景。", "", "下一场景。"]
        );
    }

    /// 正文不以空行开头 —— 首段前面没有「上一段」,补空行等于白白吃掉一行。
    #[test]
    fn no_leading_blank_line() {
        for spacing in [false, true] {
            let lines = wrap_content("开篇。\n次段。", 40, spacing);
            assert!(
                !lines[0].spans.is_empty(),
                "spacing={spacing} 首行不该是空行"
            );
        }
    }

    /// 连续多个空行折叠成一个,否则原文里的大段留白会把一屏顶满。
    #[test]
    fn consecutive_blank_lines_collapse() {
        assert_eq!(
            texts(&wrap_content("甲。\n\n\n\n乙。", 40, true)),
            ["甲。", "", "乙。"]
        );
    }

    /// 空内容不该产出任何行(章节为空时 total=0,坐标系另有兜底)。
    #[test]
    fn empty_content_yields_no_lines() {
        assert!(wrap_content("", 40, true).is_empty());
        assert!(wrap_content("", 40, false).is_empty());
    }

    /// 视口尺寸的换算只此一处,极矮终端下也必须 ≥ 1(否则翻页步长会退化为 0、翻不动)。
    #[test]
    fn visible_lines_never_zero() {
        for height in 0..=4u16 {
            assert_eq!(visible_lines(height), 1, "height={height}");
        }
        assert_eq!(visible_lines(33), 30);
    }

    /// 落盘往返:用户滚动产生的比例恒 < 1.0,不会被 `from_ratio` 误判成章末。
    #[test]
    fn ratio_round_trip_never_collides_with_chapter_end() {
        for total in [1usize, 2, 40, 121, 5000] {
            for line in [0, total / 3, total.saturating_sub(1)] {
                let ratio = line as f64 / total as f64;
                assert!(ratio < 1.0, "total={total} line={line}");
                assert_eq!(ScrollTarget::from_ratio(ratio), ScrollTarget::Ratio(ratio));
            }
        }
        assert_eq!(ScrollTarget::from_ratio(1.0), ScrollTarget::ChapterEnd);
        assert_eq!(ScrollTarget::ChapterEnd.as_ratio(), 1.0);
    }

    /// `Ratio` 解析必须钳到贴底位置:换终端尺寸后若停在贴底之下,
    /// `current_line < end_scroll` 恒假 → 前向滚动/翻页失效并误报章末。
    #[test]
    fn ratio_resolve_clamps_to_end_scroll_across_viewports() {
        // 会话 A:total=40、view=21 → end_scroll=19,滚到章末存下 19/40。
        let target = ScrollTarget::from_ratio(19.0 / 40.0);
        // 会话 B:终端变高到整章一屏放得下 → end_scroll=0,必须回到 0 而不是 19。
        assert_eq!(target.resolve(40, 0), 0);
        // 重排后 total 变小(拉宽终端):同样不得越过新的贴底位置。
        assert_eq!(ScrollTarget::from_ratio(0.7).resolve(60, 30), 30);
        // 视口不变时按比例定位,行为不变。
        assert_eq!(ScrollTarget::from_ratio(0.5).resolve(100, 70), 50);
    }

    /// 末尾留白只在产生它的视口里有效,视口一变即退化为贴底。
    #[test]
    fn overscroll_survives_only_its_own_viewport() {
        // total=100、view=30 → end_scroll=70;翻页落到 84(留白区)。
        let target = ScrollTarget::Overscroll {
            ratio: 84.0 / 100.0,
            end_scroll: 70,
        };
        assert_eq!(target.resolve(100, 70), 84, "同视口应保留留白");
        assert_eq!(target.resolve(100, 40), 40, "视口变了应退化为贴底");
        // 落盘只存比例,重开后按 Ratio 解析(留白不跨会话保留)。
        assert_eq!(
            ScrollTarget::from_ratio(target.as_ratio()).resolve(100, 70),
            70
        );
    }

    /// 空章节/极窄终端下 total=0,解析不得 panic,且不产生非法行号。
    #[test]
    fn resolve_is_safe_when_content_is_empty() {
        assert_eq!(ScrollTarget::Ratio(0.5).resolve(0, 0), 0);
        assert_eq!(ScrollTarget::ChapterEnd.resolve(0, 0), 0);
        assert_eq!(
            ScrollTarget::Overscroll {
                ratio: 0.5,
                end_scroll: 0
            }
            .resolve(0, 0),
            0
        );
    }
}
