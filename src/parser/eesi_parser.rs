use crate::{parser::common::ParseError, utils::error_spec::{ErrorSpecPredicate, FunctionErrorSpec}};


pub fn get_function_spec_strings() -> Result<Vec<String>, ParseError> {
    println!("\n");

    let path = std::env::current_dir().or(Err(ParseError::NoESSSFile))?.into_string().or(Err(ParseError::NoESSSFile))? + "/eesi_results.txt"; 
    println!("EESI Results File should be at {}", path);

    let raw_string = match std::fs::read_to_string(&path).or(Err(ParseError::WholeInput)) {
        Ok(raw_string) => raw_string,
        Err(err) => {   
            println!("-> not found!");
            return Err(err);
        }
    };

    let spec_strings = 
        raw_string
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    Ok(spec_strings)
}

pub fn parse_spec_string(spec_string: String) -> FunctionErrorSpec {

    println!("\nParsing Spec String: {spec_string}");

    let split = 
        spec_string
        .split(" ")
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let function_name = 
        split
        .get(1)
        .unwrap_or(&"ErrorParsingFunctionName".to_string())
        .clone();

    let predicate_string = split.get(2).unwrap_or(&"ErrorParsingPredicate".to_string()).clone();

    let predicate = match predicate_string.as_str() {
        "bottom" => ErrorSpecPredicate::Empty,
        "<0" => ErrorSpecPredicate::LesserZero,
        ">0" => ErrorSpecPredicate::GreaterZero,
        "<=0" => ErrorSpecPredicate::LesEqZero,
        ">=0" => ErrorSpecPredicate::GrEqZero,
        "==0" => ErrorSpecPredicate::EqualZero,
        "!=0" => ErrorSpecPredicate::NotEqZero,
        "top" => ErrorSpecPredicate::All,
        _ => ErrorSpecPredicate::Indeterminate,
    };

    println!("Predicate for {function_name} is {predicate:?}");
    FunctionErrorSpec::new(function_name, predicate)
}

pub fn parse_eesi() -> Option<Vec<FunctionErrorSpec>> {

    eprintln!("EESI parser active!");

    if let Ok(spec_strings) = get_function_spec_strings() {
        let mut specs = Vec::new();
        for spec_string in spec_strings {
            specs.push(parse_spec_string(spec_string));
        }
        return Some(specs);
    } else {
        return None;
    }

}

pub fn print_eesi_statistics(results: &Option<Vec<FunctionErrorSpec>>) {

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
                ErrorSpecPredicate::Empty => empty += 1,
                ErrorSpecPredicate::GrEqZero => gr_eq_zero += 1,
                ErrorSpecPredicate::LesEqZero => les_eq_zero += 1,
                ErrorSpecPredicate::EqualZero => equal_zero += 1,
                ErrorSpecPredicate::GreaterZero => greater_zero += 1,
                ErrorSpecPredicate::LesserZero => lesser_zero += 1,
                ErrorSpecPredicate::NotEqZero => not_eq_zero += 1,
                ErrorSpecPredicate::All => all += 1,
                ErrorSpecPredicate::Indeterminate => indeterminate += 1,
                _ => indeterminate += 1,
            }
        }
    
        println!("\n\nEESI Parsing Statistics:");
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