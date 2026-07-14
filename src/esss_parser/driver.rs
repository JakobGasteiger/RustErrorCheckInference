

use crate::utils::ret_val_check::ReturnValueCheck;
use std::io::Write;


pub struct FunctionErrorSpec {
    func_name: String,
    error_spec: ReturnValueCheck, 
}

#[derive(Clone, Copy, Debug)]
pub struct ParseError();

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            _ => write!(f, "Parsing Error"),
        }
    }
}

impl std::error::Error for ParseError {}

impl ParseError {
    fn new() -> Self {
        Self()
    }
}

fn get_function_spec_strings() -> Result<Vec<String>, ParseError> {

    let mut spec_strings = Vec::new();

    // TODO temporary path
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/esss_output.txt");
    let raw_string = std::fs::read_to_string(&path).or(Err(ParseError::new()))?;

    // split into the two-line strings which denote the spec of one function
    spec_strings = 
        raw_string
        .lines()
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|chunk| chunk.join("\n"))
        .collect::<Vec<_>>();

    // verify they're all 2 lines long
    for str in &spec_strings {
        if str.lines().collect::<Vec<&str>>().len() != 2 {
            return Err(ParseError::new());
        }
    }

    // eprintln!("\n");
    // for str in &spec_strings {
    //     eprintln!("{}", str);
    // }
    return Ok(spec_strings);
}

pub fn parse_specs() {

    eprintln!("Spec parser active!");

    let spec_strings = get_function_spec_strings();
    if spec_strings.is_err() {
        //println!("Parsing Error");
        return;
    }
}