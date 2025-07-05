use crate::file_tree::TreeNode;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::io;
use std::time::{Duration, Instant};
use anyhow::Result;

pub struct App {
    pub tree: TreeNode,
    pub list_state: ListState,
    pub items: Vec<(TreeNode, usize)>,
    pub selected: usize,
    pub state_file: std::path::PathBuf,
    pub root_path: std::path::PathBuf,
    pub frame_count: u64,
    pub _last_update: Instant,
    pub floating_emojis: Vec<FloatingEmoji>,
}

#[derive(Clone)]
pub struct FloatingEmoji {
    emoji: &'static str,
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    color_index: usize,
}

const MYSTICAL_EMOJIS: &[&str] = &[
    "🦙", "🌵", "🪶", "🎶", "🌀", "💫", "🍄", "🔥",
    "🌙", "☀️", "🌿", "🧿", "✨", "👁️", "🧘", "🎨",
    "🧭", "🐍", "⛰️", "🗿", "🪨", "🏺", "🌄"
];

const PSYCHEDELIC_COLORS: &[Color] = &[
    Color::Rgb(155, 89, 182),  // Purple
    Color::Rgb(255, 107, 53),  // Orange
    Color::Rgb(0, 206, 209),    // Cyan
    Color::Rgb(255, 105, 180),  // Pink
    Color::Rgb(255, 0, 255),    // Magenta
    Color::Rgb(255, 215, 0),    // Gold
    Color::Rgb(64, 224, 208),   // Turquoise
    Color::Rgb(138, 43, 226),   // Violet
];

impl App {
    pub fn new(tree: TreeNode, state_file: std::path::PathBuf, root_path: std::path::PathBuf) -> Self {
        let mut app = Self {
            tree,
            list_state: ListState::default(),
            items: Vec::new(),
            selected: 0,
            state_file,
            root_path,
            frame_count: 0,
            _last_update: Instant::now(),
            floating_emojis: Self::init_floating_emojis(),
        };
        app.update_items();
        app.list_state.select(Some(0));
        app
    }

    fn init_floating_emojis() -> Vec<FloatingEmoji> {
        vec![
            FloatingEmoji { emoji: "🦙", x: 5.0, y: 2.0, dx: 0.5, dy: 0.3, color_index: 0 },
            FloatingEmoji { emoji: "🌵", x: 20.0, y: 5.0, dx: -0.3, dy: 0.2, color_index: 1 },
            FloatingEmoji { emoji: "🪶", x: 40.0, y: 3.0, dx: 0.4, dy: -0.3, color_index: 2 },
            FloatingEmoji { emoji: "🌀", x: 60.0, y: 8.0, dx: -0.6, dy: 0.4, color_index: 3 },
            FloatingEmoji { emoji: "✨", x: 30.0, y: 10.0, dx: 0.3, dy: -0.2, color_index: 4 },
        ]
    }

    fn update_floating_emojis(&mut self, width: u16, height: u16) {
        for emoji in &mut self.floating_emojis {
            emoji.x += emoji.dx;
            emoji.y += emoji.dy;

            if emoji.x <= 0.0 || emoji.x >= width as f32 - 2.0 {
                emoji.dx *= -1.0;
            }
            if emoji.y <= 0.0 || emoji.y >= height as f32 - 4.0 {
                emoji.dy *= -1.0;
            }

            emoji.x = emoji.x.clamp(0.0, width as f32 - 2.0);
            emoji.y = emoji.y.clamp(0.0, height as f32 - 4.0);
            
            emoji.color_index = (emoji.color_index + 1) % PSYCHEDELIC_COLORS.len();
        }

        // Occasionally add new emoji
        if self.frame_count % 60 == 0 && self.floating_emojis.len() < 10 {
            let new_emoji = FloatingEmoji {
                emoji: MYSTICAL_EMOJIS[self.frame_count as usize % MYSTICAL_EMOJIS.len()],
                x: (self.frame_count % width as u64) as f32,
                y: 2.0,
                dx: 0.3,
                dy: 0.2,
                color_index: self.frame_count as usize % PSYCHEDELIC_COLORS.len(),
            };
            self.floating_emojis.push(new_emoji);
        }

        // Remove old emojis if too many
        if self.floating_emojis.len() > 12 {
            self.floating_emojis.remove(0);
        }
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

fn get_psychedelic_color(frame: u64, index: usize) -> Color {
    let color_index = ((frame / 10) as usize + index) % PSYCHEDELIC_COLORS.len();
    PSYCHEDELIC_COLORS[color_index]
}

fn get_shamanic_symbol(frame: u64) -> &'static str {
    const SYMBOLS: &[&str] = &["◈", "◊", "✦", "✧", "⟡", "◉", "◎", "◇"];
    SYMBOLS[(frame / 20) as usize % SYMBOLS.len()]
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(50);
    
    loop {
        terminal.draw(|f| {
            // Update animations
            app.frame_count += 1;
            app.update_floating_emojis(f.size().width, f.size().height);
            
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(3),
                ].as_ref())
                .split(f.size());

            // Render psychedelic title
            let title_style = Style::default()
                .fg(get_psychedelic_color(app.frame_count, 0))
                .add_modifier(Modifier::BOLD);
            let title = vec![
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(get_shamanic_symbol(app.frame_count), title_style),
                    Span::raw(" "),
                    Span::styled("🦙 I C A R O S 🦙", title_style),
                    Span::raw(" "),
                    Span::styled(get_shamanic_symbol(app.frame_count + 1), title_style),
                    Span::raw(" - "),
                    Span::styled("Stop AI Agents from Going on Vision Quests", 
                        Style::default().fg(get_psychedelic_color(app.frame_count, 3))),
                    Span::raw(" "),
                    Span::styled(get_shamanic_symbol(app.frame_count + 2), title_style),
                ]),
            ];
            let title_widget = Paragraph::new(title)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(get_psychedelic_color(app.frame_count, 1)))
                    .style(Style::default().bg(Color::Rgb(91, 44, 111))))  // Dark purple bg
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(title_widget, chunks[0]);

            // Render floating emojis in the background
            for emoji in &app.floating_emojis {
                let x = emoji.x as u16;
                let y = emoji.y as u16 + chunks[1].y;
                if x < chunks[1].width && y < chunks[1].y + chunks[1].height {
                    let emoji_style = Style::default()
                        .fg(PSYCHEDELIC_COLORS[emoji.color_index])
                        .add_modifier(Modifier::BOLD);
                    let emoji_span = Span::styled(emoji.emoji, emoji_style);
                    let emoji_widget = Paragraph::new(Line::from(vec![emoji_span]));
                    let emoji_area = Rect {
                        x: chunks[1].x + x,
                        y,
                        width: 2,
                        height: 1,
                    };
                    f.render_widget(emoji_widget, emoji_area);
                }
            }

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
                        spans.push(Span::styled("🔒 ", 
                            Style::default().fg(Color::Rgb(255, 107, 53))));
                        if node.is_dir && node.allow_create_in_locked {
                            spans.push(Span::styled("➕ ", 
                                Style::default().fg(Color::Rgb(0, 206, 209))));
                        } else {
                            spans.push(Span::raw("   "));
                        }
                    } else {
                        spans.push(Span::raw("   "));
                        spans.push(Span::raw("   "));
                    }
                    
                    let style = if node.is_locked {
                        Style::default()
                            .fg(Color::Rgb(255, 127, 80))  // Coral
                            .add_modifier(Modifier::BOLD)
                    } else if node.is_dir {
                        Style::default()
                            .fg(get_psychedelic_color(app.frame_count, *indent))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                            .fg(Color::Rgb(255, 215, 0))  // Gold
                    };
                    
                    spans.push(Span::styled(&node.name, style));
                    
                    ListItem::new(Line::from(spans))
                })
                .collect();

            let list = List::new(items)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(get_psychedelic_color(app.frame_count, 5)))
                    .title(format!(" {} File Guardian {} ", 
                        MYSTICAL_EMOJIS[(app.frame_count / 30) as usize % MYSTICAL_EMOJIS.len()],
                        MYSTICAL_EMOJIS[(app.frame_count / 30 + 1) as usize % MYSTICAL_EMOJIS.len()]
                    ))
                    .style(Style::default().bg(Color::Rgb(0, 0, 0))))
                .highlight_style(Style::default()
                    .bg(Color::Rgb(138, 43, 226))
                    .add_modifier(Modifier::BOLD));

            f.render_stateful_widget(list, chunks[1], &mut app.list_state);

            // Render bottom help bar
            let help_text = vec![
                Line::from(vec![
                    Span::styled(" 🌀 ", Style::default().fg(get_psychedelic_color(app.frame_count, 6))),
                    Span::styled("↑↓", Style::default().fg(Color::Rgb(255, 105, 180)).add_modifier(Modifier::BOLD)),
                    Span::raw(":navigate "),
                    Span::styled("Space", Style::default().fg(Color::Rgb(0, 206, 209)).add_modifier(Modifier::BOLD)),
                    Span::raw(":lock "),
                    Span::styled("c", Style::default().fg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD)),
                    Span::raw(":allow-create "),
                    Span::styled("Enter", Style::default().fg(Color::Rgb(138, 43, 226)).add_modifier(Modifier::BOLD)),
                    Span::raw(":expand "),
                    Span::styled("q", Style::default().fg(Color::Rgb(255, 127, 80)).add_modifier(Modifier::BOLD)),
                    Span::raw(":quit"),
                    Span::styled(" 🌀", Style::default().fg(get_psychedelic_color(app.frame_count, 7))),
                ]),
            ];
            let help_widget = Paragraph::new(help_text)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(get_psychedelic_color(app.frame_count, 2)))
                    .style(Style::default().bg(Color::Rgb(91, 44, 111))))
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(help_widget, chunks[2]);
        })?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
            
        if crossterm::event::poll(timeout)? {
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
        
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
    Ok(())
}