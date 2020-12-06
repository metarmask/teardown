#![feature(proc_macro_hygiene)]
use command_macros::command;

use std::{ffi::OsString, io, path::PathBuf};
use structopt::StructOpt;
use std::os::unix::fs::symlink;

#[derive(StructOpt)]
enum Subcommand {
    Build,
    Run { args: Vec<OsString> }

}

#[derive(StructOpt)]
struct Options {
    #[structopt(subcommand)]
    command: Subcommand
}

fn get_folder_of_this_file() -> PathBuf {
    let mut path = PathBuf::from(file!());
    path.pop();
    let current_dir = std::env::current_dir().unwrap();
    current_dir.join(path)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Options::from_args();
    let this_folder = get_folder_of_this_file();
    println!("what {:?}", this_folder);
    match args.command {
        Subcommand::Build => {

        }
        Subcommand::Run { args } => {
            let python_addon_folder = this_folder.ancestors().nth(2).unwrap().join("python-addon");
            let run_test = python_addon_folder.join("run_test.py");
            let library_name = "libteardown_import.so";
            let library_path = this_folder.ancestors().nth(3).unwrap().join("target/release").join(library_name);
            command!(cargo build --release -p teardown-blender-import).status().unwrap();
            if let Err(err) = symlink(library_path, python_addon_folder.join("teardown_import").join(library_name)) {
                match err.kind() {
                    io::ErrorKind::AlreadyExists => println!("✓ Symlink exists"),
                    _ => return Err(err.into())
                }
            }
            command!(blender --python (run_test) [ args ]).status().unwrap();
        }
    }
    Ok(())
}