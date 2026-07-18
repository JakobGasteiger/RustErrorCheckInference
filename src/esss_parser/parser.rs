

use crate::utils::error_spec::ErrorSpec;
use std::{collections::HashSet, io::Write};


pub struct FunctionErrorSpec {
    func_name: String,
    error_spec: ErrorSpec, 
}

impl FunctionErrorSpec {
    fn new(func_name: String, error_spec: ErrorSpec) -> Self {
        Self {
            func_name,
            error_spec
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ParseError {
    WholeInput,
    Function,
    Interval
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            _ => write!(f, "{:?}", self),
        }
    }
}

impl std::error::Error for ParseError {}


fn get_function_spec_strings() -> Result<Vec<String>, ParseError> {

    println!("\n");

    // TODO temporary path
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/esss_results.txt");
    println!("{}", path);

    let raw_string = std::fs::read_to_string(&path).or(Err(ParseError::WholeInput))?;
    //println!("{}", raw_string);

    // split into the two-line strings which denote the spec of one function
    let spec_strings = 
        raw_string
        .lines()
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|chunk| chunk.join("\n"))
        .collect::<Vec<_>>();

    // verify they're all 2 lines long
    for str in &spec_strings {
        if str.lines().collect::<Vec<&str>>().len() != 2 {
            return Err(ParseError::WholeInput);
        }
    }

    for str in &spec_strings {
        println!("\n{}", str);
    }
    return Ok(spec_strings);

}


fn parse_interval_to_range(interval_string: String) -> Result<Vec<i128>, ParseError> {

    println!("Parsing Interval: {}", interval_string);

    let mut inner = interval_string;
    if inner.starts_with("[") {
        inner = inner.strip_prefix("[").unwrap_or(&inner.clone()).to_string();
    }
    if inner.ends_with("]") {
        inner = inner.strip_suffix("]").unwrap_or(&inner.clone()).to_string();
    }

    let mut parts = inner.split(',').map(str::trim);

    let lo = parts
        .next()
        .ok_or(ParseError::Interval)?
        .parse::<i128>()
        .map_err(|_| ParseError::Interval)?;

    let hi = parts
        .next()
        .ok_or(ParseError::Interval)?
        .parse::<i128>()
        .map_err(|_| ParseError::Interval)?;

    println!("Lo: {}, Hi: {}", lo, hi);

    if lo > hi {
        return Err(ParseError::Interval);
    }

    // if the interval crosses zero (should never happen but its good to be prepared), 
    // zero must be explicitly included in the range to make sure the final predicate isnt wrong
    if lo < 0 && hi > 0 {
        return Ok(vec![lo,0,hi]);
    }

    Ok(vec![lo,hi])

}

fn parse_spec_string(spec_string: String) -> Result<FunctionErrorSpec, ParseError> {

    let split_spec_string = spec_string.lines().map(|s| s.to_string()).collect::<Vec<_>>();

    let header = split_spec_string.get(0).ok_or(ParseError::Function)?.clone();
    let spec_line = split_spec_string.get(1).ok_or(ParseError::Function).clone()?.trim().to_string();

    // ESSS output is formatted as Function: function_name {return index 0}, so we need to get the seconds element when splitting at spaces
    let function_name = header.split(" ").map(|s| s.to_string()).collect::<Vec<_>>().get(1).ok_or(ParseError::Function)?.clone();
    println!("\nFunction name is: {}", function_name);

    let mut error_values: Vec<i128> = Vec::new();

    println!("Parsing spec line: {}", spec_line);

    if spec_line == "EMPTY" {
        println!("ErrorSpec is Empty");
        return Ok(FunctionErrorSpec { func_name: function_name, error_spec: ErrorSpec::Empty }); 
    }

    // if the spec line is not just EMPTY, we split it into its intervals
    let intervals = 
        spec_line
        .split("] U [")
        //.filter(|s| s.starts_with("[") && s.ends_with("]"))
        .map(|s| s.to_string())
        .collect::<Vec<String>>();

    if intervals.is_empty() {
        println!("No Intervals found!");
        return Ok(FunctionErrorSpec { func_name: function_name, error_spec: ErrorSpec::Indeterminate })
    }

    for interval in intervals {

        if let Ok(interval_values) = parse_interval_to_range(interval) {
            error_values.append(interval_values.clone().as_mut());
        } else {
            return Ok(FunctionErrorSpec { func_name: function_name, error_spec: ErrorSpec::Indeterminate });
        }
    }

    let error_spec = ErrorSpec::from_number_set(HashSet::from_iter(error_values));
    println!("ErrorSpec is {:?}", error_spec);
    Ok(FunctionErrorSpec { func_name: function_name, error_spec })
}


fn parse_spec_strings(spec_strings: Vec<String>) -> Vec<Result<FunctionErrorSpec, ParseError>> {

    let mut specs: Vec<Result<FunctionErrorSpec, ParseError>> = Vec::new();

    for spec_string in spec_strings {        

        let spec = parse_spec_string(spec_string);
        specs.push(spec);
    }

    specs
}

pub fn parse_specs() -> Vec<Result<FunctionErrorSpec, ParseError>> {

    eprintln!("Spec parser active!");

    if let Ok(spec_strings) = get_function_spec_strings() {

        let specs = parse_spec_strings(spec_strings);
    
        return specs
    } else {
        return vec![Err(ParseError::WholeInput)];
    }

}