//! per-source 登录态(`SourceState`)的 app 侧持久化(`~/.novel/source-state/{url_md5}.json`)。
//!
//! 库(`parse-book-source`)的 [`SourceState`] 不硬编码 `~/.novel` 路径,保持纯净;
//! 这里给定路径、做加载时 TTL 清理与落盘(含 unix 0600 权限,见库侧 `state.rs`)。
//! 登录态(loginHeader / cookies / 加密 loginInfo)由本模块管理,经 `build_engine` 注入每个引擎。

use crate::{
    Result,
    utils::{get_md5_string, novel_catch_dir},
};
use parse_book_source::state::SourceState;
use std::path::PathBuf;

/// 某书源登录态文件路径(`~/.novel/source-state/{url_md5}.json`)。
/// 纯路径计算,不做文件系统副作用;建目录由保存方负责(库侧 `SourceState::save` 已建父目录)。
pub fn source_state_path(source_url: &str) -> Result<PathBuf> {
    Ok(novel_catch_dir()?
        .join("source-state")
        .join(get_md5_string(source_url))
        .with_extension("json"))
}

/// 加载书源登录态;若已过期(TTL)则清登录态并落盘,返回清理后的状态。
/// 文件缺失/损坏返回默认空状态(库侧 [`SourceState::load`] 已容错)。
pub fn load_source_state(source_url: &str) -> SourceState {
    let Ok(path) = source_state_path(source_url) else {
        return SourceState::default();
    };
    let mut state = SourceState::load(&path);
    if state.purge_if_expired() {
        let _ = state.save(&path);
    }
    state
}

/// 保存书源登录态(0600 落盘,保护明文 loginHeader/cookie 与加密 loginInfo)。
pub fn save_source_state(source_url: &str, state: &SourceState) -> Result<()> {
    let path = source_state_path(source_url)?;
    state.save(&path)?;
    Ok(())
}
