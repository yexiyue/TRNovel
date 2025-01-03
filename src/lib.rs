use app::App;
use clap::{Parser, Subcommand};
use crossterm::style::Stylize;
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
pub mod quick_start;
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
    let size = terminal.size()?;

    match App::new(trnovel, size).await {
        Ok(app) => {
            app.run(terminal).await?;
            ratatui::restore();
        }
        Err(e) => {
            ratatui::restore();
            eprintln!("{}: {}", "Error".red().bold(), e.to_string().red());
        }
    }
    Ok(())
}

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = r#"
  _______ _____  _   _                _ 
 |__   __|  __ \| \ | |              | |
    | |  | |__) |  \| | _____   _____| |
    | |  |  _  /| . ` |/ _ \ \ / / _ \ |
    | |  | | \ \| |\  | (_) \ V /  __/ |
    |_|  |_|  \_\_| \_|\___/ \_/ \___|_|
                                            
  终端小说阅读器 (Terminal reader for novel)
  ==========================================

  TRNovel 是一个终端小说阅读器，支持以下功能。
    - 本地小说
    - 网络小说
    - 历史记录

  GitHub: https://github.com/yexiyue/trnovel
  
  如果您觉得还不错，请考虑给项目点个 star，谢谢！
"#
)]
pub struct TRNovel {
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

    /// 本地小说
    #[command(short_flag = 'l')]
    Local {
        /// 小说文件夹路径
        path: Option<PathBuf>,
    },

    /// 历史记录模式，查看阅读记录
    #[command(short_flag = 'H')]
    History,
}
