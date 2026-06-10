use crate::{
    Result,
    errors::Errors,
    novel::{Novel, network_novel::NetworkNovel},
    utils::{get_md5_string, novel_catch_dir},
};
use parse_book_source::BookListItem;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt::Display, fs::File, path::PathBuf};

/// 历史记录（一个用于展示，一个用于缓存，方便下次快速访问）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkNovelCache {
    #[serde(flatten)]
    pub book_list_item: BookListItem,
    pub book_source_url: String,
    pub book_source_name: String,
    pub current_chapter: usize,
    pub current_chapter_name: String,
    pub line_percent: f64,
    pub chapter_percent: f64,
    /// 书籍级捕获变量(书源 `scope=book` 的多步 vars,见 js-host-bridge D7-bis):
    /// 随本书快照跨会话复用(如详情/列表捕获的 token 带入目录/正文)。旧快照无此字段靠 default 兼容。
    #[serde(default)]
    pub book_vars: BTreeMap<String, String>,
}

impl NetworkNovelCache {
    pub fn save(&self) -> Result<()> {
        let cache_path = Self::cache_path(&self.book_list_item.book_url)?;
        let file = File::create(cache_path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn cache_path<T: Display>(url: T) -> Result<PathBuf> {
        let novel_catch_dir = PathBuf::new().join(novel_catch_dir()?).join("network");

        if !novel_catch_dir.exists() {
            std::fs::create_dir_all(&novel_catch_dir)?;
        }

        Ok(novel_catch_dir
            .join(get_md5_string(url))
            .with_extension("json"))
    }
}

// 从本地小说创建缓存
impl TryFrom<&NetworkNovel> for NetworkNovelCache {
    type Error = Errors;
    fn try_from(value: &NetworkNovel) -> Result<Self> {
        let novel_chapters = value.novel_chapters.clone();
        let source = value.engine.source();

        // 回写引擎本会话累积的 persistent cookie(enabledCookieJar 回灌,如服务端轮换/续签的
        // token)到 per-source 登录态——否则只活在进程内,重启即丢、用户被迫重登。
        // 本路径随阅读退出/历史保存频繁触发,失败吞错(贴合 Drop 自动保存吞错约定),不阻断退出。
        let persistent = value.engine.persistent_cookies();
        if !persistent.is_empty() {
            let source_url = value.engine.source_url();
            let mut state = crate::cache::load_source_state(source_url);
            for (dom, cookie) in persistent {
                // 逐域 merge(引擎新值同名优先)而非整串覆盖:不清掉登录流程写入的同域其它
                // cookie;代价是 Max-Age=0 已删除的同名旧键可能复活,靠 TTL 清理兜底。
                let merged = parse_book_source::cookie::merge_cookie_str(
                    state.cookies.get(&dom).map(String::as_str).unwrap_or(""),
                    &cookie,
                );
                state.cookies.insert(dom, merged);
            }
            let _ = crate::cache::save_source_state(source_url, &state);
        }

        Ok(Self {
            current_chapter: novel_chapters.current_chapter,
            current_chapter_name: value.get_current_chapter_name()?,
            line_percent: novel_chapters.line_percent,
            book_list_item: value.book_list_item.clone(),
            book_source_url: source.url.clone(),
            book_source_name: source.name.clone(),
            chapter_percent: (value.current_chapter as f64
                / value.get_chapters_result()?.len() as f64)
                * 100.0,
            // 导出书籍级捕获变量,随快照落盘(scope=book 跨会话复用)。
            book_vars: value.engine.book_vars(),
        })
    }
}

// 从路径加载缓存
impl TryFrom<&str> for NetworkNovelCache {
    type Error = Errors;
    fn try_from(value: &str) -> Result<Self> {
        let cache_path = Self::cache_path(value)?;
        let file = File::open(cache_path)?;
        Ok(serde_json::from_reader(file)?)
    }
}
