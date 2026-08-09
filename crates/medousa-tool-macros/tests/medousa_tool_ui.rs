#[test]
fn medousa_tool_signatures() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/pass_*.rs");
    tests.compile_fail("tests/ui/fail_*.rs");
}
