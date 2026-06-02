#![allow(clippy::needless_update)]

use app::App;
use clap::{Parser, Subcommand};
use ratatui_kit::{ElementExt, element};
use std::{env, ffi::OsString, fmt::Debug, fs, path::PathBuf};
use utils::novel_catch_dir;

pub mod app;
pub mod browser_assist;
pub mod cache;
pub mod components;
pub mod doctor;
pub mod errors;
pub mod file_list;
pub mod hooks;
pub mod import;
pub mod novel;
pub mod pages;
pub mod utils;

pub use cache::*;
pub use errors::Result;

use crate::app::AppProps;

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

    // 书源体检:非 TUI,跑全流程后打印 ✓/✗ 列表并退出。
    if let Some(Commands::Doctor { path }) = &trnovel.subcommand {
        doctor::run(path).await;
        return Ok(());
    }

    // 导入书源:非 TUI,把书源 JSON(文件/URL)写入 ~/.novel 后退出。
    if let Some(Commands::Import { source }) = &trnovel.subcommand {
        import::run(source).await;
        return Ok(());
    }

    let props = AppProps { trnovel };

    element!(App(..props)).fullscreen().await?;

    Ok(())
}

#[derive(Parser, Debug, Clone, Hash, PartialEq, Eq)]
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
    - 主题设置

  GitHub: https://github.com/yexiyue/trnovel
  
  如果您觉得还不错，请考虑给项目点个 star，谢谢！
"#
)]
pub struct TRNovel {
    #[command(subcommand)]
    pub subcommand: Option<Commands>,
}

#[derive(Debug, Subcommand, Clone, Hash, PartialEq, Eq)]
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

    /// 体检书源：全流程验证书源 JSON,逐项报告 ✓/✗(用于校验 AI 生成的书源)
    #[command(short_flag = 'd')]
    Doctor {
        /// 书源 JSON 文件路径
        path: PathBuf,
    },

    /// 导入书源：把书源 JSON(本地文件或 URL)写入 ~/.novel,使其在网络小说里可用
    #[command(short_flag = 'i')]
    Import {
        /// 书源 JSON 文件路径或 URL
        source: String,
    },
}
