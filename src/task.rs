#[derive(Debug, Clone)]
pub enum Status {
    Running,
    Pending,
}
#[derive(Debug, Clone)]
pub struct Task {
    pub id: usize,
    pub name: String,
    pub depend: Vec<usize>,
    pub status: Status,
    pub file_name: String,
    pub in_degree: usize,
    pub next: Vec<usize>,
    pub test: bool,
}
