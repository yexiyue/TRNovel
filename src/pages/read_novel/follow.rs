//! Viewport positioning uses source layout rows, never estimated speech timing.
pub(super) fn target(
    line: usize,
    current: usize,
    view: usize,
    end: usize,
    force: bool,
) -> Option<usize> {
    if view == 0 {
        return None;
    }
    let outside_safe_area =
        line < current || line >= current.saturating_add(view.saturating_sub(2));
    let target = line.saturating_sub(view / 3).min(end);
    ((force || outside_safe_area) && target != current).then_some(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_area_does_not_move_and_bottom_or_outside_repositions() {
        assert_eq!(target(12, 10, 12, 100, false), None);
        assert_eq!(target(20, 10, 12, 100, false), Some(16));
        assert_eq!(target(8, 10, 12, 100, false), Some(4));
        assert_eq!(target(30, 10, 12, 100, false), Some(26));
        assert_eq!(target(12, 10, 12, 100, true), Some(8));
    }
    #[test]
    fn source_mapping_survives_wrapping_crlf_and_spacing() {
        let text = format!("{}\r\nHello world.\n\n朗读位置。", "开头".repeat(20));
        let start = text.find("朗读").unwrap();
        for (width, spacing) in [(8, false), (8, true), (12, false), (12, true)] {
            let layout =
                crate::pages::read_novel::search::ContentLayout::new(&text, width, spacing);
            let line = layout.match_line(&(start..text.len()));
            let end = layout.lines.len().saturating_sub(3);
            let next = target(line, 0, 3, end, false).unwrap();
            assert!(next <= end && line >= next && line < next + 3);
        }
    }
    #[test]
    fn tiny_views_and_chapter_end_are_bounded() {
        assert_eq!(target(90, 60, 30, 65, false), Some(65));
        assert_eq!(target(0, 0, 1, 0, false), None);
        assert_eq!(target(3, 0, 1, 5, false), Some(3));
        assert_eq!(target(3, 0, 0, 5, true), None);
    }
}
