#[derive(Debug, Clone)]
pub enum Status {
    Running,
    Pending,
}
#[derive(Debug, Clone)]
pub struct Task {
    pub id: usize,
    pub name: String,
    pub status: Status,
    pub file_name: String,
    pub test: bool,
}
