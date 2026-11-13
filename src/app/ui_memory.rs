//! 应用级界面记忆:按路由(以及目录)记住列表光标,返回时恢复。
//!
//! 只保存在进程内:界面记忆是本次运行的体验细节,不是用户配置,没必要落盘。
//! 键统一用短字符串,页面 mount 时读取、选中变化时写回。

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex, MutexGuard},
};

/// 首页菜单。
pub const HOME: &str = "home";
/// 历史记录列表。
pub const SELECT_HISTORY: &str = "select-history";
/// 本地小说列表(选中项按目录分别保存,键见页面侧)。
pub const LOCAL_NOVEL: &str = "local-novel";
/// 本地小说上次停留的目录。
pub const LOCAL_NOVEL_DIR: &str = "local-novel.dir";
/// 主题设置列表。
pub const THEME_SETTING: &str = "theme-setting";
/// 全局设置:index 保存分组,offset 保存组内光标。
pub const SETTINGS: &str = "settings";

/// 一个列表光标。offset 留给需要滚动位置的列表,当前页面只用 index。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Cursor {
    pub index: usize,
    pub offset: usize,
}

impl Cursor {
    pub const fn new(index: usize, offset: usize) -> Self {
        Self { index, offset }
    }

    /// 列表缩短后把光标钳到最后一项;空列表归零。
    pub fn clamped(self, len: usize) -> Self {
        if len == 0 {
            return Self::default();
        }
        let last = len - 1;
        Self {
            index: self.index.min(last),
            offset: self.offset.min(last),
        }
    }
}

/// 记忆表本体。抽成普通结构体,单测直接构造,不碰进程级全局状态。
#[derive(Debug, Default)]
pub struct UiMemory {
    cursors: HashMap<String, Cursor>,
    dirs: HashMap<String, PathBuf>,
}

impl UiMemory {
    pub fn cursor(&self, key: &str) -> Option<Cursor> {
        self.cursors.get(key).copied()
    }

    pub fn save_cursor(&mut self, key: impl Into<String>, cursor: Cursor) {
        self.cursors.insert(key.into(), cursor);
    }

    /// 恢复选中索引:越界钳到最后一项,没有记录时用调用方给的默认值。
    pub fn restored_index(&self, key: &str, len: usize, fallback: usize) -> usize {
        match self.cursor(key) {
            Some(cursor) => cursor.clamped(len).index,
            None if len == 0 => 0,
            None => fallback.min(len - 1),
        }
    }

    pub fn dir(&self, key: &str) -> Option<PathBuf> {
        self.dirs.get(key).cloned()
    }

    pub fn save_dir(&mut self, key: impl Into<String>, dir: &Path) {
        self.dirs.insert(key.into(), dir.to_path_buf());
    }
}

static UI_MEMORY: LazyLock<Mutex<UiMemory>> = LazyLock::new(|| Mutex::new(UiMemory::default()));

fn memory() -> MutexGuard<'static, UiMemory> {
    UI_MEMORY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn restored_cursor(key: &str) -> Option<Cursor> {
    memory().cursor(key)
}

pub fn restored_index(key: &str, len: usize, fallback: usize) -> usize {
    memory().restored_index(key, len, fallback)
}

pub fn save_cursor(key: impl Into<String>, cursor: Cursor) {
    memory().save_cursor(key, cursor);
}

pub fn save_index(key: impl Into<String>, index: usize) {
    save_cursor(key, Cursor::new(index, 0));
}

pub fn remembered_dir(key: &str) -> Option<PathBuf> {
    memory().dir(key)
}

pub fn remember_dir(key: impl Into<String>, dir: &Path) {
    memory().save_dir(key, dir);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_is_clamped_to_shrinking_list() {
        assert_eq!(Cursor::new(4, 7).clamped(3), Cursor::new(2, 2));
        assert_eq!(Cursor::new(2, 2).clamped(0), Cursor::default());
        assert_eq!(Cursor::new(0, 0).clamped(5), Cursor::new(0, 0));
    }

    #[test]
    fn cursor_round_trips_and_falls_back_by_key() {
        let mut memory = UiMemory::default();
        assert_eq!(memory.restored_index(HOME, 4, 0), 0);

        memory.save_cursor(HOME, Cursor::new(3, 1));
        assert_eq!(memory.restored_index(HOME, 4, 0), 3);
        // 列表缩短后钳到最后一项。
        assert_eq!(memory.restored_index(HOME, 2, 0), 1);
        // 没有记忆的键仍使用页面默认值,并受长度约束。
        assert_eq!(memory.restored_index("other", 4, 2), 2);
        assert_eq!(memory.restored_index("other", 2, 9), 1);
        assert_eq!(memory.restored_index("other", 0, 9), 0);
    }

    #[test]
    fn directory_round_trips() {
        let mut memory = UiMemory::default();
        assert_eq!(memory.dir(LOCAL_NOVEL_DIR), None);

        memory.save_dir(LOCAL_NOVEL_DIR, Path::new("D:/books"));
        assert_eq!(memory.dir(LOCAL_NOVEL_DIR), Some(PathBuf::from("D:/books")));
        assert_eq!(memory.dir("other"), None);
    }
}
