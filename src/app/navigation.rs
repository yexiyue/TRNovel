use ratatui_kit::prelude::*;

pub const HISTORY_LENGTH: usize = 10;
static TRAIL: Atom<Trail> = Atom::new(Trail::default);

/// Labels mirror the framework history; route state remains owned by the router.
#[derive(Clone)]
pub struct Trail {
    paths: Vec<String>,
    current: usize,
}

impl Default for Trail {
    fn default() -> Self {
        Self {
            paths: vec!["/home".into()],
            current: 0,
        }
    }
}

impl Trail {
    fn push(&mut self, path: &str) {
        if self.paths.len() >= HISTORY_LENGTH {
            self.paths.remove(0);
            self.current = self.current.saturating_sub(1);
        }
        self.current += 1;
        self.paths.truncate(self.current);
        self.paths.push(path.into());
    }

    fn go(&mut self, delta: i32) {
        let next = self.current as i64 + i64::from(delta);
        if next >= 0 && (next as usize) < self.paths.len() {
            self.current = next as usize;
        }
    }

    pub fn back_label(&self) -> Option<&'static str> {
        self.current
            .checked_sub(1)
            .map(|i| page_label(&self.paths[i]))
    }

    pub fn forward_label(&self) -> Option<&'static str> {
        self.paths.get(self.current + 1).map(|p| page_label(p))
    }
}

pub fn page_label(path: &str) -> &'static str {
    match path {
        "/home" => "首页",
        "/select-file" => "本地小说",
        "/local-novel" | "/network-novel" => "阅读",
        "/book-source" => "书源",
        "/select-books" => "书籍列表",
        "/book-detail" => "小说详情",
        "/select-history" => "历史记录",
        "/book-source-login" => "书源登录",
        "/theme-setting" => "全局设置",
        _ => "上一页",
    }
}

#[derive(Clone, Copy)]
pub struct AppNavigate {
    router: Navigate,
    trail: AtomState<Trail>,
}

impl AppNavigate {
    pub fn push(&mut self, path: &str) {
        self.trail.write().push(path);
        self.router.push(path);
    }

    pub fn push_with_state<T: Send + Sync + 'static>(&mut self, path: &str, state: T) {
        self.trail.write().push(path);
        self.router.push_with_state(path, state);
    }

    pub fn go(&mut self, delta: i32) {
        self.trail.write().go(delta);
        self.router.go(delta);
    }
}

pub trait UseAppNavigate {
    fn use_app_navigate(&mut self) -> AppNavigate;
}

impl UseAppNavigate for Hooks<'_, '_> {
    fn use_app_navigate(&mut self) -> AppNavigate {
        AppNavigate {
            router: self.use_navigate(),
            trail: self.use_atom(&TRAIL),
        }
    }
}

pub fn use_trail(hooks: &mut Hooks) -> AtomState<Trail> {
    hooks.use_atom(&TRAIL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_after_back_discards_forward_destination() {
        let mut trail = Trail::default();
        trail.push("/select-history");
        trail.push("/local-novel");
        assert_eq!(trail.back_label(), Some("历史记录"));
        trail.go(-1);
        assert_eq!(trail.forward_label(), Some("阅读"));
        trail.push("/book-source");
        assert_eq!(trail.forward_label(), None);
        assert_eq!(trail.back_label(), Some("历史记录"));
    }

    #[test]
    fn history_capacity_and_boundaries_match_router() {
        let mut trail = Trail::default();
        for _ in 0..25 {
            trail.push("/select-file");
        }
        assert_eq!(trail.paths.len(), HISTORY_LENGTH);
        trail.go(-9);
        assert_eq!(trail.back_label(), None);
        trail.go(-1);
        assert_eq!(trail.current, 0);
        trail.push("/local-novel");
        assert_eq!(trail.back_label(), Some("本地小说"));
        assert_eq!(trail.forward_label(), None);
    }
}
