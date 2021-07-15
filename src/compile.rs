use sexp::{Atom, Sexp};
use std::collections::HashMap;

use crate::ast::{Argument, Declaration, Expression, FuncCall, Policy, Statement};
use crate::constants;
use crate::error::{HLLCompileError, HLLError, HLLInternalError};
use crate::internal_rep::{AvRule, AvRuleFlavor, TypeInfo};

pub fn compile(p: &Policy) -> Result<sexp::Sexp, Vec<HLLError>> {
    let type_map = build_type_map(p);
    let type_decl_list = organize_type_map(&type_map)?;

    let av_rules = do_rules_pass(&type_map, &p.exprs)?;

    // TODO: The rest of compilation
    let cil_types = type_list_to_sexp(type_decl_list);
    let cil_av_rules = av_list_to_sexp(av_rules);
    let mut ret = cil_types;
    ret.extend(cil_av_rules.iter().cloned());
    Ok(Sexp::List(ret))
}

// TODO: Currently is domains only
fn build_type_map(p: &Policy) -> HashMap<String, TypeInfo> {
    let mut decl_map = HashMap::new();
    // TODO: This only allows domain declarations at the top level.  Is that okay?  I'm too tired
    // to think about it
    for e in &p.exprs {
        let d = match e {
            Expression::Decl(d) => d,
            _ => continue,
        };
        match d {
            Declaration::Type(t) => decl_map.insert(t.name.clone(), TypeInfo::new(&**t)),
            Declaration::Func(_) => continue,
        };
    }

    decl_map
}

// If a type couldn't be organized, it is either a cycle or a non-existant parent somewhere
// The claim that a type must have at least one parent is enforced by the parser
// This function walks the tree from a given type and determines which of these cases we are in
// Return a Vector of found errors.  This Vector can be empty in internal calls, but should not be
// when called from another function.
fn find_cycles_or_bad_types(
    type_to_check: &str,
    types: &HashMap<String, TypeInfo>,
    visited_types: HashSet<&str>,
) -> Vec<HLLError> {
    let mut ret = Vec::new();

    // type_to_check was generated from types.keys(), so it's guaranteed to be Some
    for p in &types.get(&type_to_check.to_string()).unwrap().inherits {
        // First, move the bad type check from below up here

        if visited_types.contains(&p as &str) || p == type_to_check {
            // cycle
            return vec![HLLError::Compile(HLLCompileError {
                filename: "TODO".to_string(),
                lineno: 0,
                msg: "TODO: Write cycle error message".to_string(),
            })];
        }
        let mut new_visited_types = visited_types.clone();
        new_visited_types.insert(type_to_check);

        // What do I do with this return?
        // I think I need to be returning an Option<Vec>.  This can be either None, in the event
        // that this wasn't a bad branch, or a vector of the issues detected.
        // Then in generate_type_no_parent_errors, we regroup (remove dups, return an internal
        // error on None.
        // Probably rather than handle all the error generation here, we should actually just
        // return info about which type had the issue and which issue.  That will make deduping
        // easier and we can generate errors a level up
        let mut parent_errors = find_cycles_or_bad_types(&p, types, new_visited_types);
        ret.append(&mut parent_errors)
    }

    ret
}

fn generate_type_no_parent_errors(
    missed_types: HashSet<&String>,
    types: &HashMap<String, TypeInfo>,
) -> Vec<HLLError> {
    let mut ret = Vec::new();
    for t in &missed_types {
        let mut errors = find_cycles_or_bad_types(&t, types, HashSet::new());
        if errors.is_empty() {
            return vec![HLLError::Internal(HLLInternalError {})];
        } else {
            ret.append(&mut errors);
        }
    }
    // TODO: Deduplication
    ret
}

// This function validates that the relationships in the HashMap are valid, and organizes a Vector
// of type declarations in a reasonable order to be output into CIL.
// In order to be valid, the types must meet the following properties:
// 1. All types have at least one parent
// 2. All listed parents are themselves types (or "domain" or "resource")
// 3. No cycles exist
fn organize_type_map<'a>(
    types: &'a HashMap<String, TypeInfo>,
) -> Result<Vec<&'a TypeInfo>, Vec<HLLError>> {
    let mut tmp_types: HashMap<&String, &TypeInfo> = types.iter().collect();

    let mut out: Vec<&TypeInfo> = Vec::new();

    while !tmp_types.is_empty() {
        let mut current_pass_types: Vec<&TypeInfo> = Vec::new();

        for ti in tmp_types.values() {
            let mut wait = false;

            // TODO: Do we need to consider the case when inherits is empty?  Theoretically it
            // should have always been populated with at least domain or resource by the parser.
            // Should probably return an internal error if that hasn't happened
            for key in &ti.inherits {
                if key != "domain" && key != "resource" && out.iter().any(|&x| &x.name == key) {
                    wait = true;
                    continue;
                }
            }
            if !wait {
                // This means all the parents are previously listed
                current_pass_types.push(ti);
            }
        }
        if current_pass_types.is_empty() && !tmp_types.is_empty() {
            // We can't satify the parents for all types
            // TODO: Better error handling
            return Err(generate_type_no_parent_errors(tmp_type_names, types));
        }
        for t in &current_pass_types {
            tmp_types.remove(&t.name);
        }
        out.append(&mut current_pass_types);
    }
    Ok(out)
}

fn do_rules_pass<'a>(
    types: &'a HashMap<String, TypeInfo>,
    exprs: &'a Vec<Expression>,
) -> Result<Vec<AvRule<'a>>, Vec<HLLError>> {
    let mut ret = Vec::new();
    for e in exprs {
        match e {
            Expression::Stmt(Statement::Call(c)) => {
                if c.is_builtin() {
                    let av_rule = call_to_av_rule(&**c, types)?;
                    ret.push(av_rule);
                }
            }
            Expression::Decl(Declaration::Type(t)) => {
                let child_rules = do_rules_pass(types, &t.expressions)?;
                ret.extend(child_rules.iter().cloned());
            }
            _ => continue,
        }
    }
    Ok(ret)
}

fn argument_to_typeinfo<'a>(
    a: &Argument,
    types: &'a HashMap<String, TypeInfo>,
) -> Result<&'a TypeInfo, HLLError> {
    // TODO: Handle the "this" keyword
    let t: Option<&TypeInfo> = match a {
        Argument::Var(s) => types.get(s),
        _ => None,
    };

    t.ok_or(HLLError::Compile(HLLCompileError {
        filename: "TODO".to_string(),
        lineno: 0,
        msg: format!("{:?} is not a valid type", a),
    }))
}

// TODO: This can be converted into a TryFrom for more compile time gaurantees
fn call_to_av_rule<'a>(
    c: &'a FuncCall,
    types: &'a HashMap<String, TypeInfo>,
) -> Result<AvRule<'a>, HLLError> {
    let flavor = match c.name.as_str() {
        constants::ALLOW_FUNCTION_NAME => AvRuleFlavor::Allow,
        constants::DONTAUDIT_FUNCTION_NAME => AvRuleFlavor::Dontaudit,
        constants::AUDITALLOW_FUNCTION_NAME => AvRuleFlavor::Auditallow,
        constants::NEVERALLOW_FUNCTION_NAME => AvRuleFlavor::Neverallow,
        _ => return Err(Box::new(HLLInternalError {})),
    };

    if c.args.len() != 4 {
        return Err(Box::new(HLLCompileError {}));
    }

    let source = argument_to_typeinfo(&c.args[0], types)?;
    let target = argument_to_typeinfo(&c.args[1], types)?;
    let class = match &c.args[2] {
        Argument::Var(s) => s,
        a => {
            return Err(HLLError::Compile(HLLCompileError {
                filename: "TODO".to_string(),
                lineno: 0,
                msg: format!("Expected an object class, got {:?}", a),
            }))
        }
    };
    let perms = match &c.args[3] {
        Argument::List(l) => l.iter().map(|s| s as &str).collect(),
        // TODO, a Var can probably be coerced.  This is the @makelist annotation case
        p => {
            return Err(HLLError::Compile(HLLCompileError {
                filename: "TODO".to_string(),
                lineno: 0,
                msg: format!("Expected a list of permissions, got {:?}", p),
            }))
        }
    };

    // TODO: Validate number of args, lack of class_name
    Ok(AvRule {
        av_rule_flavor: flavor,
        source: source,
        target: target,
        class: class,
        perms: perms,
    })
}

fn type_list_to_sexp(types: Vec<&TypeInfo>) -> Vec<sexp::Sexp> {
    let mut ret = Vec::new();
    for t in types {
        ret.push(Sexp::List(vec![
            Sexp::Atom(Atom::S("type".to_string())),
            Sexp::Atom(Atom::S(t.name.clone())),
        ]))
    }
    ret
}

fn av_list_to_sexp<'a, T>(av_rules: T) -> Vec<sexp::Sexp>
where
    T: IntoIterator<Item = AvRule<'a>>,
{
    av_rules.into_iter().map(|r| Sexp::from(r)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Declaration, Expression, Policy, TypeDecl};
    use crate::internal_rep::TypeInfo;

    #[test]
    fn build_type_map_test() {
        let mut exprs = Vec::new();
        exprs.push(Expression::Decl(Declaration::Type(Box::new(
            TypeDecl::new("foo".to_string(), vec!["domain".to_string()], Vec::new()),
        ))));
        let p = Policy::new(exprs);
        let types = build_type_map(&p);
        match types.get("foo") {
            Some(foo) => assert_eq!(foo.name, "foo"),
            None => panic!("Foo is not in hash map"),
        }
    }

    #[test]
    fn organize_type_map_test() {
        let mut types: HashMap<String, TypeInfo> = HashMap::new();
        let foo_type = TypeInfo::new(&TypeDecl::new(
            "foo".to_string(),
            vec!["domain".to_string()],
            Vec::new(),
        ));
        let bar_type = TypeInfo::new(&TypeDecl::new(
            "bar".to_string(),
            vec!["domain".to_string(), "foo".to_string()],
            Vec::new(),
        ));
        let baz_type = TypeInfo::new(&TypeDecl::new(
            "baz".to_string(),
            vec!["domain".to_string(), "foo".to_string(), "bar".to_string()],
            Vec::new(),
        ));
        types.insert("foo".to_string(), foo_type);
        types.insert("bar".to_string(), bar_type);
        types.insert("baz".to_string(), baz_type);

        let _type_vec = organize_type_map(&types).unwrap();
        //assert_eq!(types.name, "domain");
        //assert_eq!(*types.parent, None);
        //assert_eq!(types.children.len(), 1);
        //assert_eq!(types.children[0].name, "foo");
        // TODO: This is hard to satisfy the borrow checker with.  Let's get everything else
        // working and come back to it
        //assert_eq!(*types.children[0].parent, Some(types));
        //assert_eq!(types.children[0].children.len(), 0);
    }
}
