

use crate::utils::error_spec::{ErrorSpec, FunctionErrorSpec};
use std::{collections::HashSet, fmt::Debug, io::Write};

use crate::parser::common::ParseError;


fn get_function_spec_strings() -> Result<Vec<String>, ParseError> {

    println!("\n");

    let path = std::env::current_dir().or(Err(ParseError::NoESSSFile))?.into_string().or(Err(ParseError::NoESSSFile))? + "/esss_results.txt"; 
    println!("ESSS Results File should be at {}", path);

    let raw_string = match std::fs::read_to_string(&path).or(Err(ParseError::WholeInput)) {
        Ok(raw_string) => raw_string,
        Err(err) => {   
            println!("-> not found!");
            return Err(err);
        }
    };
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

    // for str in &spec_strings {
    //     println!("\n{}", str);
    // }
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

fn parse_spec_string(spec_string: String) -> FunctionErrorSpec {

    let split_spec_string = spec_string.lines().map(|s| s.to_string()).collect::<Vec<_>>();

    // ESSS output is formatted as Function: function_name {return index 0}, so we need to get the seconds element when splitting at spaces
    let function_name: String = match split_spec_string.get(0).ok_or(ParseError::Function) {
        Ok(header) => header
                                    .split(" ")
                                    .map(|s| s.to_string())
                                    .collect::<Vec<_>>()
                                    .get(1)
                                    .unwrap_or(&"ErrorParsingFunctionName".to_string())
                                    .clone(),
        Err(_) => "ErrorParsingFunctionName".to_string()
    };
    println!("\nFunction name is: {}", function_name);

    let spec_line = match split_spec_string.get(1).ok_or(ParseError::Function) {
        Ok(spec_line) => Ok(spec_line.trim().to_string()),
        Err(err) => Err(err)
    };

    let mut error_values: Vec<i128> = Vec::new();

    println!("Parsing spec line: {:?}", spec_line);

    if let Ok(spec_line) = spec_line {

        if spec_line == "EMPTY" {
            println!("ErrorSpec is Empty");
            return FunctionErrorSpec::new(function_name, ErrorSpec::Empty); 
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
            return FunctionErrorSpec::new(function_name, ErrorSpec::Indeterminate);
        }
    
        for interval in intervals {
    
            if let Ok(interval_values) = parse_interval_to_range(interval) {
                error_values.append(interval_values.clone().as_mut());
            } else {
                return FunctionErrorSpec::new(function_name, ErrorSpec::Indeterminate);
            }
        }
    
        let error_spec = ErrorSpec::from_number_set(HashSet::from_iter(error_values));
        println!("ErrorSpec is {:?}", error_spec);
        return FunctionErrorSpec::new(function_name, error_spec);
    }

    return FunctionErrorSpec::new(function_name, ErrorSpec::Indeterminate);
}


fn parse_spec_strings(spec_strings: Vec<String>) -> Vec<FunctionErrorSpec> {

    let mut specs: Vec<FunctionErrorSpec> = Vec::new();

    for spec_string in spec_strings {        

        let spec = parse_spec_string(spec_string);
        specs.push(spec);
    }

    specs
}

pub fn parse_esss() -> Option<Vec<FunctionErrorSpec>> {

    eprintln!("ESSS parser active!");

    if let Ok(spec_strings) = get_function_spec_strings() {
        let specs = parse_spec_strings(spec_strings);
        return Some(specs);
    } else {
        return None;
    }

}

pub fn print_esss_statistics(results: &Option<Vec<FunctionErrorSpec>>) {

    let mut total: usize = 0;
    let mut empty: usize = 0;
    let mut gr_eq_zero: usize = 0;
    let mut les_eq_zero: usize = 0;
    let mut equal_zero: usize = 0;
    let mut greater_zero: usize = 0;
    let mut lesser_zero: usize = 0;
    let mut not_eq_zero: usize = 0;
    let mut all: usize = 0;
    let mut indeterminate: usize = 0;

    if let Some(results) = results {

        for parse_result in results {
            //println!("{:?}", wrapper_function);
            total += 1;
            match parse_result.error_spec {
                ErrorSpec::Empty => empty += 1,
                ErrorSpec::GrEqZero => gr_eq_zero += 1,
                ErrorSpec::LesEqZero => les_eq_zero += 1,
                ErrorSpec::EqualZero => equal_zero += 1,
                ErrorSpec::GreaterZero => greater_zero += 1,
                ErrorSpec::LesserZero => lesser_zero += 1,
                ErrorSpec::NotEqZero => not_eq_zero += 1,
                ErrorSpec::All => all += 1,
                ErrorSpec::Indeterminate => indeterminate += 1,
                _ => indeterminate += 1,
            }
        }
    
        println!("\n\nESSS Parsing Statistics:");
        println!("Total parsed functions: {}", total);
        println!("Empty: {}", empty);
        println!("GrEqZero: {}", gr_eq_zero);
        println!("LesEqZero: {}", les_eq_zero);
        println!("EqualZero: {}", equal_zero);
        println!("GreaterZero: {}", greater_zero);
        println!("LesserZero: {}", lesser_zero);
        println!("NotEqZero: {}", not_eq_zero);
        println!("All: {}", all);
        println!("Indeterminate: {}", indeterminate);
    } else {
        // if there are no specs, do nothing
        return;
    }
}