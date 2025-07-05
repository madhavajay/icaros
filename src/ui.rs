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
    pub animations_enabled: bool,
    pub llama_x: f32,
    pub day_night_cycle: f32,
    pub wave_offset: f32,
}


const NATIVE_PATTERNS: &[&str] = &[
    "◇", "◈", "◊", "⟡", "✦", "✧", "◉", "◎", 
    "▲", "▼", "◆", "♦", "⬟", "⬢", "⬣", "⬡"
];

const DESERT_ELEMENTS: &[&str] = &[
    "🌵", "🏜️", "⛰️", "🪨", "🌄", "🌅"
];

const TIME_ELEMENTS: &[(&str, &str)] = &[
    ("☀️", "Dawn"),
    ("🌞", "Day"),
    ("🌅", "Dusk"),
    ("🌙", "Night"),
    ("⭐", "Midnight"),
];

const EARTH_COLORS: &[Color] = &[
    Color::Rgb(184, 134, 11),   // Dark goldenrod
    Color::Rgb(210, 105, 30),   // Chocolate
    Color::Rgb(205, 133, 63),   // Peru
    Color::Rgb(222, 184, 135),  // Burlywood
    Color::Rgb(160, 82, 45),    // Sienna
    Color::Rgb(139, 69, 19),    // Saddle brown
    Color::Rgb(255, 140, 0),    // Dark orange
    Color::Rgb(218, 165, 32),   // Goldenrod
];

const SUNSET_GRADIENT: &[Color] = &[
    Color::Rgb(255, 94, 77),    // Sunset red
    Color::Rgb(255, 140, 0),    // Dark orange
    Color::Rgb(255, 206, 84),   // Sunset yellow
    Color::Rgb(237, 117, 56),   // Sunset orange
    Color::Rgb(95, 39, 205),    // Sunset purple
    Color::Rgb(52, 31, 151),    // Deep purple
    Color::Rgb(0, 0, 70),       // Night blue
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
            animations_enabled: true,
            llama_x: 0.0,
            day_night_cycle: 0.0,
            wave_offset: 0.0,
        };
        app.update_items();
        app.list_state.select(Some(0));
        app
    }

    fn update_animations(&mut self, width: u16) {
        // Update llama position (slow wandering)
        self.llama_x += 0.3;
        if self.llama_x > width as f32 + 10.0 {
            self.llama_x = -5.0;
        }
        
        // Update day/night cycle
        self.day_night_cycle += 0.005;
        if self.day_night_cycle > 1.0 {
            self.day_night_cycle = 0.0;
        }
        
        // Update wave offset for gradient effects
        self.wave_offset += 0.1;
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

fn get_gradient_color(position: f32, offset: f32, colors: &[Color]) -> Color {
    let wave = ((position + offset).sin() + 1.0) / 2.0;
    let index = (wave * (colors.len() - 1) as f32) as usize;
    let next_index = (index + 1).min(colors.len() - 1);
    let t = wave * (colors.len() - 1) as f32 - index as f32;
    
    interpolate_color(colors[index], colors[next_index], t)
}

fn interpolate_color(c1: Color, c2: Color, t: f32) -> Color {
    match (c1, c2) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            Color::Rgb(
                (r1 as f32 + (r2 as f32 - r1 as f32) * t) as u8,
                (g1 as f32 + (g2 as f32 - g1 as f32) * t) as u8,
                (b1 as f32 + (b2 as f32 - b1 as f32) * t) as u8,
            )
        },
        _ => c1,
    }
}

fn get_sky_color(day_night: f32) -> Color {
    let index = (day_night * (SUNSET_GRADIENT.len() - 1) as f32) as usize;
    let next_index = (index + 1).min(SUNSET_GRADIENT.len() - 1);
    let t = day_night * (SUNSET_GRADIENT.len() - 1) as f32 - index as f32;
    
    interpolate_color(SUNSET_GRADIENT[index], SUNSET_GRADIENT[next_index], t)
}

fn get_time_emoji(day_night: f32) -> (&'static str, &'static str) {
    let index = (day_night * TIME_ELEMENTS.len() as f32) as usize;
    let index = index.min(TIME_ELEMENTS.len() - 1);
    TIME_ELEMENTS[index]
}

fn get_native_pattern(frame: u64, offset: usize) -> &'static str {
    NATIVE_PATTERNS[(frame / 15 + offset as u64) as usize % NATIVE_PATTERNS.len()]
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
            if app.animations_enabled {
                app.frame_count += 1;
                app.update_animations(f.size().width);
            }
            
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Min(0),
                    Constraint::Length(3),
                ].as_ref())
                .split(f.size());

            // Create animated title with gradient background
            let mut title_spans = Vec::new();
            let width = chunks[0].width as usize;
            
            if app.animations_enabled {
                // Create gradient background
                let sky_color = get_sky_color(app.day_night_cycle);
                let (time_emoji, time_name) = get_time_emoji(app.day_night_cycle);
                
                // Build the title line with gradient effect
                for i in 0..width {
                    let x = i as f32 / width as f32;
                    let gradient_color = get_gradient_color(x * 10.0, app.wave_offset, EARTH_COLORS);
                    
                    // Place llama
                    if i as f32 >= app.llama_x && (i as f32) < app.llama_x + 2.0 {
                        title_spans.push(Span::styled("🦙", Style::default().fg(Color::White).bg(sky_color)));
                    }
                    // Place desert elements
                    else if i % 15 == 0 && i > 0 && i < width - 5 {
                        let desert_elem = DESERT_ELEMENTS[(i / 15) % DESERT_ELEMENTS.len()];
                        title_spans.push(Span::styled(desert_elem, Style::default().fg(gradient_color).bg(sky_color)));
                    }
                    // Place time emoji
                    else if i == width - 10 {
                        title_spans.push(Span::styled(time_emoji, Style::default().fg(Color::White).bg(sky_color)));
                    }
                    // Native patterns
                    else if i % 8 == 0 {
                        let pattern = get_native_pattern(app.frame_count, i);
                        title_spans.push(Span::styled(pattern, Style::default().fg(gradient_color).bg(sky_color)));
                    }
                    else {
                        title_spans.push(Span::styled(" ", Style::default().bg(sky_color)));
                    }
                }
                
                // Title text overlay
                let title_text = "◈ I C A R O S ◈";
                let title_start = (width / 2).saturating_sub(title_text.len() / 2);
                for (i, ch) in title_text.chars().enumerate() {
                    if title_start + i < title_spans.len() {
                        let pulse = ((app.frame_count as f32 * 0.05 + i as f32 * 0.3).sin() + 1.0) / 2.0;
                        let text_color = interpolate_color(
                            Color::Rgb(255, 255, 255),
                            Color::Rgb(255, 215, 0),
                            pulse
                        );
                        title_spans[title_start + i] = Span::styled(
                            ch.to_string(), 
                            Style::default().fg(text_color).bg(sky_color).add_modifier(Modifier::BOLD)
                        );
                    }
                }
                
                // Add subtitle
                let subtitle = format!(" {} Journey ", time_name);
                let _subtitle_start = (width / 2).saturating_sub(subtitle.len() / 2);
                let _subtitle_y = 2; // This would need to be on a second line
            } else {
                // Static title
                let static_line = format!("{:^width$}", "◈ I C A R O S ◈ - Stop AI Agents from Going on Vision Quests", width = width);
                title_spans.push(Span::styled(static_line, Style::default().fg(Color::Rgb(255, 215, 0))));
            }
            
            let title = vec![Line::from(title_spans)];
            let title_widget = Paragraph::new(title)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(if app.animations_enabled {
                        get_gradient_color(0.0, app.wave_offset * 2.0, EARTH_COLORS)
                    } else {
                        Color::Rgb(210, 105, 30)  // Static chocolate
                    }))
                    .style(Style::default()))
                .alignment(ratatui::layout::Alignment::Center);
            f.render_widget(title_widget, chunks[0]);

            // No more floating emojis in the main area

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
                            .fg(Color::Rgb(0, 206, 209))  // Static cyan for directories
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
                    .border_style(Style::default().fg(Color::Rgb(138, 43, 226)))  // Static violet
                    .title(" 🦙 File Guardian 🦙 ")  // Static title
                    .style(Style::default().bg(Color::Rgb(0, 0, 0))))
                .highlight_style(Style::default()
                    .bg(Color::Rgb(138, 43, 226))
                    .add_modifier(Modifier::BOLD));

            f.render_stateful_widget(list, chunks[1], &mut app.list_state);

            // Render bottom help bar
            let left_pattern = if app.animations_enabled {
                get_native_pattern(app.frame_count, 0)
            } else {
                "◇"
            };
            let right_pattern = if app.animations_enabled {
                get_native_pattern(app.frame_count, 1)
            } else {
                "◈"
            };
            let pattern_color = if app.animations_enabled {
                get_gradient_color(0.5, app.wave_offset, EARTH_COLORS)
            } else {
                Color::Rgb(160, 82, 45)  // Static sienna
            };
            let help_text = vec![
                Line::from(vec![
                    Span::styled(format!(" {} ", left_pattern), Style::default().fg(pattern_color)),
                    Span::styled("↑↓", Style::default().fg(Color::Rgb(255, 105, 180)).add_modifier(Modifier::BOLD)),
                    Span::raw(":navigate "),
                    Span::styled("Space", Style::default().fg(Color::Rgb(0, 206, 209)).add_modifier(Modifier::BOLD)),
                    Span::raw(":lock "),
                    Span::styled("c", Style::default().fg(Color::Rgb(255, 215, 0)).add_modifier(Modifier::BOLD)),
                    Span::raw(":allow-create "),
                    Span::styled("Enter", Style::default().fg(Color::Rgb(138, 43, 226)).add_modifier(Modifier::BOLD)),
                    Span::raw(":expand "),
                    Span::styled("a", Style::default().fg(Color::Rgb(64, 224, 208)).add_modifier(Modifier::BOLD)),
                    Span::raw(":toggle-anim "),
                    Span::styled("q", Style::default().fg(Color::Rgb(255, 127, 80)).add_modifier(Modifier::BOLD)),
                    Span::raw(":quit"),
                    Span::styled(format!(" {} ", right_pattern), Style::default().fg(pattern_color)),
                ]),
            ];
            let help_widget = Paragraph::new(help_text)
                .block(Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(if app.animations_enabled {
                        get_gradient_color(1.0, app.wave_offset * 2.0, EARTH_COLORS)
                    } else {
                        Color::Rgb(139, 69, 19)  // Static saddle brown
                    }))
                    .style(Style::default()))
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
                KeyCode::Char('a') => app.animations_enabled = !app.animations_enabled,
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