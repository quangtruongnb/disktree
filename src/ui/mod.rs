pub mod statusbar;
pub mod tree;

use crate::scanner::{DirEntry, ScanResult};
use crate::trash;
use bytesize::ByteSize;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, List, ListState, Paragraph};
use ratatui::Frame;
use std::sync::mpsc::Receiver;
use std::time::Duration;

pub enum AppState {
    Scanning { tick: usize },
    Ready,
}

pub struct App {
    pub state: AppState,
    pub root: DirEntry,
    /// Stack of selected child indices at each nav level
    pub nav_stack: Vec<usize>,
    pub list_state: ListState,
    pub skipped_count: usize,
    pub status_message: Option<String>,
    pub confirm_trash: bool,
    pub should_quit: bool,
}

impl App {
    pub fn new_scanning(scan_path: &std::path::Path) -> Self {
        // Placeholder empty root while scanning
        let root = DirEntry {
            name: scan_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| scan_path.to_string_lossy().to_string()),
            path: scan_path.to_path_buf(),
            size: 0,
            is_dir: true,
            children: vec![],
            flag: None,
        };
        App {
            state: AppState::Scanning { tick: 0 },
            root,
            nav_stack: vec![],
            list_state: ListState::default(),
            skipped_count: 0,
            status_message: None,
            confirm_trash: false,
            should_quit: false,
        }
    }

    pub fn new(root: DirEntry, skipped_count: usize) -> Self {
        let mut list_state = ListState::default();
        if !root.children.is_empty() {
            list_state.select(Some(0));
        }
        App {
            state: AppState::Ready,
            root,
            nav_stack: vec![],
            list_state,
            skipped_count,
            status_message: None,
            confirm_trash: false,
            should_quit: false,
        }
    }

    pub fn apply_scan_result(&mut self, result: ScanResult, home: &std::path::Path) {
        let flagged = crate::highlight::apply_flags(result.root, home);
        self.skipped_count = result.skipped_count;
        self.root = flagged;
        self.state = AppState::Ready;
        if !self.root.children.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    /// Return the children of the currently navigated directory.
    pub fn current_children(&self) -> &[DirEntry] {
        let mut node = &self.root;
        for &idx in &self.nav_stack {
            if idx < node.children.len() {
                node = &node.children[idx];
            } else {
                return &[];
            }
        }
        &node.children
    }

    pub fn current_dir_size(&self) -> u64 {
        let mut node = &self.root;
        for &idx in &self.nav_stack {
            if idx < node.children.len() {
                node = &node.children[idx];
            } else {
                return 0;
            }
        }
        node.size
    }

    pub fn current_path(&self) -> String {
        let mut node = &self.root;
        for &idx in &self.nav_stack {
            if idx < node.children.len() {
                node = &node.children[idx];
            } else {
                break;
            }
        }
        node.path.to_string_lossy().to_string()
    }

    /// The currently selected entry (if any).
    pub fn selected_entry(&self) -> Option<&DirEntry> {
        let selected = self.list_state.selected()?;
        self.current_children().get(selected)
    }

    pub fn next(&mut self) {
        let len = self.current_children().len();
        if len == 0 {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some((current + 1).min(len - 1)));
    }

    pub fn previous(&mut self) {
        let len = self.current_children().len();
        if len == 0 {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0);
        self.list_state.select(Some(current.saturating_sub(1)));
    }

    pub fn enter(&mut self) {
        let selected = match self.list_state.selected() {
            Some(i) => i,
            None => return,
        };
        let children = self.current_children();
        if selected >= children.len() {
            return;
        }
        if !children[selected].is_dir {
            return;
        }
        self.nav_stack.push(selected);
        let new_len = self.current_children().len();
        if new_len > 0 {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    pub fn go_back(&mut self) {
        if let Some(parent_idx) = self.nav_stack.pop() {
            self.list_state.select(Some(parent_idx));
        }
    }

    pub fn jump_to_root(&mut self) {
        self.nav_stack.clear();
        let len = self.root.children.len();
        if len > 0 {
            self.list_state.select(Some(0));
        } else {
            self.list_state.select(None);
        }
    }

    pub fn initiate_trash(&mut self) {
        let name = self.selected_entry().map(|e| e.name.clone());
        if let Some(name) = name {
            self.confirm_trash = true;
            self.status_message = Some(format!("Move '{}' to Trash? [y/n]", name));
        }
    }

    pub fn confirm_trash_action(&mut self) {
        if !self.confirm_trash {
            return;
        }
        self.confirm_trash = false;

        let target_path = match self.selected_entry() {
            Some(e) => e.path.clone(),
            None => {
                self.status_message = None;
                return;
            }
        };

        match trash::move_to_trash(&target_path) {
            Ok(()) => {
                let updated_root = trash::remove_entry(self.root.clone(), &target_path);
                self.root = updated_root;
                // Clamp selection
                let len = self.current_children().len();
                let sel = self.list_state.selected().unwrap_or(0);
                if len == 0 {
                    self.list_state.select(None);
                } else {
                    self.list_state.select(Some(sel.min(len - 1)));
                }
                self.status_message = None;
            }
            Err(e) => {
                self.status_message = Some(format!("Trash failed: {:?}", e));
            }
        }
    }

    pub fn cancel_trash(&mut self) {
        self.confirm_trash = false;
        self.status_message = None;
    }
}

const SPINNER: [char; 4] = ['|', '/', '-', '\\'];

fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let [header_area, list_area, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Header
    let total_size = ByteSize::b(app.current_dir_size()).to_string();
    let header_text = format!(" disk-tree  {}    Total: {}", app.current_path(), total_size);
    let header = Paragraph::new(Line::from(header_text))
        .style(Style::default().fg(Color::White).bg(Color::DarkGray));
    frame.render_widget(header, header_area);

    match &app.state {
        AppState::Scanning { tick } => {
            let spinner = SPINNER[tick % 4];
            let msg = Paragraph::new(format!("  {} Scanning...", spinner))
                .style(Style::default().fg(Color::Yellow));
            frame.render_widget(msg, list_area);
        }
        AppState::Ready => {
            let parent_size = app.current_dir_size();
            let items: Vec<_> =
                tree::build_list_items(app.current_children(), parent_size, area.width);
            let is_empty = items.is_empty();

            if is_empty {
                let empty_msg = Paragraph::new("  (empty)")
                    .style(Style::default().fg(Color::DarkGray));
                frame.render_widget(empty_msg, list_area);
            } else {
                let list = List::new(items)
                    .block(Block::default())
                    .highlight_style(Style::default().bg(Color::Blue).fg(Color::White));
                frame.render_stateful_widget(list, list_area, &mut app.list_state);
            }
        }
    }

    // Status bar
    let status = statusbar::build_status_bar(
        app.skipped_count,
        app.status_message.as_deref(),
        app.confirm_trash,
    );
    frame.render_widget(status, status_area);
}

pub fn run(
    mut app: App,
    scan_rx: Option<Receiver<crate::scanner::ScanResult>>,
    home: std::path::PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();

    loop {
        // Check for completed scan
        if let Some(ref rx) = scan_rx {
            if let Ok(result) = rx.try_recv() {
                app.apply_scan_result(result, &home);
            }
        }

        // Advance spinner tick
        if let AppState::Scanning { ref mut tick } = app.state {
            *tick = tick.wrapping_add(1);
        }

        terminal.draw(|frame| render(frame, &mut app))?;

        if crossterm::event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Ignore most input while scanning
                if matches!(app.state, AppState::Scanning { .. }) {
                    if key.code == KeyCode::Char('q') {
                        break;
                    }
                    continue;
                }

                if app.confirm_trash {
                    match key.code {
                        KeyCode::Char('y') => app.confirm_trash_action(),
                        _ => app.cancel_trash(),
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Up => app.previous(),
                        KeyCode::Down => app.next(),
                        KeyCode::Right | KeyCode::Enter => app.enter(),
                        KeyCode::Left | KeyCode::Backspace => app.go_back(),
                        KeyCode::Char('r') | KeyCode::Esc => app.jump_to_root(),
                        KeyCode::Char('d') => app.initiate_trash(),
                        _ => {}
                    }
                }
            }
        }

        if app.should_quit {
            break;
        }
    }

    ratatui::restore();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_file(name: &str, path: &str) -> DirEntry {
        DirEntry {
            name: name.to_string(),
            path: PathBuf::from(path),
            size: 100,
            is_dir: false,
            flag: None,
            children: vec![],
        }
    }

    fn make_dir(name: &str, path: &str, children: Vec<DirEntry>) -> DirEntry {
        DirEntry {
            name: name.to_string(),
            path: PathBuf::from(path),
            size: children.iter().map(|c| c.size).sum(),
            is_dir: true,
            flag: None,
            children,
        }
    }

    fn make_test_tree() -> DirEntry {
        make_dir(
            "root",
            "/root",
            vec![
                make_dir(
                    "sub",
                    "/root/sub",
                    vec![make_file("leaf", "/root/sub/leaf")],
                ),
                make_file("file", "/root/file"),
            ],
        )
    }

    #[test]
    fn test_initial_selection() {
        let app = App::new(make_test_tree(), 0);
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_enter_pushes_nav_stack() {
        let mut app = App::new(make_test_tree(), 0);
        app.list_state.select(Some(0)); // select "sub"
        app.enter();
        assert_eq!(app.nav_stack, vec![0]);
        assert_eq!(app.current_children().len(), 1); // "leaf"
    }

    #[test]
    fn test_enter_on_file_does_not_push() {
        let mut app = App::new(make_test_tree(), 0);
        app.list_state.select(Some(1)); // select "file"
        app.enter();
        assert!(app.nav_stack.is_empty());
    }

    #[test]
    fn test_go_back_pops_nav_stack() {
        let mut app = App::new(make_test_tree(), 0);
        app.list_state.select(Some(0));
        app.enter();
        app.go_back();
        assert!(app.nav_stack.is_empty());
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_go_back_at_root_is_noop() {
        let mut app = App::new(make_test_tree(), 0);
        app.go_back();
        assert!(app.nav_stack.is_empty());
    }

    #[test]
    fn test_jump_to_root_clears_stack() {
        let mut app = App::new(make_test_tree(), 0);
        app.list_state.select(Some(0));
        app.enter();
        app.jump_to_root();
        assert!(app.nav_stack.is_empty());
        assert_eq!(app.list_state.selected(), Some(0));
    }

    #[test]
    fn test_next_wraps_at_end() {
        let mut app = App::new(make_test_tree(), 0);
        app.list_state.select(Some(0));
        app.next();
        assert_eq!(app.list_state.selected(), Some(1));
        app.next(); // at end — should stay
        assert_eq!(app.list_state.selected(), Some(1));
    }

    #[test]
    fn test_previous_stops_at_zero() {
        let mut app = App::new(make_test_tree(), 0);
        app.list_state.select(Some(0));
        app.previous();
        assert_eq!(app.list_state.selected(), Some(0));
    }
}
