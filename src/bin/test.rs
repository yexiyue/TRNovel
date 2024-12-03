use anyhow::Result;
use crossterm::event;
use ratatui::widgets::Clear;
use trnovel::components::table::Cell;

fn main() -> Result<()> {
    let mut terminal = ratatui::init();

    loop {
        terminal.draw(|f| {
            let cell = Cell::from("hello");
            f.render_widget(cell, f.area());
        })?;

        if let event::Event::Key(key) = event::read()? {
            if key.code == event::KeyCode::Char('q') {
                break;
            }
        }
    }

    ratatui::restore();
    Ok(())
}
