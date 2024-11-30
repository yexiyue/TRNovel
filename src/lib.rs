pub mod app;
pub mod components;
use std::path::PathBuf;

pub mod errors;
pub mod events;
pub mod file_list;
pub mod history;
pub mod novel;
pub mod routes;
pub mod utils;

use app::App;
use clap::{Parser, Subcommand};
pub async fn run() -> anyhow::Result<()> {
    let args = NovelTUI::parse();
    let terminal = ratatui::init();

    App::new(args.path)?.run(terminal).await?;

    ratatui::restore();

    Ok(())
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct NovelTUI {
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
}
