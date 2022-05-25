use std::io::Write;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use tui::{backend::CrosstermBackend, Terminal};

pub fn init_terminal() -> std::io::Result<Terminal<CrosstermBackend<impl Write>>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);

    Ok(Terminal::new(backend)?)
}

pub fn deinit_terminal(
    mut terminal: Terminal<CrosstermBackend<impl Write>>,
) -> std::io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

#[macro_export]
macro_rules! temp_mute_terminal {
    ($terminal:expr, $code:block) => {
        disable_raw_mode()?;
        execute!($terminal.backend_mut(), LeaveAlternateScreen)?;
        $terminal.show_cursor()?;
        $code();
        enable_raw_mode()?;
        execute!($terminal.backend_mut(), EnterAlternateScreen)?;
        $terminal.hide_cursor()?;
        $terminal.clear()?;
    };
}
