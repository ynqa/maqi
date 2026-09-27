//! Command information extracted from rendered manuals.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flag {
    pub names: Vec<String>,
    pub value: Option<String>,
    pub repeated: bool,
    pub choices: Vec<String>,
    pub help: String,
}

#[derive(Debug)]
pub struct Manual {
    pub command: Vec<String>,
    pub help: String,
    pub flags: Vec<Flag>,
    pub inherited: Vec<Flag>,
    pub source: String,
}
