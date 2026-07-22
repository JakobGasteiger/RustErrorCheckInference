use crate::{parser::common::ParseError, utils::error_spec::{ErrorSpec, FunctionErrorSpec}};


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
        "bottom" => ErrorSpec::Empty,
        "<0" => ErrorSpec::LesserZero,
        ">0" => ErrorSpec::GreaterZero,
        "<=0" => ErrorSpec::LesEqZero,
        ">=0" => ErrorSpec::GrEqZero,
        "==0" => ErrorSpec::EqualZero,
        "!=0" => ErrorSpec::NotEqZero,
        "top" => ErrorSpec::All,
        _ => ErrorSpec::Indeterminate,
    };

    println!("Predicate for {function_name} is {predicate:?}");
    FunctionErrorSpec::new(function_name, predicate)
}

pub fn parse_eesi() -> Vec<FunctionErrorSpec> {

    eprintln!("EESI parser active!");

    if let Ok(spec_strings) = get_function_spec_strings() {
        let mut specs = Vec::new();
        for spec_string in spec_strings {
            specs.push(parse_spec_string(spec_string));
        }
        return specs
    } else {
        return Vec::new();
    }

}