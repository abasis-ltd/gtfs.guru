#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableStatus {
    Ok,
    MissingFile,
    ParseError,
}

impl TableStatus {
    pub fn is_parsed_successfully(self) -> bool {
        matches!(self, TableStatus::Ok)
    }
}
