//! Python bindings for the Outpack query parser.
//!
//! This file exports a Python module named `outpack_query_parser` which can be used from a Python
//! application to parse an Outpack query.
//!
//! # Example:
//! ```py
//! from outpack_query_parser import parse_query
//! print(parse_query("name == 'foo'"))
//! # Prints:
//! # Test(operator=TestOperator.Equal, lhs=LookupName(), rhs=Literal(value='foo'))
//! ```

use crate::query::query_types::{self, Literal, Lookup, PacketLookup, QueryNode, TestValue};
use crate::query::ParseError;
use lazy_static::lazy_static;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

/// Set of Python AST Classes.
///
/// These generally mirror the native Rust types used by the parser.
/// Rust-style enums don't have a Python equivalent. These are instead represented with a separate
/// class for each variant. Variants can be distinguised using `isinstance`:
///
/// ```py
/// if isinstance(node, Literal):
///     print("Literal: ", node.value)
/// elif isinstance(node, LookupParameter):
///     print("Parameter lookup: ", node.name)
/// ```
///
/// The classes are defined using the dataclasses module and have typical Python semantics.
#[allow(non_snake_case)]
struct Classes {
    Latest: PyObject,
    Single: PyObject,
    Negation: PyObject,
    Brackets: PyObject,
    Test: PyObject,
    BooleanExpr: PyObject,

    Literal: PyObject,
    LookupThis: PyObject,
    LookupEnvironment: PyObject,
    LookupParameter: PyObject,
    LookupId: PyObject,
    LookupName: PyObject,
}

lazy_static! {
    static ref CLASSES: Classes = {
        Python::with_gil(|py| {
            let dataclasses = py.import("dataclasses").unwrap();
            let make_dataclass = |name, fields: &[&str]| -> PyObject {
                let fields = PyTuple::new(py, fields);
                dataclasses
                    .call_method1("make_dataclass", (name, fields))
                    .unwrap()
                    .into()
            };

            Classes {
                Latest: make_dataclass("Latest", &["inner"]),
                Single: make_dataclass("Single", &["inner"]),
                Brackets: make_dataclass("Brackets", &["inner"]),
                Negation: make_dataclass("Negation", &["inner"]),
                Test: make_dataclass("Test", &["operator", "lhs", "rhs"]),
                BooleanExpr: make_dataclass("BooleanExpr", &["operator", "lhs", "rhs"]),

                Literal: make_dataclass("Literal", &["value"]),
                LookupThis: make_dataclass("LookupThis", &["name"]),
                LookupEnvironment: make_dataclass("LookupEnvironment", &["name"]),
                LookupParameter: make_dataclass("LookupParameter", &["name"]),
                LookupId: make_dataclass("LookupId", &[]),
                LookupName: make_dataclass("LookupName", &[]),
            }
        })
    };
}

#[pyclass]
enum BooleanOperator {
    And,
    Or,
}

#[pyclass]
enum TestOperator {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[pyfunction]
fn parse_query<'a>(py: Python, input: &'a str) -> PyResult<PyObject> {
    convert_query(py, crate::query::parse_query(input)?)
}

#[pymodule]
fn outpack_query_parser(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_query, m)?)?;

    m.add("Latest", &CLASSES.Latest)?;
    m.add("Single", &CLASSES.Single)?;
    m.add("Brackets", &CLASSES.Brackets)?;
    m.add("Negation", &CLASSES.Negation)?;
    m.add("Test", &CLASSES.Test)?;
    m.add("BooleanExpr", &CLASSES.BooleanExpr)?;

    m.add("Literal", &CLASSES.Literal)?;
    m.add("LookupThis", &CLASSES.LookupThis)?;
    m.add("LookupEnvironment", &CLASSES.LookupEnvironment)?;
    m.add("LookupParameter", &CLASSES.LookupParameter)?;
    m.add("LookupId", &CLASSES.LookupId)?;
    m.add("LookupName", &CLASSES.LookupName)?;

    // PyO3's `#[pyclass]` does a decent job of generating idiomatic code for enums that don't have
    // any data. We can just use these rather than eg. calling the Python `enum` package.
    m.add_class::<BooleanOperator>()?;
    m.add_class::<TestOperator>()?;
    Ok(())
}

impl From<ParseError> for PyErr {
    fn from(err: ParseError) -> PyErr {
        PyValueError::new_err(err.to_string())
    }
}

fn convert_query(py: Python, query: QueryNode) -> PyResult<PyObject> {
    match query {
        QueryNode::Latest(None) => CLASSES.Latest.call1(py, (py.None(),)),
        QueryNode::Latest(Some(inner)) => CLASSES.Latest.call1(py, (convert_query(py, *inner)?,)),
        QueryNode::Single(inner) => CLASSES.Single.call1(py, (convert_query(py, *inner)?,)),
        QueryNode::Negation(inner) => CLASSES.Negation.call1(py, (convert_query(py, *inner)?,)),
        QueryNode::Brackets(inner) => CLASSES.Brackets.call1(py, (convert_query(py, *inner)?,)),

        QueryNode::Test(test_type, lhs, rhs) => CLASSES.Test.call1(
            py,
            (
                test_type.to_object(py),
                convert_test_value(py, lhs)?,
                convert_test_value(py, rhs)?,
            ),
        ),

        QueryNode::BooleanExpr(operator, lhs, rhs) => CLASSES.BooleanExpr.call1(
            py,
            (
                operator.to_object(py),
                convert_query(py, *lhs)?,
                convert_query(py, *rhs)?,
            ),
        ),
    }
}

fn convert_test_value(py: Python, test_value: TestValue) -> PyResult<PyObject> {
    match test_value {
        TestValue::Lookup(Lookup::Packet(PacketLookup::Name)) => CLASSES.LookupName.call0(py),
        TestValue::Lookup(Lookup::Packet(PacketLookup::Id)) => CLASSES.LookupId.call0(py),
        TestValue::Lookup(Lookup::Packet(PacketLookup::Parameter(name))) => {
            CLASSES.LookupParameter.call1(py, (name,))
        }
        TestValue::Lookup(Lookup::This(name)) => CLASSES.LookupThis.call1(py, (name,)),
        TestValue::Lookup(Lookup::Environment(name)) => {
            CLASSES.LookupEnvironment.call1(py, (name,))
        }

        TestValue::Literal(literal) => {
            let value = match literal {
                Literal::Bool(b) => b.to_object(py),
                Literal::String(s) => s.to_object(py),
                Literal::Number(n) => n.to_object(py),
            };
            CLASSES.Literal.call1(py, (value,))
        }
    }
}

impl ToPyObject for query_types::TestOperator {
    fn to_object(&self, py: Python) -> PyObject {
        match self {
            query_types::TestOperator::Equal => TestOperator::Equal,
            query_types::TestOperator::NotEqual => TestOperator::NotEqual,
            query_types::TestOperator::LessThan => TestOperator::LessThan,
            query_types::TestOperator::LessThanOrEqual => TestOperator::LessThanOrEqual,
            query_types::TestOperator::GreaterThan => TestOperator::GreaterThan,
            query_types::TestOperator::GreaterThanOrEqual => TestOperator::GreaterThanOrEqual,
        }
        .into_py(py)
    }
}

impl ToPyObject for query_types::BooleanOperator {
    fn to_object(&self, py: Python) -> PyObject {
        match self {
            query_types::BooleanOperator::And => BooleanOperator::And,
            query_types::BooleanOperator::Or => BooleanOperator::Or,
        }
        .into_py(py)
    }
}
