use assert_cmd::prelude::*; // Add methods on commands
use predicates::prelude::*; // Used for writing assertions
use std::path::PathBuf;
use std::process::Command; // Run programs
use regex::Regex;

fn run(path: &str, query: &str) -> Command {
    let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    d.push(path);

    let mut cmd = Command::cargo_bin("syns").unwrap();
    cmd.arg("--no-color").arg(query).arg(d);
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

    cmd.assert().code(0).stdout(
        predicate::str::is_match(
            "^\\[.*test-files/main.c:4]     printf\\(\"Hello %s!\\\\n\", \"world\"\\);
$",
        )
        .unwrap(),
    );
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

    cmd.assert().code(0).stdout(
        predicate::str::is_match(
            "^\\[.*test-files/main.c:3-6]
int main\\(\\) \\{
    printf\\(\"Hello %s!\\\\n\", \"world\"\\);
    return 0;
}
$",
        )
        .unwrap(),
    );
}

#[test]
fn test_match_group_end() {
    let mut cmd = run("test-files/main.c", "printf(\\.,\\.\\$)");

    cmd.assert().code(0).stdout(
        predicate::str::is_match(
            "^\\[.*test-files/main.c:4]     printf\\(\"Hello %s!\\\\n\", \"world\"\\);
$",
        )
        .unwrap(),
    );
}

#[test]
fn test_not_match_group_end() {
    let mut cmd = run("test-files/main.c", "printf(\\.,\\$)");

    cmd.assert()
        .code(1)
        .stdout(predicate::str::is_match("^$").unwrap());
}

#[test]
fn test_multiple_match_multiple_files() {
    let mut cmd = run("test-files", "\"Hello world!\"");

    let value = cmd.assert()
        .code(0)
        .get_output()
        .clone();


    let r = Regex::new(r"\[.*test-files").unwrap();
    let raw_string = String::from_utf8(value.stdout).unwrap();
    let lines = raw_string.lines()
        .map(|line| r.replace_all(line, "[test-files").to_string())
        .collect::<Vec<String>>();

    assert_eq!(lines.len(), 10);

    let expected_output = r#"
[test-files/main.py:3]     print('Hello world!')
[test-files/hello/elixir.ex:3]     IO.puts "Hello world!"
[test-files/hello/vb.vb:5]     Console.WriteLine("Hello world!")
[test-files/hello/csharp.cs:7]             System.Console.WriteLine("Hello world!");
[test-files/hello/haskell.hs:2] main = putStrLn "Hello world!"
[test-files/hello/javascript.js:1] console.log("Hello world!")
[test-files/hello/python.py:2]     print("Hello world!")
[test-files/hello/clojure.clj:2]   (println "Hello world!"))
[test-files/hello/rust.rs:2]    println!("Hello world!");
[test-files/hello/java.java:5]         System.out.println("Hello world!");"#;


    for line in &lines {
        dbg!(&line);
    }
    for line in expected_output.lines() {
        dbg!(&line);
        if !line.is_empty() {
            assert!(lines.contains(&line.to_string()));
        }
    }
}

#[test]
fn test_multiple_match_filename_only() {
    let mut cmd = run("test-files", "\"Hello world!\"");
    cmd.arg("-l");

    let value = cmd.assert()
        .code(0)
        .get_output()
        .clone();


    let r = Regex::new(r".*test-files").unwrap();
    let raw_string = String::from_utf8(value.stdout).unwrap();
    let lines = raw_string.lines()
        .map(|line| r.replace_all(line, "test-files").to_string())
        .collect::<Vec<String>>();

    assert_eq!(lines.len(), 10);

    let expected_output = r#"
test-files/main.py
test-files/hello/elixir.ex
test-files/hello/vb.vb
test-files/hello/csharp.cs
test-files/hello/haskell.hs
test-files/hello/javascript.js
test-files/hello/python.py
test-files/hello/clojure.clj
test-files/hello/rust.rs
test-files/hello/java.java"#;


    for line in &lines {
        dbg!(&line);
    }
    for line in expected_output.lines() {
        dbg!(&line);
        if !line.is_empty() {
            assert!(lines.contains(&line.to_string()));
        }
    }
}
