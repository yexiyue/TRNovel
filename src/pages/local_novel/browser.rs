//! 本地小说目录浏览的纯逻辑：目录枚举、排序、路径截断与列表行构建。
//!
//! 这里刻意不依赖 ratatui-kit 的 hooks，页面只负责把状态喂进来；这样排序、
//! 上级目录和窄屏截断都能用普通单测覆盖，不必启动终端。

use anyhow::{Context, Result};
use ratatui::{text::Line, widgets::ListItem};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// 快捷入口只保存标签和路径；是否展示由 quick_dirs 按目录是否存在决定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickDir {
    pub label: &'static str,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntryItem {
    pub name: String,
    pub path: PathBuf,
    pub kind: EntryKind,
}

impl DirEntryItem {
    fn new(name: String, path: PathBuf, kind: EntryKind) -> Self {
        Self { name, path, kind }
    }
}

/// 当前目录一次读取的结果。带上目录本身，页面才能判断异步结果是否已经过期。
#[derive(Debug)]
pub struct DirListing {
    pub dir: PathBuf,
    pub entries: Vec<DirEntryItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Parent,
    Shortcut,
    Directory,
    File,
}

/// 列表一行。path 同时用于「进入/打开」和判断是否有权限。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRow {
    pub label: String,
    pub path: PathBuf,
    pub kind: RowKind,
}

impl From<BrowserRow> for ListItem<'static> {
    fn from(row: BrowserRow) -> Self {
        ListItem::new(row.label)
    }
}

/// 只把 .txt 当小说；扩展名不区分大小写，和 NovelFileIndex 的规则保持一致。
pub fn is_supported_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
}

/// 读取一层目录：目录在前、文件在后；隐藏项和不支持的文件直接跳过。
///
/// 单个条目读取失败（权限、遍历中被删除）只跳过该条，不能让整个目录都打不开。
pub fn list_directory(dir: &Path) -> Result<Vec<DirEntryItem>> {
    let mut directories = Vec::new();
    let mut files = Vec::new();

    for entry in fs::read_dir(dir).with_context(|| format!("无法读取目录 {}", dir.display()))?
    {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        // 隐藏目录会带出 .git/node_modules 之类的大量无关项，隐藏文件也一并跳过。
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let is_dir = match entry.file_type() {
            Ok(file_type) if file_type.is_symlink() => path.is_dir(),
            Ok(file_type) => file_type.is_dir(),
            Err(_) => path.is_dir(),
        };
        if is_dir {
            directories.push(DirEntryItem::new(name, path, EntryKind::Directory));
        } else if is_supported_file(&path) {
            files.push(DirEntryItem::new(name, path, EntryKind::File));
        }
    }

    sort_entries(&mut directories);
    sort_entries(&mut files);
    directories.extend(files);
    Ok(directories)
}

fn sort_entries(entries: &mut [DirEntryItem]) {
    entries.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
}

/// 相对路径的父级可能为空串：到这一层就意味着没有「上级目录」可回。
pub fn parent_dir(path: &Path) -> Option<PathBuf> {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

/// 左侧截断、保留尾部；路径最右边才是用户真正关心的目录名。
///
/// 用 ratatui 的显示宽度而不是字节数/字符数，中文目录名和全角字符也不会溢出。
pub fn truncate_path_left(path: &str, max_width: usize) -> String {
    if Line::from(path).width() <= max_width {
        return path.to_owned();
    }
    if max_width == 0 {
        return String::new();
    }

    let budget = max_width.saturating_sub(1);
    let mut used = 0;
    let mut tail = Vec::new();
    for ch in path.chars().rev() {
        let width = Line::from(ch.to_string()).width();
        if used + width > budget {
            break;
        }
        used += width;
        tail.push(ch);
    }
    tail.reverse();
    format!("…{}", tail.into_iter().collect::<String>())
}

/// 路由带进来的是文件时从它的父目录开始浏览；目录本身或无效路径由调用方兜底。
pub fn initial_dir(route: Option<&Path>, fallback: PathBuf) -> PathBuf {
    match route {
        Some(path) if path.is_dir() => absolutize(path),
        Some(path) if path.is_file() => path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(absolutize)
            .unwrap_or(fallback),
        _ => fallback,
    }
}

/// 快捷入口必须真实存在；工作目录（cwd）也走同一套去重，避免主目录=cwd 时出现两行。
pub fn quick_dirs(cwd: &Path) -> Vec<QuickDir> {
    let candidates = [
        QuickDir {
            label: "主目录",
            path: dirs::home_dir().unwrap_or_default(),
        },
        QuickDir {
            label: "工作目录",
            path: cwd.to_path_buf(),
        },
        QuickDir {
            label: "桌面",
            path: dirs::desktop_dir().unwrap_or_default(),
        },
        QuickDir {
            label: "文档",
            path: dirs::document_dir().unwrap_or_default(),
        },
    ];
    dedupe_existing(candidates.to_vec())
}

/// 去掉不存在和重复的入口。空 PathBuf（系统没有该特殊目录）会被 is_dir 自然过滤。
pub fn dedupe_existing(candidates: Vec<QuickDir>) -> Vec<QuickDir> {
    let mut seen = std::collections::HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| {
            if !candidate.path.is_dir() {
                return false;
            }
            let key = absolutize(&candidate.path);
            seen.insert(key)
        })
        .collect()
}

/// 组装列表行：上级目录、快捷入口在前，当前目录条目按「目录→文件」排在后面。
///
/// 筛选只作用于当前目录内容，不隐藏「上级目录」——否则用户被一个错误的过滤词
/// 困在目录里时连返回入口都没有；快捷入口在筛选态隐藏，避免结果里混入无关路径。
pub fn build_rows(
    current: &Path,
    entries: &[DirEntryItem],
    filter: &str,
    quick: &[QuickDir],
) -> Vec<BrowserRow> {
    let mut rows = Vec::new();

    if let Some(parent) = parent_dir(current) {
        rows.push(BrowserRow {
            label: ".. 上级目录".to_owned(),
            path: parent,
            kind: RowKind::Parent,
        });
    }

    let query = filter.trim().to_lowercase();
    if query.is_empty() {
        // 快捷入口是固定导航锚点：即使正好等于当前目录也保留。否则从主目录
        // 启动时「主目录」入口会凭空消失，看起来像功能没做。
        for quick_dir in quick {
            rows.push(BrowserRow {
                label: format!("★ {}", quick_dir.label),
                path: quick_dir.path.clone(),
                kind: RowKind::Shortcut,
            });
        }
    }

    for entry in entries {
        if !query.is_empty() && !entry.name.to_lowercase().contains(&query) {
            continue;
        }
        rows.push(match entry.kind {
            EntryKind::Directory => BrowserRow {
                label: format!("▸ {}/", entry.name),
                path: entry.path.clone(),
                kind: RowKind::Directory,
            },
            EntryKind::File => BrowserRow {
                label: format!("  {}", entry.name),
                path: entry.path.clone(),
                kind: RowKind::File,
            },
        });
    }

    rows
}

fn absolutize(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU32, Ordering},
    };

    static NEXT_TEMP: AtomicU32 = AtomicU32::new(1);

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "trnovel-browser-{name}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn list_directory_filters_hidden_and_non_txt_then_sorts() {
        let root = temp_dir("list");
        fs::create_dir(root.join("beta")).unwrap();
        fs::create_dir(root.join("Alpha")).unwrap();
        fs::create_dir(root.join(".hidden-dir")).unwrap();
        fs::write(root.join("b.TXT"), "b").unwrap();
        fs::write(root.join("a.txt"), "a").unwrap();
        fs::write(root.join("note.md"), "n").unwrap();
        fs::write(root.join(".secret.txt"), "s").unwrap();

        let entries = list_directory(&root).unwrap();
        let names: Vec<_> = entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.kind))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Alpha", EntryKind::Directory),
                ("beta", EntryKind::Directory),
                ("a.txt", EntryKind::File),
                ("b.TXT", EntryKind::File),
            ]
        );
    }

    #[test]
    fn parent_dir_stops_at_empty_or_root() {
        assert_eq!(parent_dir(Path::new("a/b")), Some(PathBuf::from("a")));
        assert_eq!(parent_dir(Path::new("a")), None);
        #[cfg(windows)]
        assert_eq!(parent_dir(Path::new(r"C:\")), None);
        #[cfg(not(windows))]
        assert_eq!(parent_dir(Path::new("/")), None);
    }

    #[test]
    fn truncate_path_keeps_tail_and_respects_display_width() {
        assert_eq!(truncate_path_left("short", 10), "short");
        assert_eq!(truncate_path_left("anything", 0), "");

        let path = r"C:\books\a-very-long-novel-name.txt";
        let truncated = truncate_path_left(path, 12);
        assert!(truncated.starts_with('…'));
        assert!(truncated.ends_with("name.txt"));
        assert!(Line::from(truncated.as_str()).width() <= 12);

        let cjk = r"C:\小说\很长的小说名字.txt";
        let truncated = truncate_path_left(cjk, 10);
        assert!(truncated.starts_with('…'));
        assert!(truncated.ends_with(".txt"));
        assert!(Line::from(truncated.as_str()).width() <= 10);
    }

    #[test]
    fn initial_dir_uses_parent_for_a_file() {
        let root = temp_dir("initial");
        let file = root.join("book.txt");
        fs::write(&file, "x").unwrap();
        let fallback = temp_dir("fallback");

        assert_eq!(initial_dir(Some(&file), fallback.clone()), root);
        assert_eq!(initial_dir(Some(&root), fallback.clone()), root);
        assert_eq!(initial_dir(None, fallback.clone()), fallback);
    }

    #[test]
    fn quick_dirs_skip_missing_and_duplicate_entries() {
        let root = temp_dir("quick");
        let other = root.join("other");
        fs::create_dir(&other).unwrap();
        let candidates = vec![
            QuickDir {
                label: "主目录",
                path: root.clone(),
            },
            QuickDir {
                label: "不存在",
                path: root.join("missing"),
            },
            QuickDir {
                label: "工作目录",
                path: root.clone(),
            },
            QuickDir {
                label: "文档",
                path: other.clone(),
            },
        ];
        let deduped = dedupe_existing(candidates);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].path, root);
        assert_eq!(deduped[1].path, other);
    }

    #[test]
    fn build_rows_keeps_parent_and_hides_quick_dirs_while_filtering() {
        let root = temp_dir("rows");
        let home = temp_dir("home");
        let entries = vec![DirEntryItem::new(
            "Novel.txt".to_owned(),
            root.join("Novel.txt"),
            EntryKind::File,
        )];
        let quick = vec![
            QuickDir {
                label: "主目录",
                path: home,
            },
            QuickDir {
                label: "工作目录",
                path: root.clone(),
            },
        ];

        let rows = build_rows(&root, &entries, "", &quick);
        assert_eq!(rows[0].kind, RowKind::Parent);
        assert_eq!(rows[1].kind, RowKind::Shortcut);
        // 当前目录本身也保留快捷入口，避免启动目录对应的入口消失。
        assert_eq!(rows[2].kind, RowKind::Shortcut);
        assert_eq!(rows[3].kind, RowKind::File);

        let rows = build_rows(&root, &entries, "nov", &quick);
        assert_eq!(rows[0].kind, RowKind::Parent);
        assert!(rows.iter().all(|row| row.kind != RowKind::Shortcut));
        assert_eq!(rows.len(), 2);
    }
}
