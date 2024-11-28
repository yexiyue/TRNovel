use crossterm::event::EventStream;
use futures::{FutureExt, StreamExt};
use std::time::Duration;
use tokio::{sync::mpsc::UnboundedSender, time::interval};
use tokio_util::sync::CancellationToken;

use crate::routes::Route;

#[derive(Debug, Clone)]
pub enum Events {
    Tick,
    Render,
    Go,
    Back,
    Pop,
    KeyEvent(crossterm::event::KeyEvent),
    PushRoute(Route),
    ReplaceRoute(Route),
    Resize(u16, u16),
    Error(String),
}

pub fn event_loop(event_tx: UnboundedSender<Events>, cancellation_token: CancellationToken) {
    tokio::spawn(async move {
        let mut events = EventStream::new();
        let mut tick_interval = interval(Duration::from_secs_f64(1.0 / 4.0));
        let mut render_interval = interval(Duration::from_secs_f64(1.0 / 60.0));
        loop {
            let event = tokio::select! {
                _ = tick_interval.tick()=>{
                    Events::Tick
                }
                _ = render_interval.tick()=>{
                    Events::Render
                }
                _ = cancellation_token.cancelled()=>{
                    break;
                }
                crossterm_event = events.next().fuse()=>{
                    match crossterm_event {
                        Some(Ok(event)) => {
                            match event {
                                crossterm::event::Event::Key(key_event) => {
                                    Events::KeyEvent(key_event)
                                },
                                crossterm::event::Event::Resize(width, height) => {
                                    Events::Resize(width, height)
                                },
                                _ => continue
                            }
                        },
                        Some(Err(err)) => Events::Error(err.to_string()),
                        None => break,
                    }
                }

            };

            if event_tx.send(event).is_err() {
                // 如果发送失败，说明接收端已经关闭，退出循环
                break;
            }
        }
        cancellation_token.cancel();
    });
}
