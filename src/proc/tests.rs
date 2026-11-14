use super::*;
use std::process::Command;

#[test]
fn a_missing_program_is_an_error_and_a_real_one_is_not() {
    assert!(detached(Command::new("boxvibe-no-such-program")).is_err());
    assert!(detached(Command::new("true")).is_ok());
}
