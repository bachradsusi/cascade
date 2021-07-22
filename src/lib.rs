#[macro_use]
extern crate lalrpop_util;

mod ast;
mod compile;
mod constants;
pub mod error;
mod functions;
mod internal_rep;

use error::HLLError;
use std::fs::File;
use std::io::{Error as IOError, Read};

lalrpop_mod!(pub parser);

// TODO: Should use a more specific error type
pub fn compile_system_policy(
    input_files: Vec<&mut File>
) -> Result<String, Vec<error::HLLError>> {
    let mut policies: Vec<Box<ast::Policy>> = Vec::new();
    for f in input_files {
        let mut policy_str = String::new();
        match f.read_to_string(&mut policy_str) {
            Ok(_) => (),
            Err(e) => return Err(Vec::from(HLLError::from(e))),
        }
        let p = match parse_policy(&policy_str) {
            Ok(p) => p,
            Err(e) => return Err(Vec::from(HLLError::from(e))),
        };

        policies.push(p);
    }

    // TODO: Combine multiple files
    let cil_tree = compile::compile(&*policies[0])?;

    Ok(generate_cil(cil_tree))

}

fn parse_policy<'a>(
    policy: &'a str,
) -> Result<
    Box<ast::Policy>,
    lalrpop_util::ParseError<usize, lalrpop_util::lexer::Token<'a>, &'static str>,
> {
    // TODO: Probably should only construct once
    // Why though?
    parser::PolicyParser::new().parse(policy)
}

fn generate_cil(s: sexp::Sexp) -> String {
    s.to_string()
}

#[cfg(test)]
mod tests {
    lalrpop_mod!(pub parser);

    use std::fs;

    const POLICIES_DIR: &str = "data/policies/";

    #[test]
    fn basic_expression_parse_test() {
        let res = parser::ExprParser::new().parse("domain foo {}");
        assert!(res.is_ok(), "Parse Error: {:?}", res);

        let res = parser::ExprParser::new().parse("virtual resource foo {}");
        assert!(res.is_ok(), "Parse Error: {:?}", res);

        let res = parser::ExprParser::new().parse("this.read();");
        assert!(res.is_ok(), "Parse Error: {:?}", res);
    }

    #[test]
    fn basic_policy_parse_test() {
        let policy_file = [POLICIES_DIR, "tmp_file.hll"].concat();
        let policy = fs::read_to_string(policy_file).unwrap();

        let res = parser::PolicyParser::new().parse(&policy);
        assert!(res.is_ok(), "Parse Error: {:?}", res);
    }
}
