use anyhow::Result;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use futures::{stream::StreamExt, FutureExt};
use ratatui::{DefaultTerminal, Frame};
use std::{path::PathBuf, time::Duration};

mod mode;
use mode::Mode;

use crate::{
    components::{
        select_novel::{select_file::SelectFile, select_history::SelectHistory, SelectNovel},
        warning::Warning,
        Component,
    },
    events::Events,
};

pub struct App<'a> {
    pub mode: Mode<'a>,
    pub prev_mode: Option<Mode<'a>>,
    pub show_exit: bool,
    pub event_rx: tokio::sync::mpsc::UnboundedReceiver<Events>,
    pub event_tx: tokio::sync::mpsc::UnboundedSender<Events>,
    pub warning: Option<String>,
}

impl<'a> App<'a> {
    pub fn new(path: PathBuf) -> Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

        tx.send(Events::Init(path))?;
        let tx_clone = tx.clone();
        let tx_clone2 = tx.clone();

        tokio::spawn(async move {
            let mut events = crossterm::event::EventStream::new();
            while let Some(Ok(event)) = events.next().fuse().await {
                tx_clone.send(Events::CrosstermEvent(event)).unwrap();
            }
        });

        tokio::spawn(async move {
            let mut time = tokio::time::interval(Duration::from_millis(300));
            loop {
                time.tick().await;
                tx_clone2.send(Events::Tick).unwrap();
            }
        });

        Ok(Self {
            mode: Mode::default(),
            prev_mode: None,
            show_exit: false,
            event_rx: rx,
            event_tx: tx,
            warning: None,
        })
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        while !self.show_exit {
            terminal.draw(|frame| match self.draw(frame) {
                Ok(_) => {}
                Err(e) => {
                    self.warning = Some(e.to_string());
                }
            })?;
            match self.handle_events().await {
                Ok(_) => {}
                Err(e) => {
                    self.warning = Some(e.to_string());
                }
            }
        }
        Ok(())
    }

    pub async fn handle_events(&mut self) -> Result<()> {
        let Some(event) = self.event_rx.recv().await else {
            return Ok(());
        };

        if let Some(_) = &self.warning {
            match event {
                Events::CrosstermEvent(event) => match event {
                    Event::Key(key) => {
                        if key.kind == KeyEventKind::Press {
                            match key.code {
                                KeyCode::Esc => self.warning = None,
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
            return Ok(());
        }

        self.mode
            .handle_events(event.clone(), self.event_tx.clone())?;

        match event {
            Events::CrosstermEvent(event) => match event {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => self.show_exit = true,
                            KeyCode::Char('c') => {
                                if key.modifiers.contains(KeyModifiers::CONTROL) {
                                    self.show_exit = true;
                                }
                            }
                            KeyCode::Backspace => {
                                if let Mode::Read(_) = self.mode {
                                    if self.prev_mode.is_some() {
                                        self.mode = self.prev_mode.clone().unwrap();
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            },
            Events::SelectNovel((tree, history)) => {
                self.prev_mode = Some(Mode::Select(SelectNovel::new(
                    SelectFile::new(tree)?,
                    SelectHistory::new(history.histories),
                )?));
            }
            _ => {}
        }
        Ok(())
    }

    pub fn draw(&mut self, frame: &mut Frame<'_>) -> anyhow::Result<()> {
        self.mode.draw(frame, frame.area())?;

        if let Some(warning) = &self.warning {
            frame.render_widget(Warning::new(warning), frame.area());
        }
        Ok(())
    }
}
