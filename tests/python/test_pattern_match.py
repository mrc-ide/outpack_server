import pytest
from outpack_query_parser import parse_query, Latest, Operator, Literal, LookupName, Literal

# Calling this Test makes pytest think it's a test class and freak out
from outpack_query_parser import Test as NodeTest

def test_pattern_match():
    match parse_query("latest"):
        case Latest(None):
            pass
        case _:
            pytest.fail("pattern did not match")

    match parse_query("latest(name == 'foo')"):
        case Latest(NodeTest()):
            pass
        case _:
            pytest.fail("pattern did not match")

    match parse_query("name == 'foo'"):
        case NodeTest(Operator.Equal, LookupName, Literal("foo")):
            pass
        case _:
            pytest.fail("pattern did not match")
