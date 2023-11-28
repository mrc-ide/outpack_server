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
//! # Test(operator=Operator.Equal, lhs=LookupName(), rhs=Literal(value='foo'))
//! ```
//!
//! Most of the glue is handled by the PyO3 crate. Calling into the actual parser is trivially done
//! by the [`parse_query`] function. Most of the module's code is responsible for setting a
//! parallel AST type hiearchy and implementing conversion from the query_types module to these
//! types.

use crate::query::query_types as ast;
use crate::query::ParseError;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyNone, PyString, PyTuple};

#[pyfunction]
fn parse_query<'a>(input: &'a str) -> Result<ast::QueryNode<'a>, ParseError> {
    crate::query::parse_query(input)
}

#[pymodule]
fn outpack_query_parser(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_query, m)?)?;
    m.add_class::<Latest>()?;
    m.add_class::<Single>()?;
    m.add_class::<Test>()?;
    m.add_class::<Negation>()?;
    m.add_class::<Brackets>()?;
    m.add_class::<BooleanOperator>()?;
    m.add_class::<Operator>()?;
    m.add_class::<Literal>()?;
    m.add_class::<LookupThis>()?;
    m.add_class::<LookupParameter>()?;
    m.add_class::<LookupId>()?;
    m.add_class::<LookupName>()?;
    Ok(())
}

/// Concatenate string literals together, interspersing a separator between each element.
///
/// The first argument is the separator. The elements to concatenate are passed as the subsequent
/// elements.
///
/// ```ignore(https://github.com/rust-lang/rust/issues/97030)
/// assert_eq!(intersperse!(", ", "a", "b", "c"), "a, b, c");
/// ```
macro_rules! intersperse {
    ($sep:expr  $(,)?) => { "" };
    ($sep:expr, $head:expr $(,)?) => { $head };
    ($sep:expr, $head:expr, $($tail:expr),+ $(,)?) => {
        concat!($head, $sep, intersperse!($sep, $($tail,)+))
    };
}

/// Define a Python class with dataclass-like semantics.
///
/// The class will have a constructor, __repr__ and __eq__ methods and getters for each field.
///
/// Fields must be Python values, wrapped in a `Py<T>`, such as `Py<PyAny>`, `Py<PyString>` or
/// `Py<C>` where `C` is another struct with a `#[pyclass]` annotation.
macro_rules! dataclass {
    () => {};

    (struct $name:ident ; $($rest:tt)* ) => {
        #[pyclass(frozen)]
        struct $name;

        dataclass_impl!(struct $name { });
        dataclass!($($rest)*);
    };

    (struct $name:ident { $($field_name:ident : $field_type:ty),* $(,)? } $($rest:tt)* ) => {
        #[pyclass(frozen, get_all)]
        struct $name {
            $($field_name: $field_type),*
        }
        dataclass_impl!(struct $name { $($field_name: $field_type),* });
        dataclass!($($rest)*);
    };
}

macro_rules! dataclass_impl {
    (struct $name:ident { $($field_name:ident : $field_type:ty),* $(,)? } ) => {
        #[pymethods]
        impl $name {
            #[new]
            fn new($($field_name: $field_type),*) -> Self {
                Self { $($field_name,)* }
            }

            fn __repr__(&self, #[allow(unused)] py: Python<'_>) -> PyResult<String> {
                Ok(format!(concat!(
                    stringify!($name),
                    "(",
                    intersperse!(", ", $(concat!(stringify!($field_name), "={}"),)*),
                    ")"), $(self.$field_name.as_ref(py).repr()?),*))
            }

            fn __eq__(&self, py: Python<'_>, other: PyObject) -> PyResult<bool> {
                if let Ok(other) = other.downcast::<PyCell<Self>>(py) {
                    #[allow(unused)]
                    let other_inner = other.get();
                    Ok($(self.$field_name.as_ref(py).eq(&other_inner.$field_name)? &&)* true)
                } else {
                    Ok(false)
                }
            }

            // This allows match statements with positional sub-patterns.
            // See https://peps.python.org/pep-0622/#the-match-protocol
            #[classattr]
            fn __match_args__<'a>(py: Python<'a>) -> &'a PyTuple {
                PyTuple::new::<&str, _>(py, [ $(stringify!($field_name),)* ])
            }
        }
    }
}

dataclass! {
    struct Test {
        operator: PyObject,
        lhs: PyObject,
        rhs: PyObject,
    }
    struct BooleanOperator {
        operator: PyObject,
        lhs: PyObject,
        rhs: PyObject,
    }
    struct Latest {
        inner: PyObject,
    }
    struct Single {
        inner: PyObject,
    }
    struct Negation {
        inner: PyObject,
    }
    struct Brackets {
        inner: PyObject,
    }
    struct Literal {
        value: PyObject,
    }
    struct LookupThis {
        name: Py<PyString>,
    }
    struct LookupEnvironment {
        name: Py<PyString>,
    }
    struct LookupParameter {
        name: Py<PyString>,
    }
    struct LookupName;
    struct LookupId;
}

#[pyclass]
enum Operator {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
    And,
    Or,
}

impl From<ParseError> for PyErr {
    fn from(err: ParseError) -> PyErr {
        PyValueError::new_err(err.to_string())
    }
}

// parse_query uses this for automatic return type conversion.
// https://github.com/PyO3/pyo3/issues/1595
impl IntoPy<PyObject> for ast::QueryNode<'_> {
    fn into_py(self, py: Python) -> PyObject {
        ToPyObject::to_object(&self, py)
    }
}

impl ToPyObject for ast::QueryNode<'_> {
    fn to_object(&self, py: Python) -> PyObject {
        match self {
            ast::QueryNode::Latest(None) => Latest {
                inner: PyNone::get(py).to_object(py),
            }
            .into_py(py),

            ast::QueryNode::Latest(Some(inner)) => Latest {
                inner: inner.to_object(py),
            }
            .into_py(py),

            ast::QueryNode::Single(inner) => Single {
                inner: inner.to_object(py),
            }
            .into_py(py),

            ast::QueryNode::Negation(inner) => Negation {
                inner: inner.to_object(py),
            }
            .into_py(py),

            ast::QueryNode::Brackets(inner) => Brackets {
                inner: inner.to_object(py),
            }
            .into_py(py),

            ast::QueryNode::Test(operator, lhs, rhs) => Test {
                operator: operator.to_object(py),
                lhs: lhs.to_object(py),
                rhs: rhs.to_object(py),
            }
            .into_py(py),

            ast::QueryNode::BooleanOperator(operator, lhs, rhs) => BooleanOperator {
                operator: operator.to_object(py),
                lhs: lhs.to_object(py),
                rhs: rhs.to_object(py),
            }
            .into_py(py),
        }
    }
}

impl ToPyObject for ast::TestValue<'_> {
    fn to_object(&self, py: Python) -> PyObject {
        match self {
            ast::TestValue::Lookup(inner) => inner.to_object(py),
            ast::TestValue::Literal(inner) => inner.to_object(py),
        }
    }
}

impl ToPyObject for ast::Literal<'_> {
    fn to_object(&self, py: Python) -> PyObject {
        match self {
            ast::Literal::Bool(b) => Literal {
                value: b.to_object(py),
            },
            ast::Literal::String(s) => Literal {
                value: s.to_object(py),
            },
            ast::Literal::Number(x) => Literal {
                value: x.to_object(py),
            },
        }
        .into_py(py)
    }
}

impl ToPyObject for ast::Lookup<'_> {
    fn to_object(&self, py: Python) -> PyObject {
        match self {
            ast::Lookup::Packet(ast::PacketLookup::Name) => LookupName {}.into_py(py),
            ast::Lookup::Packet(ast::PacketLookup::Id) => LookupId {}.into_py(py),
            ast::Lookup::Packet(ast::PacketLookup::Parameter(name)) => LookupParameter {
                name: name.into_py(py),
            }
            .into_py(py),

            ast::Lookup::This(name) => LookupThis {
                name: name.into_py(py),
            }
            .into_py(py),

            ast::Lookup::Environment(name) => LookupEnvironment {
                name: name.into_py(py),
            }
            .into_py(py),
        }
    }
}

impl ToPyObject for ast::Test {
    fn to_object(&self, py: Python) -> PyObject {
        match self {
            ast::Test::Equal => Operator::Equal,
            ast::Test::NotEqual => Operator::NotEqual,
            ast::Test::LessThan => Operator::LessThan,
            ast::Test::LessThanOrEqual => Operator::LessThanOrEqual,
            ast::Test::GreaterThan => Operator::GreaterThan,
            ast::Test::GreaterThanOrEqual => Operator::GreaterThanOrEqual,
        }
        .into_py(py)
    }
}

impl ToPyObject for ast::Operator {
    fn to_object(&self, py: Python) -> PyObject {
        match self {
            ast::Operator::And => Operator::And,
            ast::Operator::Or => Operator::Or,
        }
        .into_py(py)
    }
}

