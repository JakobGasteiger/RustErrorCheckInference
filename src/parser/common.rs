
// * components shared by the parsers

#[derive(Clone, Copy, Debug)]
pub enum ParseError {
    WholeInput,
    Function,
    Interval,
    NoESSSFile
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            _ => write!(f, "{:?}", self),
        }
    }
}

impl std::error::Error for ParseError {}