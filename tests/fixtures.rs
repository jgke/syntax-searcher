use assert_cmd::prelude::*;
use std::path::PathBuf;
use std::process::Command;
use test_generator::test_resources;

// for f in test-files/hello/*; do syns --no-color -o '\.' $f > tests/.fixtures/nocolor/$(basename $f); done
#[test_resources("test-files/hello/*")]
fn hello_world_langs_nocolor(file: &str) {
    let mut cmd = Command::cargo_bin("syns").unwrap();
    cmd.arg("--no-color").arg("-o").arg("\\.").arg(file);

    let filename = file.split('/').next_back().unwrap();
    let mut expected_output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    expected_output_path.push("tests/.fixtures/nocolor");
    expected_output_path.push(filename);

    let expected_output = std::fs::read(expected_output_path).unwrap();

    cmd.assert().code(0).stdout(expected_output);
}

// for f in test-files/hello/*; do syns --color '\.' $f > tests/.fixtures/color/$(basename $f); done
#[test_resources("test-files/hello/*")]
fn hello_world_langs_color(file: &str) {
    let mut cmd = Command::cargo_bin("syns").unwrap();
    cmd.arg("--color").arg("\\.").arg(file);

    let filename = file.split('/').next_back().unwrap();
    let mut expected_output_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    expected_output_path.push("tests/.fixtures/color");
    expected_output_path.push(filename);

    let expected_output = std::fs::read(expected_output_path).unwrap();

    cmd.assert().code(0).stdout(expected_output);
}
