use anyhow::Result;
use std::path::{Path, PathBuf};
use tui_tree_widget::TreeItem;
use walkdir::WalkDir;

const FILE_EXTS: [&str; 1] = ["txt"];

/// 一次目录扫描的结果快照。
///
/// 扫描只在**路径变化**时发生,之后每次改搜索词都只在这份内存索引上过滤 ——
/// 不再碰文件系统。书库上万本时连续改词也不会卡。
#[derive(Debug)]
pub struct NovelFileIndex {
    entries: Vec<NovelFileEntry>,
}

#[derive(Debug)]
struct NovelFileEntry {
    path: PathBuf,
    name: String,
    /// 预先算好的小写文件名。过滤发生在每次提交搜索词时,不能每次都为每个
    /// 条目重做一遍 `OsStr → String → to_lowercase` —— 那正是建索引要省掉的。
    name_lower: String,
    kind: EntryKind,
}

/// 文件与目录的区别用 enum 表达,而不是 `is_directory: bool` + `children`:
/// 后者允许「是文件却有子节点」这种非法状态存在,而扫描保证它永不发生。
#[derive(Debug)]
enum EntryKind {
    File,
    /// 目录只有在含至少一个受支持文件时才入索引,故子节点恒非空。
    Directory(Vec<NovelFileEntry>),
}

impl NovelFileIndex {
    /// 扫描目录建立索引;传入单个文件时索引里就只有它。
    pub fn from_path(path: PathBuf) -> Result<Self> {
        let path = if path.is_relative() {
            std::env::current_dir()?.join(path)
        } else {
            path
        };

        let entries = if path.is_file() {
            if !has_supported_ext(&path) {
                return Err(anyhow::anyhow!("不支持的文件类型"));
            }
            vec![NovelFileEntry::new(path, EntryKind::File)]
        } else {
            scan(&path)?
        };

        Ok(Self { entries })
    }

    /// 按文件名筛选出文件树。空查询返回全部。
    ///
    /// 只读内存索引,不碰文件系统。
    pub fn filter(&self, query: &str) -> Vec<TreeItem<'static, PathBuf>> {
        // 查询词只规范化一次 —— 放进逐条目的匹配里会对每个文件重做一遍。
        let query = query.trim().to_lowercase();
        build_tree(&self.entries, &query)
    }
}

impl NovelFileEntry {
    fn new(path: PathBuf, kind: EntryKind) -> Self {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            name_lower: name.to_lowercase(),
            name,
            path,
            kind,
        }
    }
}

/// 递归扫描一层目录。不含受支持文件的目录不会进索引。
fn scan(dir: &Path) -> Result<Vec<NovelFileEntry>> {
    let mut entries = Vec::new();

    let walker = WalkDir::new(dir)
        // min_depth(1) 跳过 dir 自身,省掉逐条「是不是根」的比较。
        .min_depth(1)
        .max_depth(1)
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
        });

    for entry in walker {
        let entry = entry?;
        // 判定一律走 walkdir 缓存的 file_type():`Path::is_dir()` 每次都打一次
        // metadata syscall,而且**跟随符号链接**——与上面排序用的判定不一致,
        // 会让指向目录的软链「排序时算文件、遍历时算目录」,顺序不符合目录优先。
        let is_dir = entry.file_type().is_dir();
        let path = entry.into_path();

        if is_dir {
            let children = scan(&path)?;
            if children.is_empty() {
                continue;
            }
            entries.push(NovelFileEntry::new(path, EntryKind::Directory(children)));
        } else if has_supported_ext(&path) {
            entries.push(NovelFileEntry::new(path, EntryKind::File));
        }
    }

    Ok(entries)
}

/// 命中的文件逐层保留其所在目录;过滤后为空的目录不进结果。
fn build_tree(entries: &[NovelFileEntry], query: &str) -> Vec<TreeItem<'static, PathBuf>> {
    let mut items = Vec::new();

    for entry in entries {
        match &entry.kind {
            EntryKind::Directory(children) => {
                let children = build_tree(children, query);
                if children.is_empty() {
                    continue;
                }
                // `TreeItem::new` 只在同级 identifier 重复时失败,而 identifier 是
                // 文件系统路径、同级天然唯一 —— 该分支不可达。真出现了也只跳过这
                // 一个节点:整棵树置空会让界面显示「没有匹配的小说」,把索引的问题
                // 伪装成搜索词的问题。
                match TreeItem::new(entry.path.clone(), entry.name.clone(), children) {
                    Ok(item) => items.push(item),
                    Err(error) => debug_assert!(false, "同级路径重复: {error}"),
                }
            }
            EntryKind::File if entry.name_lower.contains(query) => {
                items.push(TreeItem::new_leaf(entry.path.clone(), entry.name.clone()));
            }
            EntryKind::File => {}
        }
    }

    items
}

/// 扩展名比较不区分大小写 —— Windows 上 `.TXT` 很常见,漏掉它等于书库少一半。
fn has_supported_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            FILE_EXTS
                .iter()
                .any(|supported| ext.eq_ignore_ascii_case(supported))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 建一个测试专用的临时目录。开头先清一次:上一轮若在断言处 panic,
    /// 清理不会执行,残留会让下一轮读到脏数据。
    fn temp_dir(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("trnovel-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("创建测试目录");
        root
    }

    /// 树里所有叶子的路径,按出现顺序。
    fn leaf_paths(items: &[TreeItem<'static, PathBuf>]) -> Vec<PathBuf> {
        items
            .iter()
            .flat_map(|item| {
                if item.children().is_empty() {
                    vec![item.identifier().clone()]
                } else {
                    leaf_paths(item.children())
                }
            })
            .collect()
    }

    /// 顶层节点的显示名,按出现顺序。
    fn top_level_names(items: &[TreeItem<'static, PathBuf>]) -> Vec<String> {
        items
            .iter()
            .map(|item| {
                item.identifier()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    // Given 已扫描出包含多个目录和 TXT 文件的索引
    // When 连续用不同搜索词过滤
    // Then 过滤只读已有索引、不重新扫盘(扫描后把文件删掉仍能返回)
    #[test]
    fn filtering_reuses_the_scanned_index() {
        let root = temp_dir("file-index");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("创建子目录");
        fs::write(root.join("alpha.txt"), "alpha").expect("创建 alpha");
        fs::write(root.join("ignored.md"), "ignored").expect("创建 md");
        fs::write(nested.join("beta.txt"), "beta").expect("创建 beta");

        let index = NovelFileIndex::from_path(root.clone()).expect("建立文件索引");

        // 索引建好后把文件删掉:仍能过滤出来 = 确实没再碰文件系统。
        fs::remove_file(root.join("alpha.txt")).expect("删除 alpha");
        fs::remove_file(nested.join("beta.txt")).expect("删除 beta");

        assert_eq!(
            leaf_paths(&index.filter("alpha")),
            vec![root.join("alpha.txt")]
        );
        assert_eq!(
            leaf_paths(&index.filter("beta")),
            vec![nested.join("beta.txt")]
        );

        let _ = fs::remove_dir_all(root);
    }

    // Given 索引里有一个不含命中文件的子目录
    // When 用只命中根目录文件的搜索词过滤
    // Then 该子目录不出现在结果里(空目录白占一行,还得展开才知道是空的)
    #[test]
    fn directories_without_matches_are_dropped() {
        let root = temp_dir("empty-dir");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("创建子目录");
        fs::write(root.join("alpha.txt"), "alpha").expect("创建 alpha");
        fs::write(nested.join("beta.txt"), "beta").expect("创建 beta");

        let index = NovelFileIndex::from_path(root.clone()).expect("建立文件索引");

        assert_eq!(top_level_names(&index.filter("alpha")), vec!["alpha.txt"]);
        // 空查询返回全部,含命中文件的目录仍在,且目录排在文件前面。
        assert_eq!(
            top_level_names(&index.filter("")),
            vec!["nested", "alpha.txt"]
        );

        let _ = fs::remove_dir_all(root);
    }

    // Given 文件名与扩展名都含大写
    // When 用小写词过滤
    // Then 都能命中 —— 扩展名与文件名匹配都不区分大小写
    #[test]
    fn matching_ignores_case_for_both_name_and_extension() {
        let root = temp_dir("case");
        fs::write(root.join("Example.TXT"), "example").expect("创建 Example.TXT");
        fs::write(root.join("other.txt"), "other").expect("创建 other.txt");

        let index = NovelFileIndex::from_path(root.clone()).expect("建立文件索引");

        // .TXT 也要被扫进来。
        assert_eq!(index.filter("").len(), 2);
        assert_eq!(
            top_level_names(&index.filter("example")),
            vec!["Example.TXT"]
        );
        assert_eq!(
            top_level_names(&index.filter("EXAMPLE")),
            vec!["Example.TXT"]
        );
        // 纯空白等同于空查询,返回全部而不是零结果。
        assert_eq!(index.filter("   ").len(), 2);
        assert!(index.filter("missing").is_empty());

        let _ = fs::remove_dir_all(root);
    }

    // Given 直接指向单个小说文件的路径
    // When 建立索引
    // Then 索引里只有它;非受支持类型则报错
    #[test]
    fn single_file_path_is_indexed_and_validated() {
        let root = temp_dir("single-file");
        let book = root.join("solo.txt");
        fs::write(&book, "solo").expect("创建 solo.txt");
        fs::write(root.join("note.md"), "note").expect("创建 note.md");

        let index = NovelFileIndex::from_path(book.clone()).expect("建立文件索引");
        assert_eq!(leaf_paths(&index.filter("")), vec![book]);

        assert!(NovelFileIndex::from_path(root.join("note.md")).is_err());

        let _ = fs::remove_dir_all(root);
    }
}
