use functional::tree::Origin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OriginMapping {
    pub(crate) generated: Position,
    pub(crate) origin: Option<Origin>,
}
