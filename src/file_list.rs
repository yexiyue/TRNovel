use anyhow::Result;
use std::{ffi::OsStr, path::PathBuf};
use tui_tree_widget::TreeItem;
use walkdir::WalkDir;

const FILE_EXTS: [&str; 1] = ["txt"];

#[derive(Debug, Clone, PartialEq)]
pub struct NovelFileIndex {
    entries: Vec<NovelFileEntry>,
}

#[derive(Debug, Clone, PartialEq)]
struct NovelFileEntry {
    path: PathBuf,
    name: String,
    is_directory: bool,
    children: Vec<NovelFileEntry>,
}

impl NovelFileIndex {
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let path = if path.is_relative() {
            std::env::current_dir()?.join(path)
        } else {
            path
        };

        if path.is_file() {
            ensure_supported_file(&path)?;
            return Ok(Self {
                entries: vec![NovelFileEntry {
                    name: path.file_name().unwrap().to_string_lossy().to_string(),
                    path,
                    is_directory: false,
                    children: vec![],
                }],
            });
        }

        Ok(Self {
            entries: scan_novels(path, &FILE_EXTS)?,
        })
    }

    /// 按文件名筛选出树,复用已扫描的索引 —— 不碰文件系统。
    pub fn filter(&self, filter: Option<&str>) -> Vec<TreeItem<'static, PathBuf>> {
        filter_entries(&self.entries, filter)
    }
}

fn ensure_supported_file(path: &std::path::Path) -> Result<()> {
    let supported = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| FILE_EXTS.contains(&ext));
    if !supported {
        return Err(anyhow::anyhow!("不支持的文件类型"));
    }
    Ok(())
}

fn scan_novels(path: PathBuf, file_exts: &[&str]) -> Result<Vec<NovelFileEntry>> {
    let mut res = vec![];

    let walkdir = WalkDir::new(&path)
        .sort_by(|a, b| {
            // 目录排在文件前面，同类按路径名排序。
            // 用 walkdir 缓存的 file_type()（不额外 syscall），并以单一全序 key
            // (!is_dir, path) 的元组比较，保证满足全序——否则遇到既非目录也非
            // 普通文件的条目（Windows 的 reparse point / junction / 符号链接 /
            // 无权限项，is_dir()、is_file() 可能都为 false）时，与“目录优先”规则
            // 混合会破坏传递性，触发 std 排序的 total order panic。
            let a_is_dir = a.file_type().is_dir();
            let b_is_dir = b.file_type().is_dir();
            b_is_dir.cmp(&a_is_dir).then_with(|| a.path().cmp(b.path()))
        })
        .max_depth(1);

    for entity in walkdir {
        let entity = entity?;

        if entity.path().to_path_buf() == path {
            continue;
        }
        if entity.path().is_dir() {
            let children = scan_novels(entity.clone().into_path(), file_exts)?;
            if children.is_empty() {
                continue;
            }
            res.push(NovelFileEntry {
                path: entity.clone().into_path(),
                name: entity.file_name().to_string_lossy().to_string(),
                is_directory: true,
                children,
            });
        } else if entity.path().is_file()
            && file_exts
                .iter()
                .any(|&e| e == entity.path().extension().unwrap_or(OsStr::new("")))
        {
            res.push(NovelFileEntry {
                path: entity.clone().into_path(),
                name: entity.file_name().to_string_lossy().to_string(),
                is_directory: false,
                children: vec![],
            });
        }
    }
    Ok(res)
}

/// 只在内存索引上过滤,命中文件所在的目录逐层保留,空目录不进结果。
///
/// 不返回 `Result`:`TreeItem::new` 仅在同级 identifier 重复时失败,而这里的
/// identifier 是文件系统路径、同级天然唯一,故该分支实际不可达。真出现了也只
/// 跳过这一个节点 —— 把错误吞成空列表会让界面显示「没有匹配的小说」,用户会
/// 误以为是搜索词的问题。
fn filter_entries(
    entries: &[NovelFileEntry],
    filter: Option<&str>,
) -> Vec<TreeItem<'static, PathBuf>> {
    let mut result = Vec::new();
    for entry in entries {
        if entry.is_directory {
            let children = filter_entries(&entry.children, filter);
            if children.is_empty() {
                continue;
            }
            match TreeItem::new(entry.path.clone(), entry.name.clone(), children) {
                Ok(item) => result.push(item),
                Err(error) => {
                    debug_assert!(false, "同级路径重复,索引不变量被破坏: {error}");
                }
            }
        } else if matches_filter(&entry.path, filter) {
            result.push(TreeItem::new_leaf(entry.path.clone(), entry.name.clone()));
        }
    }
    result
}

fn matches_filter(path: &std::path::Path, filter: Option<&str>) -> bool {
    let Some(filter) = filter.map(str::trim).filter(|filter| !filter.is_empty()) else {
        return true;
    };

    path.file_name()
        .map(|name| {
            name.to_string_lossy()
                .to_lowercase()
                .contains(&filter.to_lowercase())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{NovelFileIndex, matches_filter};
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    #[test]
    fn filename_filter_is_case_insensitive_and_ignores_blank_query() {
        let path = Path::new("小说/Example.TXT");

        assert!(matches_filter(path, None));
        assert!(matches_filter(path, Some("  ")));
        assert!(matches_filter(path, Some("example")));
        assert!(!matches_filter(path, Some("other")));
    }

    // Given 已扫描出包含多个目录和 TXT 文件的 NovelFileIndex
    // When 连续使用不同搜索词过滤索引
    // Then 过滤只读取已有索引，不重新扫描文件系统，并返回正确的文件树
    #[test]
    fn filtering_an_index_reuses_scanned_entries() {
        let root = std::env::temp_dir().join(format!("trnovel-file-index-{}", std::process::id()));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("创建测试目录");
        fs::write(root.join("alpha.txt"), "alpha").expect("创建 alpha");
        fs::write(root.join("ignored.md"), "ignored").expect("创建 md");
        fs::write(nested.join("beta.txt"), "beta").expect("创建 beta");

        let index = NovelFileIndex::from_path(root.clone()).expect("建立文件索引");
        fs::remove_file(root.join("alpha.txt")).expect("删除 alpha");
        fs::remove_file(nested.join("beta.txt")).expect("删除 beta");
        let alpha = index.filter(Some("alpha"));
        let beta = index.filter(Some("beta"));

        assert_eq!(tree_paths(&alpha), vec![root.join("alpha.txt")]);
        assert_eq!(tree_paths(&beta), vec![nested.join("beta.txt")]);

        fs::remove_dir_all(root).expect("清理测试目录");
    }

    // Given 索引里有一个不含命中文件的子目录
    // When 用只命中根目录文件的搜索词过滤
    // Then 该子目录不出现在结果中(空目录会白占一行,还得按一次才知道是空的)
    #[test]
    fn directories_without_matches_are_dropped() {
        let root = std::env::temp_dir().join(format!("trnovel-empty-dir-{}", std::process::id()));
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("创建测试目录");
        fs::write(root.join("alpha.txt"), "alpha").expect("创建 alpha");
        fs::write(nested.join("beta.txt"), "beta").expect("创建 beta");

        let index = NovelFileIndex::from_path(root.clone()).expect("建立文件索引");

        let alpha = index.filter(Some("alpha"));
        assert_eq!(top_level_names(&alpha), vec!["alpha.txt".to_string()]);

        // 不带搜索词时,含命中文件的目录仍要保留。
        let all = index.filter(None);
        assert_eq!(
            top_level_names(&all),
            vec!["nested".to_string(), "alpha.txt".to_string()]
        );

        fs::remove_dir_all(root).expect("清理测试目录");
    }

    fn top_level_names(items: &[tui_tree_widget::TreeItem<'static, PathBuf>]) -> Vec<String> {
        items
            .iter()
            .map(|item| {
                item.identifier()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect()
    }

    fn tree_paths(items: &[tui_tree_widget::TreeItem<'static, PathBuf>]) -> Vec<PathBuf> {
        items
            .iter()
            .flat_map(|item| {
                let mut paths = if item.children().is_empty() {
                    vec![item.identifier().clone()]
                } else {
                    vec![]
                };
                paths.extend(tree_paths(item.children()));
                paths
            })
            .collect()
    }
}
