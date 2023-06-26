use crate::metadata::ParameterValue;

#[derive(Debug)]
pub enum LookupLhs<'a> {
    Name,
    Id,
    Parameter(&'a str)
}

#[derive(Debug)]
pub enum QueryNode<'a> {
    Latest(Option<Box<QueryNode<'a>>>),
    Lookup(LookupLhs<'a>, ParameterValue<'a>),
}
