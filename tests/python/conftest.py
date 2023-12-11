import sys

collect_ignore = []

if sys.version_info < (3, 10):
    # This test uses pattern matching, which isn't supported in older Python versions.
    collect_ignore.append("test_pattern_match.py")
