use crate::{
    TTSConfig,
    components::Loading,
    hooks::UseScrollbar,
    keymap::{ReaderAction, display_first_key},
    theme::ReaderTheme,
};
use novel_tts::utils::TextSegment;
use ratatui::{
    layout::{Constraint, Direction, Flex, Margin},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_kit::prelude::*;
use ratatui_kit_keymap::UseKeymapHandler;
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
    pub is_scroll: bool,
    pub is_loading: bool,
    pub width: u16,
    pub height: u16,
    pub on_prev: Handler<'static, bool>,
    pub on_next: Handler<'static, ()>,
    pub chapter_name: String,
    pub chapter_percent: f64,
    pub scroll_target: Option<State<ScrollTarget>>,
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
    let mut is_listening = hooks.use_state(|| false);
    let mut highlight_range = hooks.use_state(|| None::<TextSegment>);
    let tts_config = *hooks.use_context::<State<TTSConfig>>();
    let novel_tts = hooks.use_atom(&crate::state::NOVEL_TTS);
    let mut chapter_tts = hooks.use_state(|| None::<novel_tts::ChapterTTS>);
    let mut player = hooks.use_state(|| None::<novel_tts::Player>);
    let mut is_listening_done = hooks.use_state(|| false);
    let mut on_prev = props.on_prev.take();
    let mut on_next = props.on_next.take();
    // 章末/章首「再按一次」确认态(防误触跳章)。
    let mut edge = hooks.use_state(|| Edge::None);

    // 自动播放下一章节
    if is_listening_done.get() && tts_config.read().auto_play {
        if props.has_next {
            on_next(());
        } else {
            // 全书最后一章:没有下一章可续播。必须显式复位「播放中」——否则 `on_next` 静默
            // no-op 使 `content` 不变,以 `content` 为 deps 的清理 effect 不会重跑,
            // 底部会永久显示「播放中」且 p 键只在 pause/play 间空转、无法重播。
            is_listening.set(false);
        }
        is_listening_done.set(false);
    }

    hooks.use_effect(
        move || {
            if let Some(player) = player.write().take() {
                player.sink.stop();
            }
            if let Some(chapter_tts) = chapter_tts.write().take() {
                chapter_tts.cancel();
            }
            is_listening.set(false);
        },
        props.content.clone(),
    );

    hooks.use_effect(
        || {
            if let Some(player) = player.write().as_mut() {
                player.set_speed(tts_config.read().speed);
                player.set_volume(tts_config.read().volume);
            }
        },
        format!("{}-{}", tts_config.read().speed, tts_config.read().volume),
    );

    hooks.use_async_effect(
        {
            let content = props.content.clone();
            async move {
                if let Some(tts) = novel_tts.read().as_ref()
                    && tts_config.read().auto_play
                    && chapter_tts.read().is_none()
                {
                    let mut chapter = if let Some(chapter_tts) = chapter_tts.read().as_ref() {
                        chapter_tts.cancel();
                        chapter_tts.clone()
                    } else {
                        tts.chapter_tts(&content)
                    };

                    let (queue_output, mut receiver) =
                        chapter.stream(tts_config.read().voice.into(), |e| {
                            eprintln!("{e:?}");
                        });

                    let texts = chapter.texts.clone();
                    tokio::spawn(async move {
                        while let Some(index) = receiver.recv().await {
                            if let Some(index) = index {
                                highlight_range.set(Some(texts[index].clone()));
                            } else {
                                is_listening_done.set(true);
                            }
                        }
                    });

                    let p = tts.player(queue_output);
                    p.set_speed(tts_config.read().speed);
                    p.set_volume(tts_config.read().volume);

                    is_listening.set(true);
                    player.set(Some(p));
                    chapter_tts.set(Some(chapter));
                }
            }
        },
        (props.content.clone(), novel_tts.read().is_some()),
    );

    hooks.use_async_effect(
        async move {
            if let Some(tts) = novel_tts.read().as_ref()
                && let Some(chapter) = chapter_tts.write().as_mut()
            {
                let (queue_output, mut receiver) =
                    chapter.stream(tts_config.read().voice.into(), |e| {
                        eprintln!("{e:?}");
                    });

                let texts = chapter.texts.clone();
                tokio::spawn(async move {
                    while let Some(index) = receiver.recv().await {
                        if let Some(index) = index {
                            highlight_range.set(Some(texts[index].clone()));
                        } else {
                            is_listening_done.set(true);
                        }
                    }
                });

                let p = tts.player(queue_output);
                p.set_speed(tts_config.read().speed);
                p.set_volume(tts_config.read().volume);

                is_listening.set(true);
                player.set(Some(p));
            }
        },
        tts_config.read().voice,
    );

    let paragraph = hooks.use_memo(
        || {
            // 包成 TextParagraph(Send + Sync):0.30 起 owned Paragraph 内含 Block 而非 Send,
            // 无法直接存入 use_memo 的状态体系。TextParagraph Deref 到 Paragraph,后续 line_count/
            // 渲染照常。
            let paragraph = if let Some(segment) = highlight_range.read().as_ref()
                && is_listening.get()
            {
                Paragraph::new(highlight(
                    &props.content,
                    segment,
                    (props.width as usize).saturating_sub(2),
                    theme.tts_highlight,
                ))
            } else {
                Paragraph::new(textwrap::fill(
                    &props.content,
                    (props.width as usize).saturating_sub(2),
                ))
            };
            TextParagraph::from(paragraph)
        },
        (
            is_listening.get(),
            highlight_range.read().clone(),
            props.content.clone(),
            props.width,
            theme.tts_highlight,
        ),
    );

    let scroll_target = hooks.use_state(ScrollTarget::default);
    let mut scroll_target = props.scroll_target.unwrap_or(scroll_target);

    let is_scroll = props.is_scroll;
    // 滚动坐标系:以「内容总行数」为唯一绝对基准,三个量各司其职——
    //   total      正文总行数,落盘进度的分母(与视口无关,换终端尺寸后仍指向同一行)
    //   view       可见行数(height - 3:上下边框 + 底部状态栏)
    //   end_scroll 贴底位置(最后一行正好落在屏幕底部)= 章末判定线
    // 旧实现只有一个 `line_count`(实为 end_scroll)同时兼任滚动上限、进度分母与行号分母,
    // 分母含视口高度 → 进度随终端尺寸漂移,且末尾无法留白。
    let total = paragraph.line_count(props.width.saturating_sub(2));
    let view = visible_lines(props.height);
    let end_scroll = total.saturating_sub(view);

    let mut current_line = hooks.use_memo(
        || scroll_target.get().resolve(total, end_scroll),
        (total, view, scroll_target.get()),
    );
    let mut current_time = hooks.use_state(String::default);

    hooks.use_future(async move {
        current_time.set(chrono::Local::now().format("%H:%M").to_string());
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    // 滚动条的 content_length 要传**最大滚动位置**而非总行数:`use_scrollbar` 内部把两个
    // 入参同除以组件高度,传 total 会让 position 恒小于 content_len、滑块永远到不了底。
    hooks.use_scrollbar(end_scroll, Some(current_line));

    let props_content = props.content.clone();
    let has_prev = props.has_prev;
    let has_next = props.has_next;
    // 翻页步长:整屏减去用户配置的重叠行数,让上一屏末尾的若干行留在新一屏开头作视觉锚点
    // (issue #63)。公式在 ReaderDisplayConfig 上,设置面板展示「每页滚动 M 行」时调同一个。
    let step = reader_display.read().page_step(view);
    // 按语义 action 分发(键位可经 ~/.novel/keybindings.toml 自定义);
    // 页面级 action(模式/浮层切换)不在本组件处理,Ignored 交给上层。
    let reader_keymap = hooks.use_atom(&crate::state::KEYMAP).read().reader.clone();
    hooks.use_keymap_handler(
        EventScope::Current,
        EventPriority::Normal,
        reader_keymap.clone(),
        move |action, _key| {
            if !is_scroll {
                return EventResult::Ignored;
            }
            // 逐行滚动与整页翻页只差这一个步长,边界行为(全书边界只提示 / 章内边界
            // 「再按一次」武装 / 二次确认翻章)完全一致,故各自合并为一支单一实现:
            // 分开写就是把边界语义复制两份,将来加第三种步长(半页)还要再复制一次。
            let delta = match action {
                ReaderAction::PageUp | ReaderAction::PageDown => step,
                _ => 1,
            };
            match action {
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
                ReaderAction::VolumeUp => {
                    tts_config.write().increase_volume();
                    EventResult::Consumed
                }
                ReaderAction::VolumeDown => {
                    tts_config.write().decrease_volume();
                    EventResult::Consumed
                }
                ReaderAction::TogglePlay => {
                    if let Some(player) = player.read().as_ref() {
                        if is_listening.get() {
                            player.pause();
                            is_listening.set(false);
                        } else {
                            player.play();
                            is_listening.set(true);
                        }
                    } else if let Some(tts) = novel_tts.read().as_ref() {
                        let mut chapter = tts.chapter_tts(&props_content);
                        let (queue_output, mut receiver) =
                            chapter.stream(tts_config.read().voice.into(), |e| {
                                eprintln!("{e:?}");
                            });

                        let texts = chapter.texts.clone();
                        tokio::spawn(async move {
                            while let Some(index) = receiver.recv().await {
                                if let Some(index) = index {
                                    highlight_range.set(Some(texts[index].clone()));
                                } else {
                                    is_listening_done.set(true);
                                }
                            }
                        });
                        let p = tts.player(queue_output);
                        p.set_speed(tts_config.read().speed);
                        p.set_volume(tts_config.read().volume);
                        is_listening.set(true);
                        player.set(Some(p));
                        chapter_tts.set(Some(chapter));
                    }
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

    let show_title = reader_display.read().show_title;

    element!(Border(
        border_style: theme.border,
        top_title: if show_title {
            Some(Line::from(props.chapter_name.to_string()).style(theme.chapter).centered())
        }else{
            None
        },
        bottom_title: if show_title {
           Some((if is_listening.get(){
                Line::from(
                    format!(
                        "播放中: 播放速度{} / 音量{}",
                        tts_config.read().speed,
                        tts_config.read().volume,
                    )
                )
                .style(theme.footer)
            }else{
                Line::from(format!(
                    "按 {} 播放/暂停",
                    display_first_key(&reader_keymap, ReaderAction::TogglePlay)
                )).style(theme.footer)
            }).style(theme.footer).centered())
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
        View(
            flex_direction: Direction::Horizontal,
            justify_content: Flex::SpaceBetween,
            height: Constraint::Length(1),
            margin: Margin::new(1,0),
        ){
            // 分子取「最后可见行」而非首行:读到章末时显示 200/200 而不是 170/200。
            widget(Line::from(format!("{}/{} 行", (current_line + view).min(total), total)).style(theme.footer))
            // 章末/章首「再按一次」确认提示(仅武装时显示;accent+bold 醒目;
            // 键名从 keymap 动态取,重绑后提示的就是新键)。
            widget(Line::from(match edge.get() {
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
                Edge::None => String::new(),
            }).style(theme.chapter).centered())
            widget(Line::from(format!("{:.2}% {}",props.chapter_percent, current_time.read().clone())).style(theme.progress).right_aligned())
        }
    })
}

pub fn highlight(
    text: &str,
    segment: &TextSegment,
    width: usize,
    highlight_style: Style,
) -> Vec<Line<'static>> {
    let pattern: String = regex::escape(&segment.text);
    let regex = regex::Regex::new(&pattern).unwrap();
    let res = regex.find_at(text, segment.start);

    if let Some(mat) = res {
        let marked = format!(
            "{}\u{001E}{}\u{002E}{}",
            &text[..mat.start()],
            mat.as_str(),
            &text[mat.end()..]
        );
        let highlighted = textwrap::fill(&marked, width);
        // let re_mark = Regex::new(r"(?ms)<b>(.*?)</b>").unwrap();
        // let mat = re_mark.find(&highlighted).unwrap();
        highlight_text(&highlighted, highlight_style)
    } else {
        let texts = textwrap::fill(text, width);
        texts
            .lines()
            .map(|line| Line::from(line.to_string()))
            .collect::<Vec<_>>()
    }
}

fn highlight_text(text: &str, highlight_style: Style) -> Vec<Line<'static>> {
    let mut lines = vec![];
    let mut matched = 0;

    for line in text.lines() {
        let mut spans = vec![];
        let mut highlight = line.to_string();
        if let Some((start, rest)) = line.split_once('\u{001E}') {
            matched += 1;
            spans.push(Span::from(start.to_string()));
            highlight = rest.to_string();
        }

        if matched > 0 {
            if let Some((highlight, end)) = &highlight.split_once('\u{002E}') {
                matched -= 1;
                spans.push(Span::from(highlight.to_string()).style(highlight_style));
                spans.push(Span::from(end.to_string()));
            } else {
                spans.push(Span::from(highlight).style(highlight_style));
            }
        } else {
            spans.push(Span::from(line.to_string()));
        }
        lines.push(Line::from(spans));
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;

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
