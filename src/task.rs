use tokio::time::Instant;

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
    pub cmd: String,
    pub test: bool,
    pub timestamp: Instant,
}
