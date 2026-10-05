use std::{ops::Range, sync::Arc};

use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui_kit::prelude::tui_input::backend::crossterm::EventHandler;

use ratatui::{
    style::Style,
    text::{Line, Span},
};
use ratatui_kit::prelude::tui_input;

/// Ephemeral state shared by the reading page and its content component.
#[derive(Default)]
pub struct ChapterSearch {
    pub draft: tui_input::Input,
    pub editing: bool,
    pub query: String,
    pub matches: Arc<Vec<Range<usize>>>,
    pub selected: usize,
    pub revision: u64,
}

impl ChapterSearch {
    pub fn handle_input(&mut self, event: &Event, text: &str) {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Esc => self.editing = false,
                KeyCode::Enter => self.submit(text, self.draft.value().to_string()),
                _ => {
                    self.draft.handle_event(event);
                }
            },
            Event::Paste(value) => {
                for ch in value.chars().filter(|ch| !ch.is_control()) {
                    self.draft.handle(tui_input::InputRequest::InsertChar(ch));
                }
            }
            _ => {}
        }
    }

    pub fn submit(&mut self, text: &str, query: String) {
        self.matches = Arc::new(find_matches(text, &query));
        self.query = query;
        self.selected = 0;
        self.editing = false;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn advance(&mut self, backwards: bool) {
        let count = self.matches.len();
        if count > 0 {
            self.selected = if backwards {
                (self.selected + count - 1) % count
            } else {
                (self.selected + 1) % count
            };
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn status(&self) -> String {
        if self.query.is_empty() {
            String::new()
        } else if self.matches.is_empty() {
            "正文搜索：未找到".into()
        } else {
            format!(
                "正文搜索：第 {} / {} 个",
                self.selected + 1,
                self.matches.len()
            )
        }
    }
}

fn find_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    if query.is_empty() || query.contains(['\n', '\r']) {
        return Vec::new();
    }
    let mut offset = 0;
    let mut matches = Vec::new();
    for raw in text.split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        matches.extend(
            line.match_indices(query)
                .map(|(start, value)| offset + start..offset + start + value.len()),
        );
        offset += raw.len();
    }
    matches
}

/// Wrap plain text first, preserving byte coordinates for styling and navigation.
#[derive(Default)]
pub struct ContentLayout {
    pub lines: Vec<Line<'static>>,
    ranges: Vec<Range<usize>>,
}

impl ContentLayout {
    pub fn new(text: &str, width: usize, spacing: bool) -> Self {
        let mut layout = Self::default();
        let mut offset = 0;
        for raw in text.split_inclusive('\n') {
            let line = raw.trim_end_matches(['\n', '\r']);
            let blank = line.trim().is_empty();
            if (blank || spacing)
                && !layout.lines.is_empty()
                && !layout
                    .lines
                    .last()
                    .is_some_and(|line| line.spans.is_empty())
            {
                layout.lines.push(Line::default());
                layout.ranges.push(offset..offset);
            }
            if !blank {
                let mut cursor = 0;
                for wrapped in textwrap::wrap(line, width) {
                    // Default textwrap options return contiguous source slices, omitting
                    // only whitespace at soft wraps. Search forward to retain that gap.
                    let start = cursor + line[cursor..].find(wrapped.as_ref()).unwrap_or(0);
                    let end = start + wrapped.len();
                    layout.ranges.push(offset + start..offset + end);
                    layout.lines.push(Line::from(wrapped.into_owned()));
                    cursor = end;
                }
            }
            offset += raw.len();
        }
        layout
    }

    pub fn match_line(&self, target: &Range<usize>) -> usize {
        self.ranges
            .iter()
            .position(|range| range.end > target.start)
            .unwrap_or_else(|| self.lines.len().saturating_sub(1))
    }

    pub fn styled(
        &self,
        matches: &[Range<usize>],
        selected: usize,
        search_style: Style,
        tts: Option<(&Range<usize>, Style)>,
    ) -> Vec<Line<'static>> {
        self.lines
            .iter()
            .zip(&self.ranges)
            .map(|(line, range)| {
                if range.is_empty() {
                    return line.clone();
                }
                let text = line.spans[0].content.as_ref();
                let first = matches.partition_point(|item| item.end <= range.start);
                let mut cuts = vec![range.start, range.end];
                for item in matches[first..]
                    .iter()
                    .take_while(|item| item.start < range.end)
                {
                    cuts.extend([item.start.max(range.start), item.end.min(range.end)]);
                }
                if let Some((item, _)) = tts
                    && item.start < range.end
                    && item.end > range.start
                {
                    cuts.extend([item.start.max(range.start), item.end.min(range.end)]);
                }
                cuts.sort_unstable();
                cuts.dedup();
                let spans = cuts
                    .windows(2)
                    .map(|cut| {
                        let index = matches.partition_point(|item| item.end <= cut[0]);
                        let style = if matches.get(index).is_some_and(|item| item.start <= cut[0]) {
                            if index == selected {
                                search_style.add_modifier(ratatui::style::Modifier::BOLD)
                            } else {
                                search_style
                            }
                        } else if let Some((item, style)) = tts
                            && item.contains(&cut[0])
                        {
                            style
                        } else {
                            Style::default()
                        };
                        Span::styled(
                            text[cut[0] - range.start..cut[1] - range.start].to_string(),
                            style,
                        )
                    })
                    .collect::<Vec<_>>();
                Line::from(spans)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_draft_only_commits_on_enter_and_escape_preserves_results() {
        use crossterm::event::{KeyEvent, KeyModifiers};
        let key = |code| Event::Key(KeyEvent::new(code, KeyModifiers::NONE));
        let mut state = ChapterSearch::default();
        state.submit("甲乙甲", "甲".into());
        let revision = state.revision;
        state.editing = true;
        state.handle_input(&Event::Paste("乙qbg/\n".into()), "甲乙甲");
        assert_eq!(state.draft.value(), "乙qbg/");
        assert_eq!(state.query, "甲");
        assert_eq!(state.revision, revision);
        state.handle_input(&key(KeyCode::Esc), "甲乙甲");
        assert!(!state.editing);
        assert_eq!(state.query, "甲");
        state.editing = true;
        state.draft = tui_input::Input::new("乙".into());
        state.handle_input(&key(KeyCode::Enter), "甲乙甲");
        assert_eq!(state.matches.len(), 1);
        assert_eq!(state.matches[0], 3..6);
        assert!(!state.editing);
        state.draft.reset();
        state.handle_input(&key(KeyCode::Enter), "甲乙甲");
        assert!(state.matches.is_empty());
        assert!(state.status().is_empty());
    }

    #[test]
    fn layout_matches_existing_paragraph_rules_and_maps_soft_wraps() {
        let samples = [
            "",
            "\n\n",
            "甲。\r\n\r\n乙。\n",
            "  hello-world   hello hello!",
            "　　甲乙丙丁戊己。\n第二段",
            "甲\n\n\n乙",
        ];
        for text in samples {
            for width in [0, 1, 4, 11, 80] {
                for spacing in [false, true] {
                    assert_eq!(
                        ContentLayout::new(text, width, spacing).lines,
                        super::super::read_content::wrap_content(text, width, spacing)
                    );
                }
            }
        }
        let layout = ContentLayout::new("甲乙丙丁\n戊己", 4, true);
        assert_eq!(layout.match_line(&(6..12)), 1);
        assert_eq!(layout.match_line(&(13..16)), 3);
    }

    #[test]
    fn literal_unicode_matches_are_non_overlapping_and_chapter_local() {
        assert_eq!(find_matches("甲甲甲\r\n甲甲", "甲甲"), vec![0..6, 11..17]);
        assert_eq!(find_matches("a.a A.A a.a", "a.a"), vec![0..3, 8..11]);
        assert!(find_matches("甲\n乙", "甲\n乙").is_empty());
        assert!(find_matches("甲", "").is_empty());
    }

    #[test]
    fn navigation_wraps_and_missing_results_are_safe() {
        let mut state = ChapterSearch::default();
        state.submit("甲乙甲", "甲".into());
        state.advance(true);
        assert_eq!(state.selected, 1);
        state.advance(false);
        assert_eq!(state.selected, 0);
        state.submit("甲", "乙".into());
        state.advance(true);
        assert_eq!(state.status(), "正文搜索：未找到");
    }

    #[test]
    fn styling_does_not_change_wrapping_and_search_wins_over_tts() {
        let text = "  甲乙甲乙丙。\r\n\nhello hello!";
        let matches = find_matches(text, "甲乙");
        for width in [1, 4, 12, 80] {
            for spacing in [false, true] {
                let layout = ContentLayout::new(text, width, spacing);
                let tts = 0..text.len();
                let style = Style::default().fg(ratatui::style::Color::Yellow);
                let styled = layout.styled(
                    &matches,
                    1,
                    style,
                    Some((&tts, Style::default().fg(ratatui::style::Color::Green))),
                );
                let texts = |lines: &[Line<'_>]| {
                    lines
                        .iter()
                        .map(|line| {
                            line.spans
                                .iter()
                                .map(|s| s.content.as_ref())
                                .collect::<String>()
                        })
                        .collect::<Vec<_>>()
                };
                assert_eq!(texts(&styled), texts(&layout.lines));
                assert!(
                    styled
                        .iter()
                        .flat_map(|line| &line.spans)
                        .any(|span| span.style.fg == style.fg)
                );
                assert!(layout.match_line(&matches[1]) < layout.lines.len());
            }
        }
    }
}
