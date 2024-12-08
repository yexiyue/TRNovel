pub mod app;
pub mod components;
use std::{fs, path::PathBuf};

pub mod errors;
pub mod events;
pub mod file_list;
pub mod history;
pub mod novel;
pub mod routes;
pub mod utils;

use app::App;
use clap::{Parser, Subcommand};
use utils::novel_catch_dir;

pub async fn run() -> anyhow::Result<()> {
    let args = TRNovel::parse();

    if let Some(Commands::Clear) = args.subcommand {
        fs::remove_dir_all(novel_catch_dir()?)?;
        return Ok(());
    }

    let terminal = ratatui::init();

    App::new(args.path)?.run(terminal).await?;

    ratatui::restore();

    Ok(())
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
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
}
