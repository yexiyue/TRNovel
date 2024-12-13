use app::App;
use clap::{Parser, Subcommand};
use std::{env, ffi::OsString, fmt::Debug, fs, path::PathBuf};
use utils::novel_catch_dir;

pub mod app;
pub mod cache;
pub mod components;
pub mod errors;
pub mod events;
pub mod file_list;
pub mod novel;
pub mod pages;
pub mod router;
pub mod routes;
pub mod utils;

pub use cache::*;
pub use errors::Result;
pub use events::Events;
pub use router::*;

pub async fn run() -> Result<()> {
    try_run(env::args()).await
}

pub async fn try_run<I, A>(args: I) -> Result<()>
where
    I: IntoIterator<Item = A> + Debug,
    A: Into<OsString> + Clone,
{
    let trnovel = TRNovel::parse_from(args);

    if let Some(Commands::Clear) = trnovel.subcommand {
        fs::remove_dir_all(novel_catch_dir()?)?;
        return Ok(());
    }

    let terminal = ratatui::init();

    App::new(trnovel).await?.run(terminal).await?;

    ratatui::restore();

    Ok(())
}
/// TRNovel(Terminal reader for novel)，一个终端小说阅读器
#[derive(Parser, Debug)]
#[command(author, version)]
pub struct TRNovel {
    /// 小说文件夹路径，默认为当前目录
    #[arg(default_value = "./")]
    pub path: PathBuf,

    #[command(subcommand)]
    pub subcommand: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// 快速模式，接着上一次阅读的位置继续阅读
    #[command(short_flag = 'q')]
    Quick,

    /// 清空历史记录和小说缓存
    #[command(short_flag = 'c')]
    Clear,

    /// 网络模式，使用网络小说源
    #[command(short_flag = 'n')]
    Network,

    /// 历史记录模式，查看阅读记录
    History,
}
