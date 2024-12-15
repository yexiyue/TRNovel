use crate::{
    components::{Component, Warning},
    errors::{Errors, Result},
    events::{event_loop, Events},
    history::History,
    pages::{local_novel::local_novel_first_page, network_novel::network_novel_first_page},
    routes::Routes,
};
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use parse_book_source::BookSource;
use ratatui::{layout::Size, DefaultTerminal, Frame};
use std::{
    fs::File,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

pub mod state;
pub use state::State;

pub struct App {
    pub show_exit: bool,
    pub event_rx: UnboundedReceiver<Events>,
    pub event_tx: UnboundedSender<Events>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub cancellation_token: CancellationToken,
    pub routes: Routes,
    pub state: State,
}

impl App {
    pub async fn new(path: PathBuf) -> Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let cancellation_token = CancellationToken::new();

        let book_source: BookSource =
            serde_json::from_reader(File::open("test-novels/test.json")?)?;

        event_loop(tx.clone(), cancellation_token.clone());

        let state = State {
            history: Arc::new(Mutex::new(History::load()?)),
            size: Arc::new(Mutex::new(None)),
            book_source: Arc::new(futures::lock::Mutex::new(Some(book_source.try_into()?))),
        };

        // let local_novel_router = local_novel_first_page(path)?;
        let first_page = network_novel_first_page()?;
        Ok(Self {
            event_tx: tx.clone(),
            event_rx: rx,
            show_exit: false,
            error: None,
            warning: None,
            routes: Routes::new(first_page, 0, state.clone()).await?,
            state,
            cancellation_token,
        })
    }

    pub fn exit(&mut self) {
        self.cancellation_token.cancel();
        self.show_exit = true;
    }

    pub fn render(&mut self, frame: &mut Frame<'_>) -> Result<()> {
        self.routes.render(frame, frame.area())?;

        if let Some(warning) = &self.error {
            frame.render_widget(Warning::new(warning, true), frame.area());
        }

        if let Some(warning) = &self.warning {
            frame.render_widget(Warning::new(warning, false), frame.area());
        }
        Ok(())
    }

    pub async fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let Some(events) = self.event_rx.recv().await else {
            return Ok(());
        };

        // 消耗型事件，如果返回了None就默认不进行消耗，如果不返回Render事件，就会导致不能渲染
        let events = if self.error.is_none() && self.warning.is_none() {
            self.routes
                .handle_events(events.clone(), self.state.clone())
                .await?
                .unwrap_or(events)
        } else {
            events
        };

        match events {
            Events::KeyEvent(key) => {
                if key.kind == KeyEventKind::Press {
                    if self.error.is_some() {
                        if key.code == KeyCode::Char('q') {
                            self.exit();
                        }
                    } else if self.warning.is_some() {
                        if key.code == KeyCode::Esc {
                            self.warning = None;
                        }
                    } else {
                        match key.code {
                            KeyCode::Char('q') => {
                                self.exit();
                            }
                            KeyCode::Char('c') => {
                                if key.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.exit();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Events::Render => {
                terminal.draw(|frame| match self.render(frame) {
                    Ok(_) => {}
                    Err(e) => {
                        self.error = Some(e.to_string());
                    }
                })?;
            }
            Events::Error(e) => self.error = Some(e),
            Events::Resize(width, height) => {
                self.state
                    .size
                    .lock()
                    .unwrap()
                    .replace(Size::new(width, height));
            }
            _ => {}
        }

        Ok(())
    }

    /// 主循环
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let size = terminal.size()?;
        self.state.size.lock().unwrap().replace(size);

        while !self.show_exit {
            // 先处理时间
            match self.handle_events(&mut terminal).await {
                Ok(_) => {}
                Err(e) => {
                    if let Errors::Warning(tip) = e {
                        self.warning = Some(tip);
                    } else {
                        self.error = Some(e.to_string());
                    }
                }
            }

            // 然后处理消息
            match self.routes.update().await {
                Ok(_) => {}
                Err(e) => {
                    if let Errors::Warning(tip) = e {
                        self.warning = Some(tip);
                    } else {
                        self.error = Some(e.to_string());
                    }
                }
            }
        }
        Ok(())
    }
}
