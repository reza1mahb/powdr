//! Formatting functions for analyzed PIL files.
//!
//! These are not meant to be 1-1 reproductions, they will have errors.
//! Do not use this to re-generate PIL files!

use std::{
    fmt::{Display, Formatter, Result},
    str::FromStr,
};

use itertools::Itertools;

use self::{
    parsed::asm::{AbsoluteSymbolPath, SymbolPath},
    types::{ArrayType, FunctionType, TupleType, Type},
};

use super::*;

impl<T: Display> Display for Analyzed<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let degree = self.degree.unwrap_or_default();
        let mut current_namespace = AbsoluteSymbolPath::default();
        let mut update_namespace = |name: &str, f: &mut Formatter<'_>| {
            let mut namespace =
                AbsoluteSymbolPath::default().join(SymbolPath::from_str(name).unwrap());
            let name = namespace.pop().unwrap();
            if namespace != current_namespace {
                current_namespace = namespace;
                writeln!(
                    f,
                    "namespace {}({degree});",
                    current_namespace.relative_to(&Default::default())
                )?;
            };
            Ok((name, !current_namespace.is_empty()))
        };

        for statement in &self.source_order {
            match statement {
                StatementIdentifier::Definition(name) => {
                    if let Some((symbol, definition)) = self.definitions.get(name) {
                        let (name, is_local) = update_namespace(name, f)?;
                        match symbol.kind {
                            SymbolKind::Poly(poly_type) => {
                                let kind = match &poly_type {
                                    PolynomialType::Committed => "witness ",
                                    PolynomialType::Constant => "fixed ",
                                    PolynomialType::Intermediate => panic!(),
                                };
                                write!(f, "    col {kind}{name}")?;
                                if let Some(length) = symbol.length {
                                    if let PolynomialType::Committed = poly_type {
                                        write!(f, "[{length}]")?;
                                        assert!(definition.is_none());
                                    } else {
                                        // Do not print an array size, because we will do it as part of the type.
                                        assert!(matches!(
                                            definition,
                                            Some(FunctionValueDefinition::Expression(
                                                TypedExpression { e: _, ty: Some(_) }
                                            ))
                                        ));
                                    }
                                }
                                if let Some(value) = definition {
                                    writeln!(f, "{value};")?
                                } else {
                                    writeln!(f, ";")?
                                }
                            }
                            SymbolKind::Constant() => {
                                let indentation = if is_local { "    " } else { "" };
                                let Some(FunctionValueDefinition::Expression(TypedExpression {
                                    e,
                                    ty: Some(Type::Fe),
                                })) = &definition
                                else {
                                    panic!(
                                        "Invalid constant value: {}",
                                        definition.as_ref().unwrap()
                                    );
                                };

                                writeln!(f, "{indentation}constant {name} = {e};",)?;
                            }
                            SymbolKind::Other() => {
                                write!(f, "    let {name}")?;
                                if let Some(value) = definition {
                                    write!(f, "{value}")?
                                }
                                writeln!(f, ";")?
                            }
                        }
                    } else if let Some((symbol, definition)) = self.intermediate_columns.get(name) {
                        let (name, _) = update_namespace(name, f)?;
                        assert_eq!(symbol.kind, SymbolKind::Poly(PolynomialType::Intermediate));
                        if let Some(length) = symbol.length {
                            writeln!(
                                f,
                                "    col {name}[{length}] = [{}];",
                                definition.iter().format(", ")
                            )?;
                        } else {
                            assert_eq!(definition.len(), 1);
                            writeln!(f, "    col {name} = {};", definition[0])?;
                        }
                    } else {
                        panic!()
                    }
                }
                StatementIdentifier::PublicDeclaration(name) => {
                    let decl = &self.public_declarations[name];
                    let (name, is_local) = update_namespace(&decl.name, f)?;
                    let indentation = if is_local { "    " } else { "" };
                    writeln!(
                        f,
                        "{indentation}public {name} = {}{}({});",
                        decl.polynomial,
                        decl.array_index
                            .map(|i| format!("[{i}]"))
                            .unwrap_or_default(),
                        decl.index
                    )?;
                }
                StatementIdentifier::Identity(i) => writeln!(f, "    {}", &self.identities[*i])?,
            }
        }

        Ok(())
    }
}

impl<T: Display> Display for FunctionValueDefinition<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            FunctionValueDefinition::Array(items) => {
                write!(f, " = {}", items.iter().format(" + "))
            }
            FunctionValueDefinition::Query(e) => format_outer_function(e, Some("query"), f),
            FunctionValueDefinition::Expression(TypedExpression { e, ty: None }) => {
                format_outer_function(e, None, f)
            }
            FunctionValueDefinition::Expression(TypedExpression { e, ty: Some(ty) })
                if *ty == Type::col() =>
            {
                format_outer_function(e, None, f)
            }
            FunctionValueDefinition::Expression(TypedExpression { e, ty: Some(ty) }) => {
                write!(f, ": {ty} = {e}")
            }
        }
    }
}

fn format_outer_function<T: Display>(
    e: &Expression<T>,
    qualifier: Option<&str>,
    f: &mut Formatter<'_>,
) -> Result {
    let q = qualifier.map(|s| format!(" {s}")).unwrap_or_default();
    match e {
        parsed::Expression::LambdaExpression(lambda) if lambda.params.len() == 1 => {
            let body = if q.is_empty() {
                format!("{{ {} }}", lambda.body)
            } else {
                format!("{}", lambda.body)
            };
            write!(f, "({}){q} {body}", lambda.params.iter().format(", "),)
        }
        _ => write!(f, " ={q} {e}"),
    }
}

impl<T: Display> Display for RepeatedArray<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.is_empty() {
            return Ok(());
        }
        write!(f, "[{}]", self.pattern.iter().format(", "))?;
        if self.is_repeated() {
            write!(f, "*")?;
        }
        Ok(())
    }
}

impl<T: Display> Display for Identity<Expression<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.kind {
            IdentityKind::Polynomial => {
                let expression = self.expression_for_poly_id();
                if let Expression::BinaryOperation(left, BinaryOperator::Sub, right) = expression {
                    write!(f, "{left} = {right};")
                } else {
                    write!(f, "{expression} = 0;")
                }
            }
            IdentityKind::Plookup => write!(f, "{} in {};", self.left, self.right),
            IdentityKind::Permutation => write!(f, "{} is {};", self.left, self.right),
            IdentityKind::Connect => write!(f, "{} connect {};", self.left, self.right),
        }
    }
}

impl<T: Display> Display for Identity<AlgebraicExpression<T>> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.kind {
            IdentityKind::Polynomial => {
                let expression = self.expression_for_poly_id();
                if let AlgebraicExpression::BinaryOperation(
                    left,
                    AlgebraicBinaryOperator::Sub,
                    right,
                ) = expression
                {
                    write!(f, "{left} = {right};")
                } else {
                    write!(f, "{expression} = 0;")
                }
            }
            IdentityKind::Plookup => write!(f, "{} in {};", self.left, self.right),
            IdentityKind::Permutation => write!(f, "{} is {};", self.left, self.right),
            IdentityKind::Connect => write!(f, "{} connect {};", self.left, self.right),
        }
    }
}

impl<Expr: Display> Display for SelectedExpressions<Expr> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "{}{{ {} }}",
            self.selector
                .as_ref()
                .map(|s| format!("{s} "))
                .unwrap_or_default(),
            self.expressions.iter().format(", ")
        )
    }
}

impl Display for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reference::LocalVar(_index, name) => {
                write!(f, "{name}")
            }
            Reference::Poly(r) => write!(f, "{r}"),
        }
    }
}

impl<T: Display> Display for AlgebraicExpression<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            AlgebraicExpression::Reference(reference) => write!(f, "{reference}"),
            AlgebraicExpression::PublicReference(name) => write!(f, ":{name}"),
            AlgebraicExpression::Number(value) => write!(f, "{value}"),
            AlgebraicExpression::BinaryOperation(left, op, right) => {
                write!(f, "({left} {op} {right})")
            }
            AlgebraicExpression::UnaryOperation(op, exp) => write!(f, "{op}{exp}"),
        }
    }
}

impl Display for AlgebraicUnaryOperator {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        UnaryOperator::from(*self).fmt(f)
    }
}

impl Display for AlgebraicBinaryOperator {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        BinaryOperator::from(*self).fmt(f)
    }
}

impl Display for AlgebraicReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name, if self.next { "'" } else { "" },)
    }
}

impl Display for PolynomialReference {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", self.name,)
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            Type::Bool => write!(f, "bool"),
            Type::Int => write!(f, "int"),
            Type::Fe => write!(f, "fe"),
            Type::String => write!(f, "string"),
            Type::Expr => write!(f, "expr"),
            Type::Constr => write!(f, "constr"),
            Type::Array(ar) => write!(f, "{ar}"),
            Type::Tuple(tu) => write!(f, "{tu}"),
            Type::Function(fun) => write!(f, "{fun}"),
        }
    }
}

impl Display for ArrayType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let length = self.length.iter().format("");
        if self.base.needs_parentheses() {
            write!(f, "({})[{length}]", self.base)
        } else {
            write!(f, "{}[{length}]", self.base)
        }
    }
}

impl Display for TupleType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "({})", format_list_of_types(&self.items))
    }
}

impl Display for FunctionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if *self == Self::col() {
            write!(f, "col")
        } else {
            write!(
                f,
                "{} -> {}",
                format_list_of_types(&self.params),
                self.value
            )
        }
    }
}

fn format_list_of_types(types: &[Type]) -> String {
    types
        .iter()
        .map(|x| {
            if x.needs_parentheses() {
                format!("({x})")
            } else {
                x.to_string()
            }
        })
        .format(", ")
        .to_string()
}
