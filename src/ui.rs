use crate::file_tree::TreeNode;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Terminal,
};
use std::io;
use anyhow::Result;

pub struct App {
    pub tree: TreeNode,
    pub list_state: ListState,
    pub items: Vec<(TreeNode, usize)>,
    pub selected: usize,
    pub state_file: std::path::PathBuf,
    pub root_path: std::path::PathBuf,
}

impl App {
    pub fn new(tree: TreeNode, state_file: std::path::PathBuf, root_path: std::path::PathBuf) -> Self {
        let mut app = Self {
            tree,
            list_state: ListState::default(),
            items: Vec::new(),
            selected: 0,
            state_file,
            root_path,
        };
        app.update_items();
        app.list_state.select(Some(0));
        app
    }

    pub fn update_items(&mut self) {
        self.items.clear();
        let tree_clone = self.tree.clone();
        self.collect_visible_nodes(&tree_clone, 0);
    }

    fn collect_visible_nodes(&mut self, node: &TreeNode, indent: usize) {
        self.items.push((node.clone(), indent));
        
        if node.is_dir && node.is_expanded {
            for child in &node.children {
                self.collect_visible_nodes(child, indent + 1);
            }
        }
    }

    pub fn toggle_selected(&mut self) {
        if self.selected < self.items.len() {
            let path = self.items[self.selected].0.path.clone();
            toggle_node_at_path(&mut self.tree, &path);
            self.update_items();
            self.save_state();
        }
    }

    pub fn toggle_expand_selected(&mut self) {
        if self.selected < self.items.len() {
            let path = self.items[self.selected].0.path.clone();
            toggle_expand_at_path(&mut self.tree, &path);
            self.update_items();
            self.save_state();
        }
    }
    
    pub fn toggle_create_in_locked_selected(&mut self) {
        if self.selected < self.items.len() {
            let path = self.items[self.selected].0.path.clone();
            toggle_create_in_locked_at_path(&mut self.tree, &path);
            self.update_items();
            self.save_state();
        }
    }

    fn save_state(&self) {
        let mut state = crate::state::AppState::new(self.root_path.clone());
        state.update_expanded_dirs(self.get_expanded_dirs());
        state.update_from_tree(&self.tree);
        if let Err(e) = state.save_to_file(&self.state_file) {
            eprintln!("Error saving state: {}", e);
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn move_down(&mut self) {
        if self.selected < self.items.len() - 1 {
            self.selected += 1;
            self.list_state.select(Some(self.selected));
        }
    }

    pub fn get_locked_files(&self) -> Vec<std::path::PathBuf> {
        self.tree.get_locked_files()
    }

    pub fn get_expanded_dirs(&self) -> Vec<std::path::PathBuf> {
        let mut expanded = Vec::new();
        self.collect_expanded_dirs(&self.tree, &mut expanded);
        expanded
    }

    fn collect_expanded_dirs(&self, node: &TreeNode, expanded: &mut Vec<std::path::PathBuf>) {
        if node.is_dir && node.is_expanded {
            expanded.push(node.path.clone());
        }
        for child in &node.children {
            self.collect_expanded_dirs(child, expanded);
        }
    }
    
}

fn toggle_node_at_path(node: &mut TreeNode, target_path: &std::path::Path) -> bool {
    if node.path == target_path {
        node.toggle_lock();
        return true;
    }
    
    for child in &mut node.children {
        if toggle_node_at_path(child, target_path) {
            return true;
        }
    }
    false
}

fn toggle_expand_at_path(node: &mut TreeNode, target_path: &std::path::Path) -> bool {
    if node.path == target_path {
        node.toggle_expand();
        return true;
    }
    
    for child in &mut node.children {
        if toggle_expand_at_path(child, target_path) {
            return true;
        }
    }
    false
}

fn toggle_create_in_locked_at_path(node: &mut TreeNode, target_path: &std::path::Path) -> bool {
    if node.path == target_path {
        node.toggle_create_in_locked();
        return true;
    }
    
    for child in &mut node.children {
        if toggle_create_in_locked_at_path(child, target_path) {
            return true;
        }
    }
    false
}

pub fn run_ui(mut app: App) -> Result<App> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result?;
    Ok(app)
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                ].as_ref())
                .split(f.size());

            let items: Vec<ListItem> = app.items
                .iter()
                .map(|(node, indent)| {
                    let mut spans = vec![
                        Span::raw("  ".repeat(*indent)),
                    ];
                    
                    if node.is_dir {
                        spans.push(Span::raw(if node.is_expanded { "▼ " } else { "▶ " }));
                    } else {
                        spans.push(Span::raw("  "));
                    }
                    
                    if node.is_locked {
                        spans.push(Span::raw("[L] "));
                        if node.is_dir && node.allow_create_in_locked {
                            spans.push(Span::raw("[+] "));
                        } else {
                            spans.push(Span::raw("    "));
                        }
                    } else {
                        spans.push(Span::raw("    "));
                        spans.push(Span::raw("    "));
                    }
                    
                    let style = if node.is_locked {
                        Style::default().fg(Color::Red)
                    } else if node.is_dir {
                        Style::default().fg(Color::Blue).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    
                    spans.push(Span::styled(&node.name, style));
                    
                    ListItem::new(Line::from(spans))
                })
                .collect();

            let list = List::new(items)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .title("Claude File Tree - ↑↓:navigate Space:lock c:allow-create(locked dirs) Enter:expand q:quit"))
                .highlight_style(Style::default().bg(Color::DarkGray));

            f.render_stateful_widget(list, chunks[0], &mut app.list_state);
        })?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Up => app.move_up(),
                KeyCode::Down => app.move_down(),
                KeyCode::Char(' ') => app.toggle_selected(),
                KeyCode::Enter => app.toggle_expand_selected(),
                KeyCode::Char('c') => app.toggle_create_in_locked_selected(),
                _ => {}
            }
        }
    }
    Ok(())
}