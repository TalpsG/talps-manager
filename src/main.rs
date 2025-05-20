use crate::manager::TaskManager;
use color_eyre::owo_colors::OwoColorize;
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Direction::Horizontal;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Color;
use ratatui::text::{Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::{style::Stylize, text::Line, DefaultTerminal, Frame};
use tui_input::Input;

mod manager;
mod spmc_queue;
mod task;

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let terminal = ratatui::init();
    let result = App::new().run(terminal);
    ratatui::restore();
    result
}

#[derive(Debug, Default)]
pub struct App {
    state: State,
    running: bool,

    // talps manager
    task_manager: TaskManager,

    // select list
    item_list: Vec<Item>,
    idx: usize,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self::default();
        app.item_list = vec![Item::AddTask, Item::RemoveTask, Item::TaskInfo];
        app
    }

    pub fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        self.running = true;
        while self.running {
            terminal.draw(|frame| self.render(frame))?;
            self.handle_crossterm_events()?;
        }
        Ok(())
    }

    fn left_menu(&mut self, frame: &mut Frame, chunk: Rect) {
        let title = Line::from("Talps Manager").bold().centered();
        let menu = List::new(
            self.item_list
                .iter()
                .map(|item| {
                    let line = Line::from(vec![Span::from("   "), Span::from(item.as_ref())]);
                    ListItem::new(Text::from(line))
                })
                .collect::<Vec<_>>(),
        )
        .block(Block::new().title(title).bold().borders(Borders::ALL))
        .highlight_style(ratatui::style::Style::default().bg(Color::Black));

        let mut list_state = ListState::default();
        list_state.select(Some(self.idx));
        frame.render_stateful_widget(menu, chunk, &mut list_state);
    }

    fn get_content(&self) -> Paragraph {
        match self.item_list[self.idx] {
            Item::AddTask => todo!(),
            Item::RemoveTask => {
                todo!()
            }
            Item::TaskInfo => Paragraph::new("TaskInfo"),
        }
    }

    fn right_content(&mut self, frame: &mut Frame, chunk: Rect) {
        let mut content = Paragraph::new(self.item_list[self.idx].as_ref())
            .block(Block::default().borders(Borders::ALL).title("ToDO"));
        frame.render_widget(content, chunk);
    }

    fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(frame.area());
        self.left_menu(frame, chunks[0]);
        self.right_content(frame, chunks[1]);
        // frame.render_stateful_widget( )
    }

    fn handle_crossterm_events(&mut self) -> Result<()> {
        match event::read()? {
            // it's important to check KeyEventKind::Press to avoid handling key release events
            Event::Key(key) if key.kind == KeyEventKind::Press => self.on_key_event(key),
            Event::Mouse(_) => {}
            Event::Resize(_, _) => {}
            _ => {}
        }
        Ok(())
    }

    fn on_key_event(&mut self, key: KeyEvent) {
        match (key.modifiers, key.code) {
            (_, KeyCode::Esc | KeyCode::Char('q'))
            | (KeyModifiers::CONTROL, KeyCode::Char('c') | KeyCode::Char('C')) => self.quit(),
            // Add other key handlers here.
            (_, KeyCode::Up) => self.select_up(),
            (_, KeyCode::Down) => self.select_down(),
            _ => {}
        }
    }

    fn quit(&mut self) {
        self.running = false;
    }
    fn select_up(&mut self) {
        match self.state {
            State::Left => {
                self.idx = (self.idx - 1) % self.item_list.len();
            }
            State::Right => {}
        }
    }
    fn select_down(&mut self) {
        match self.state {
            State::Left => {
                self.idx = (self.idx + 1) % self.item_list.len();
            }
            State::Right => {}
        }
    }
}
#[derive(Debug, Default)]
enum Item {
    #[default]
    TaskInfo,
    AddTask,
    RemoveTask,
}
#[derive(Debug, Default)]
struct AddTaskForm {
    task_name: Input,
    depend_task: Input,
    exec_file: Input,
    idx: usize,
}
impl AsRef<str> for Item {
    fn as_ref(&self) -> &str {
        match self {
            Item::AddTask => "AddTask",
            Item::RemoveTask => "RemoveTask",
            Item::TaskInfo => "TaskInfo",
        }
    }
}

#[derive(Debug, Default)]
enum State {
    #[default]
    Left,
    Right,
}
