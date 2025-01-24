use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, MouseEventKind};
use futures::{FutureExt, StreamExt};
use std::time::Duration;
use tokio::{sync::mpsc::UnboundedSender, time::interval};
use tokio_util::sync::CancellationToken;

// 不需要Render事件是因为每次事件结束后会render一次，之前是一秒render60次会导致在小说文件比较大的情况下会导致事件响应卡顿
#[derive(Debug, Clone)]
pub enum Events {
    Tick,
    KeyEvent(crossterm::event::KeyEvent),
    // 鼠标事件
    MouseEvent(crossterm::event::MouseEvent),
    Resize(u16, u16),
    Error(String),
}

pub fn event_loop(event_tx: UnboundedSender<Events>, cancellation_token: CancellationToken) {
    tokio::spawn(async move {
        let mut events = EventStream::new();
        let mut tick_interval = interval(Duration::from_secs_f64(1.0 / 4.0));
        loop {
            let event = tokio::select! {
                _ = tick_interval.tick()=>{
                    Events::Tick
                }
                _ = cancellation_token.cancelled()=>{
                    break;
                }
                crossterm_event = events.next().fuse()=>{
                    match crossterm_event {
                        Some(Ok(event)) => {
                            match event {
                                Event::Key(key_event) => {
                                    Events::KeyEvent(key_event)
                                },
                                Event::Resize(width, height) => {
                                    Events::Resize(width, height)
                                },
                                Event::Mouse(mouse)=>{
                                    // 鼠标滚轮事件模拟成键盘Up和Down事件
                                    match mouse.kind{
                                        MouseEventKind::ScrollUp=>{
                                            Events::KeyEvent(KeyEvent::new(KeyCode::Up, mouse.modifiers))
                                        }
                                        MouseEventKind::ScrollDown=>{
                                            Events::KeyEvent(KeyEvent::new(KeyCode::Down, mouse.modifiers))
                                        }
                                        _=>{
                                            Events::MouseEvent(mouse)
                                        }
                                    }
                                }
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
