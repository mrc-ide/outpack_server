use std::cmp::Ordering;

#[derive(Debug, PartialEq)]
pub enum Lookup<'a> {
    Name,
    Id,
    Parameter(&'a str)
}

#[derive(Debug, PartialEq)]
pub enum Literal<'a> {
    Bool(bool),
    String(&'a str),
    Number(f64)
}

impl<'a> PartialOrd for Literal<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (Literal::Number(num_1), Literal::Number(num_2)) => num_1.partial_cmp(num_2),
            (_, _) => None
        }
    }
}

#[derive(Debug)]
pub enum Test {
    Equal,
    NotEqual,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Debug)]
pub enum QueryNode<'a> {
    Latest(Option<Box<QueryNode<'a>>>),
    Test(Test, Lookup<'a>, Literal<'a>),
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_partial_eq_ord_works() {
        let lit_num1 = Literal::Number(10f64);
        let lit_num2 = Literal::Number(10f64);
        let lit_num3 = Literal::Number(11.1);
        let lit_bool1 = Literal::Bool(true);
        let lit_bool2 = Literal::Bool(false);
        let lit_str1 = Literal::String("test");
        let lit_str2 = Literal::String("test2");

        assert_eq!(lit_num1, lit_num2);
        assert_ne!(lit_num2, lit_num3);
        assert_ne!(lit_num3, lit_bool1);
        assert_ne!(lit_bool1, lit_bool2);
        assert_ne!(lit_bool2, lit_str1);
        assert_ne!(lit_str1, lit_str2);

        assert!(lit_num1 < lit_num3);
        assert_eq!(lit_num3.partial_cmp(&lit_num1), Some(Ordering::Greater));
        assert!(lit_num1 <= lit_num2);
        assert!(lit_num3 > lit_num1);

        // Is undefined on non-number variants
        assert!(lit_bool1.partial_cmp(&lit_bool2).is_none());
        assert!(lit_bool2.partial_cmp(&lit_bool1).is_none());
    }
}
