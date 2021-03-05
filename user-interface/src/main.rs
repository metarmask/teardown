use std::{
    fs::File, path::PathBuf,
    io::Read
};

use structopt::StructOpt;
use teardown_bin_format::parse_file;
use teardown_editor_format::write_scene;

#[derive(StructOpt)]
enum Subcommand {
    ShowVox { path: PathBuf },
    PrintEnv { path: PathBuf },
    Convert { path: PathBuf, mod_folder: PathBuf, level_name: String }
}

#[derive(StructOpt)]
struct Options {
    #[structopt(subcommand)]
    command: Subcommand
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Options::from_args();
    match args.command {
        Subcommand::ShowVox { path } => {
            use std::convert::TryFrom;
            let mut vox_fs_file = File::open(path)?;
            let mut vox_file_bytes = Vec::new();
            vox_fs_file.read_to_end(&mut vox_file_bytes)?;
            let (_, syntaxical_vox_file) = vox::syntax::VoxFile::parse_flat(&vox_file_bytes).unwrap();
            let semantic_vox_file = vox::semantic::VoxFile::try_from(syntaxical_vox_file)?;
            println!("{:#?}", semantic_vox_file);
        }
        Subcommand::Convert { path, mod_folder, level_name } => {
            let scene = parse_file(path)?;
            write_scene(&scene, mod_folder, &level_name)?;
        }
        Subcommand::PrintEnv { path } => {
            let scene = parse_file(path)?;
            println!("{:#?}", scene.environment);
        }
    }
    Ok(())
}