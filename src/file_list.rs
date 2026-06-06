use anyhow::Result;
use std::{ffi::OsStr, path::PathBuf};
use tui_tree_widget::TreeItem;
use walkdir::WalkDir;

const FILE_EXTS: [&str; 1] = ["txt"];

#[derive(Debug, Clone)]
pub enum NovelFiles<'a> {
    File(PathBuf),
    FileTree(Vec<TreeItem<'a, PathBuf>>),
}

impl<'a> NovelFiles<'a> {
    pub fn from_path(path: PathBuf) -> Result<NovelFiles<'a>> {
        let path = if path.is_relative() {
            std::env::current_dir()?.join(path)
        } else {
            path
        };

        if path.is_file() {
            path.extension()
                .and_then(|ext| ext.to_str())
                .and_then(|ext| FILE_EXTS.iter().find(|&&e| e == ext))
                .ok_or_else(|| anyhow::anyhow!("不支持的文件类型"))?;

            Ok(NovelFiles::File(path))
        } else {
            Ok(NovelFiles::FileTree(find_novels(path, &FILE_EXTS)?))
        }
    }

    pub fn into_tree_item(self) -> Vec<TreeItem<'a, PathBuf>> {
        match self {
            NovelFiles::File(path) => vec![TreeItem::new_leaf(
                path.clone(),
                path.file_name().unwrap().to_string_lossy().to_string(),
            )],
            NovelFiles::FileTree(items) => items,
        }
    }
}

fn find_novels<'a>(path: PathBuf, file_exts: &[&str]) -> Result<Vec<TreeItem<'a, PathBuf>>> {
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
            b_is_dir
                .cmp(&a_is_dir)
                .then_with(|| a.path().cmp(b.path()))
        })
        .max_depth(1);

    for entity in walkdir {
        let entity = entity?;

        if entity.path().to_path_buf() == path {
            continue;
        }
        if entity.path().is_dir() {
            let children = find_novels(entity.clone().into_path(), file_exts)?;
            if children.is_empty() {
                continue;
            }
            res.push(TreeItem::new(
                entity.clone().into_path(),
                entity.file_name().to_string_lossy().to_string(),
                children,
            )?);
        } else if entity.path().is_file()
            && file_exts
                .iter()
                .any(|&e| e == entity.path().extension().unwrap_or(OsStr::new("")))
        {
            res.push(TreeItem::new_leaf(
                entity.clone().into_path(),
                entity.file_name().to_string_lossy().to_string(),
            ));
        }
    }
    Ok(res)
}
