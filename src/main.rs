use crate::manager::TaskManager;
use crate::State::{HasPop, Left, Right};
use color_eyre::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use ratatui::layout::Direction::{Horizontal, Vertical};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::{style::Stylize, text::Line, DefaultTerminal, Frame};
use std::num::ParseIntError;
use tui_input::backend::crossterm::EventHandler;
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
struct SelectList {
    // select list
    pub item_list: Vec<Item>,
    pub idx: usize,
}
impl SelectList {
    pub fn select_up(&mut self) {
        if self.idx == 0 {
            self.idx = self.item_list.len() - 1;
        } else {
            self.idx = self.idx - 1;
        }
    }
    pub fn select_down(&mut self) {
        if self.idx == self.item_list.len() - 1 {
            self.idx = 0;
        } else {
            self.idx = self.idx + 1;
        }
    }
}
#[derive(Debug, Default)]
pub struct App {
    state: State,
    running: bool,

    // talps manager
    task_manager: TaskManager,
    select_list: SelectList,

    new_task_form: Form,
}

impl App {
    pub fn new() -> Self {
        let mut app = Self::default();
        app.select_list.item_list =
            vec![Item::AddTask, Item::RemoveTask, Item::TaskInfo, Item::Run];
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
            self.select_list
                .item_list
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
        list_state.select(Some(self.select_list.idx));
        frame.render_stateful_widget(menu, chunk, &mut list_state);
    }

    fn add_task_form(&self, area: Rect) {}

    fn right_content(&mut self, frame: &mut Frame, chunk: Rect) {
        // right content
        // AddItem : many input about new task
        match self.select_list.item_list[self.select_list.idx] {
            Item::TaskInfo => {}
            Item::AddTask => {
                self.new_task_form.render_form(frame, chunk);
            }
            Item::RemoveTask => {}
            Item::Run => {}
        }
    }

    fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(frame.area());
        self.left_menu(frame, chunks[0]);
        self.right_content(frame, chunks[1]);
        self.render_popup(frame);
    }

    fn popup_area(&self, area: Rect, percent_x: u16, percent_y: u16) -> Rect {
        let vertical = Layout::vertical([Constraint::Percentage(percent_y)]).flex(Flex::Center);
        let horizontal = Layout::horizontal([Constraint::Percentage(percent_x)]).flex(Flex::Center);
        let [area] = vertical.areas(area);
        let [area] = horizontal.areas(area);
        area
    }
    fn render_popup(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let area = self.popup_area(area, 40, 20);
        match &self.new_task_form.popup {
            Some(popup) => {
                let block = Paragraph::new(popup.content.clone()).block(
                    Block::default()
                        .title(popup.title.clone())
                        .borders(Borders::ALL),
                );
                frame.render_widget(Clear, area);
                frame.render_widget(block, area);
                self.state = HasPop;
            }
            None => {
                frame.render_widget(Clear, area);
            }
        }
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
        match self.state {
            Left => {
                match key.code {
                    KeyCode::Esc => self.quit(),
                    // Add other key handlers here.
                    KeyCode::Up => self.select_up(),
                    KeyCode::Down => self.select_down(),
                    KeyCode::Left => self.select_left(),
                    KeyCode::Right => self.select_right(),
                    _ => {}
                }
            }
            Right => self.right_key_event(key),
            HasPop => match key.code {
                _ => self.pop_down(),
            },
        }
    }
    fn pop_down(&mut self) {
        self.state = Right;
        self.new_task_form.popup = None;
    }
    fn right_key_event(&mut self, e: KeyEvent) {
        let event = Event::Key(e.clone());
        match self.select_list.item_list[self.select_list.idx] {
            Item::TaskInfo => {}
            Item::AddTask => match e.code {
                KeyCode::Esc => self.select_left(),
                KeyCode::Enter => self.finish_input(),
                KeyCode::Up => self.new_task_form.select_up(),
                KeyCode::Down => self.new_task_form.select_down(),
                _ => {
                    let _ = self.new_task_form.current_input().handle_event(&event);
                    ()
                }
            },
            Item::RemoveTask => {}
            Item::Run => {}
        }
    }
    fn finish_input(&mut self) {
        self.new_task_form.finish_input();
    }

    fn quit(&mut self) {
        self.running = false;
    }
    fn select_up(&mut self) {
        match &self.state {
            Left => {
                self.select_list.select_up();
            }
            Right => {}
            _ => {}
        }
    }
    fn select_down(&mut self) {
        match self.state {
            Left => {
                self.select_list.select_down();
            }
            Right => {}
            _ => {}
        }
    }
    fn select_left(&mut self) {
        self.state = State::Left;
    }
    fn select_right(&mut self) {
        self.state = State::Right;
    }
}
#[derive(Debug, Default)]
enum Item {
    Run,
    #[default]
    TaskInfo,
    AddTask,
    RemoveTask,
}
#[derive(Debug)]
struct Form {
    inputs: Vec<Input>,
    idx: usize,
    popup: Option<Popup>,
}
impl Default for Form {
    fn default() -> Self {
        Self::new()
    }
}

impl Form {
    fn new() -> Self {
        Self {
            inputs: vec![Input::default(), Input::default(), Input::default()],
            idx: 0,
            popup: None,
        }
    }
    fn current_input(&mut self) -> &mut Input {
        &mut self.inputs[self.idx]
    }
    fn finish_input(&mut self) {
        assert_eq!(self.inputs.len(), 3);
        let task_name = self.inputs[0].value().to_string();
        let deps_id = self.inputs[1].value().to_string();
        let exec_file = self.inputs[2].value().to_string();
        if task_name.trim().is_empty() {
            self.popup = Some(Popup::new(
                "Error".to_string(),
                "Task name cannot be empty".to_string(),
            ));
            return;
        }
        if exec_file.trim().is_empty() {
            self.popup = Some(Popup::new(
                "Error".to_string(),
                "Exec File cannot be empty".to_string(),
            ));
            return;
        }
        let deps_id_vec: Result<Vec<usize>, ParseIntError> = deps_id
            .trim()
            .split(",")
            .map(|id| id.trim().parse::<usize>())
            .collect();
        if deps_id_vec.is_err() {
            self.popup = Some(Popup::new(
                "Error".to_string(),
                "Deps id is not valid".to_string(),
            ));
            return;
        }

        for input in &mut self.inputs {
            input.reset();
        }
    }
    fn render_form(&mut self, frame: &mut Frame, area: Rect) {
        let chunk = Layout::default()
            .direction(Vertical)
            .constraints(vec![Constraint::Length(5); 3])
            .split(area);
        for (i, input) in self.inputs.iter_mut().enumerate() {
            let is_active = i == self.idx;
            let block = Block::default()
                .title(match i {
                    0 => "Task Name",
                    1 => "Deps Task Ids",
                    2 => "Exec File Path",
                    _ => "Task Name",
                })
                .borders(Borders::ALL)
                .border_style(if is_active {
                    Style::default().fg(Color::Yellow) // 激活状态黄色边框
                } else {
                    Style::default()
                });
            let para = Paragraph::new(Text::from(input.value())).block(block);
            frame.render_widget(para, chunk[i]);
            if is_active {
                frame.set_cursor_position((
                    chunk[i].x + 1 + input.visual_cursor() as u16,
                    chunk[i].y + 1,
                ));
            }
        }
    }
    fn select_up(&mut self) {
        if self.idx == 0 {
            self.idx = self.inputs.len() - 1;
        } else {
            self.idx = self.idx - 1;
        }
    }
    fn select_down(&mut self) {
        if self.idx == self.inputs.len() - 1 {
            self.idx = 0;
        } else {
            self.idx = self.idx + 1;
        }
    }
}

impl AsRef<str> for Item {
    fn as_ref(&self) -> &str {
        match self {
            Item::AddTask => "AddTask",
            Item::RemoveTask => "RemoveTask",
            Item::TaskInfo => "TaskInfo",
            Item::Run => "Run",
        }
    }
}

#[derive(PartialEq, Debug, Default)]
enum State {
    #[default]
    Left,
    Right,
    HasPop,
}
#[derive(Debug, Default)]
struct Popup {
    title: String,
    content: String,
}
impl Popup {
    fn new(title: String, content: String) -> Self {
        Popup { title, content }
    }

    fn render(&self, frame: &mut Frame) {
        let block = Block::default().borders(Borders::ALL);
        let para = Paragraph::new(Text::from(self.content.clone())).block(block);
        frame.render_widget(para, frame.area());
    }
}
