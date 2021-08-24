use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::process::Command; // Run programs
use std::path::PathBuf;

fn run(path: &str, query: &str) -> Command {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push(path);

    let mut cmd = Command::cargo_bin("syns").unwrap();
    cmd.arg(query).arg(d);
    cmd
}

#[test]
fn file_doesnt_exist() {
    let mut cmd = run("test/file/doesnt/exist", "foo");

    cmd.assert()
        .code(2)
        .stderr(predicate::str::contains("No such file or directory"));
}

#[test]
fn test_match_find_single_file() {
    let mut cmd = run("test-files/main.c", "printf()");

    cmd.assert()
        .code(0)
        .stdout(predicate::str::is_match("^\\[.*test-files/main.c:4]     printf\\(\"Hello %s!\\\\n\", \"world\"\\);
$").unwrap());
}

#[test]
fn test_multiple_match_single_file() {
    let mut cmd = run("test-files/injection.php", "mysqli->real_escape_string");

    cmd.assert()
        .code(0)
        .stdout(predicate::str::is_match(
"^\\[[^]]*test-files/injection.php:4]                   \\$mysqli->real_escape_string\\(\\$username\\),
\\[[^]]*test-files/injection.php:5]                   \\$mysqli->real_escape_string\\(\\$password\\)\\);
$").unwrap());
}

#[test]
fn test_no_match_single_file() {
    let mut cmd = run("test-files/injection.php", "no match");

    cmd.assert()
        .code(1)
        .stdout(predicate::str::is_match("^$").unwrap());
}

#[test]
fn test_multiline_match_single_file() {
    let mut cmd = run("test-files/main.c", "main() {}");

    cmd.assert()
        .code(0)
        .stdout(predicate::str::is_match(
"^\\[.*test-files/main.c:3-6]
int main\\(\\) \\{
    printf\\(\"Hello %s!\\\\n\", \"world\"\\);
    return 0;
}
$").unwrap());
}
