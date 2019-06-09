// Copyright 2019 Matthias Krüger. See the COPYRIGHT
// file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[path = "../src/test_helpers.rs"]
mod test_helpers;

use std::fs::OpenOptions;
use std::io::prelude::*;
use std::path::PathBuf;
use std::process::Command;

use crate::test_helpers::bin_path;
use regex::Regex;

#[allow(non_snake_case)]
#[test]
fn alternative_registry_works() {
    // make sure alternative registries work

    // create a CARGO_HOME with a config file

    let cargo_home = "target/alt_registries_CARGO_HOME/";
    std::fs::create_dir_all(cargo_home).unwrap();
    let cargo_home_path = cargo_home.split('/').collect::<PathBuf>();
    let mut cargo_config_file_path = cargo_home_path.clone(); // target/alt_registries_CARGO_HOME/config
    cargo_config_file_path.push("config");
    println!("cargo config file path: {:?}", cargo_config_file_path);
    // create the config file
    //  std::fs::File::create(&cargo_config_file_path).expect("failed to create cargo_config_file in cargo home");

    // clone the crates io index
    if !String::from("target/my-index")
        .split('/')
        .collect::<PathBuf>()
        .exists()
    {
        println!("cloning registry index into target/my-index");
        let git_clone_cmd = Command::new("git")
            .arg("clone")
            .arg("https://github.com/rust-lang/crates.io-index")
            //.arg("--depth=5")
            .arg("--quiet")
            .arg("my-index")
            .current_dir("target/")
            .output();
        // located at target/my-index
        let status = git_clone_cmd.unwrap();
        let stderr = String::from_utf8_lossy(&status.stderr).to_string();
        let stdout = String::from_utf8_lossy(&status.stdout).to_string();

        if !stderr.is_empty() {
            println!("error while git cloning");
            println!("stderr:\n{:?}", stderr);
            println!("stdout:\n{:?}", stdout);
            println!("status: {:?}", status);
            panic!("error while git cloning")
        }

        println!("ERR {:?}", stderr);
        println!("OUT {:?}", stdout);
    }

    let my_registry_path = "target/my-index".split('/').collect::<PathBuf>();
    let _my_registry_path_absolute =
        std::fs::canonicalize(&my_registry_path).expect("could not canonicalize path");

    // write the ${CARGO_HOME}/config with info on where to find the alt registry
    let mut config_file = std::fs::File::create(&cargo_config_file_path).unwrap();

    // on windows, there will be an extended length path here
    // \\\\?\\C:\\Users\\travis\\build\\matthiaskrgr\\cargo-cache\\target\\alt_registries_CARGO_HOME\\config
    // but the "?" causes a parsing error:
    // "error: could not load Cargo configuration\n\nCaused by:\n  could not parse TOML configuration in `\\\\?\\C:\\Users\\travis\\build\\matthiaskrgr\\cargo-cache\\target\\alt_registries_CARGO_HOME\\config`\n\nCaused by:\n  could not parse input as TOML\n\nCaused by:\n  invalid escape character in string: `C` at line 2\n"
    let absolute_path = std::env::current_dir().unwrap();
    let path = absolute_path.join("target/my-index".split('/').collect::<PathBuf>());

    let index_path: String = if cfg!(windows) {
        /*     let mut s = String::from("file:///");
        s.push_str(&path.display().to_string());
        s*/
        String::from("file://C:/Users/travis/build/matthiaskrgr/cargo-cache/target/my-index")
    } else {
        let mut s = String::from("file://");
        s.push_str(&path.display().to_string());
        s
    };

    let config_text: &str = &format!(
        "[registries]
my-index = {{ index = '{}' }}\n",
        index_path
    );

    println!("config text:\n\n{}\n\n", config_text);

    config_file.write_all(config_text.as_bytes()).unwrap();

    let project_path = "target/test_crate".split('/').collect::<PathBuf>();;
    println!("creating dummy project dir: {:?}", project_path);
    if !project_path.exists() {
        let cargo_new_cmd = Command::new("cargo")
            .arg("new")
            .arg("--quiet")
            .arg(project_path.display().to_string())
            .output();

        let status = cargo_new_cmd.unwrap();
        let stderr = String::from_utf8_lossy(&status.stderr).to_string();
        let stdout = String::from_utf8_lossy(&status.stdout).to_string();

        if !stderr.is_empty() {
            println!("error while git cloning");
            println!("stderr:\n{:?}", stderr);
            println!("stdout:\n{:?}", stdout);
            println!("status: {:?}", status);
            panic!("error while cargo new dummy crate");
        }
        println!("ERR {:?}", stderr);
        println!("OUT {:?}", stdout);
    }

    let cargo_toml = "target/test_crate/Cargo.toml"
        .split('/')
        .collect::<PathBuf>();

    let mut file = OpenOptions::new().append(true).open(&cargo_toml).unwrap();

    if !std::fs::read_to_string(&cargo_toml)
        .unwrap()
        .contains("regex")
    {
        let additionl_cargo_toml_text = String::from(
            "regex = \"*\"
rayon = { version = \"1\", registry = \"my-index\" }\n",
        );
        for line in additionl_cargo_toml_text.lines() {
            writeln!(file, "{}", line).unwrap();
        }
    }

    // build the crate
    let mut testcrate_path = cargo_toml.clone();
    let _ = testcrate_path.pop();

    let absolute_path = std::env::current_dir().unwrap();
    let cargo_h_path = absolute_path
        .join("target")
        .join("alt_registries_CARGO_HOME");

    println!("cargo home path: {:?}", cargo_h_path);
    let build_cmd = Command::new("cargo")
        .arg("check")
        .current_dir(&testcrate_path)
        .env("CARGO_HOME", cargo_h_path.display().to_string())
        .output()
        .unwrap();

    let status = build_cmd.status;
    let stderr = String::from_utf8_lossy(&build_cmd.stderr).to_string();
    let stdout = String::from_utf8_lossy(&build_cmd.stdout).to_string();

    // @TODO handle all  command::new() calls that way!
    if !build_cmd.status.success() {
        println!("error while cargo building test crate");
        println!("stderr:\n{:?}", stderr);
        println!("stdout:\n{:?}", stdout);
        println!("status: {:?}", status);
        panic!("error while building test crate");
    }

    println!("ERR {:?}", stderr);
    println!("OUT {:?}", stdout);

    // run cargo cache
    let cargo_cache_cmd = Command::new(bin_path())
        .env("CARGO_HOME", cargo_h_path.display().to_string())
        .output()
        .unwrap();

    if !cargo_cache_cmd.status.success() {
        println!("error running cargo-cache on alt reg $CARGO_HOME");
        println!("stderr:\n{:?}", stderr);
        println!("stdout:\n{:?}", stdout);
        println!("status: {:?}", status);
        panic!("error while running cargo-home with alt regs");
    }

    let stdout = String::from_utf8_lossy(&cargo_cache_cmd.stdout).to_string();

    println!("{}", stdout);
    // check if the output is what we expect

    let mut desired_output = String::from("Cargo cache .*target.*alt_registries_CARGO_HOME.*\n\n");

    /*
    Cargo cache '/home/matthias/vcs/github/cargo-cache/target/alt_registries_CARGO_HOME':

    Total size:                             218.89 MB
    Size of 0 installed binaries:             0 B
    Size of registry:                         218.89 MB
    Size of registry index:                     211.24 MB
    Size of 22 crate archives:                  1.39 MB
    Size of 22 crate source checkouts:          6.25 MB
    Size of git db:                           0 B
    Size of 0 bare git repos:                   0 B
    Size of 0 git repo checkouts:               0 B
    */

    desired_output.push_str(
        "Total size:          .*MB
Size of 0 installed binaries: * 0 B
Size of registry:           .*MB
Size of registry index:       .*MB
Size of .. crate archives:       .*MB
Size of .. crate source checkouts:  .*MB
Size of git db:             * 0 B
Size of .* bare git repos:  * 0 B
Size of .* git repo checkouts: * 0 B",
    );

    let regex = Regex::new(&desired_output).unwrap();

    assert!(
        regex.clone().is_match(&stdout),
        "regex: {:?}, cc_output: {}",
        regex,
        stdout
    );
}