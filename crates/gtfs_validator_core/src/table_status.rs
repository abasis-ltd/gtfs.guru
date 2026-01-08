#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableStatus {
    Ok,
    MissingFile,
    ParseError,
}

impl TableStatus {
    /// Returns true if the table was parsed without errors.
    /// Missing files are considered successfully parsed (nothing to parse).
    /// Only ParseError returns false.
    pub fn is_parsed_successfully(self) -> bool {
        !matches!(self, TableStatus::ParseError)
    }
}
