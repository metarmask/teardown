use std::{collections::HashSet, fs::{self, File}, io::Read, path::PathBuf};

use structopt::StructOpt;
use teardown_bin_format::{EntityKind, parse_file};
use teardown_editor_format::{VoxStore, write_scene};

#[derive(StructOpt)]
enum Subcommand {
    ShowVox { vox_file: PathBuf },
    PrintEnv { bin_file: PathBuf },
    Convert { bin_file: PathBuf, teardown_folder: PathBuf, mod_folder: PathBuf, level_name: String },
    ConvertAll { teardown_folder: PathBuf, mods_folder: PathBuf }
}

#[derive(StructOpt)]
struct Options {
    #[structopt(subcommand)]
    command: Subcommand
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Options::from_args();
    match args.command {
        Subcommand::ShowVox { vox_file } => {
            let semantic = vox::syntax::parse_file(vox_file);
            println!("{:?}", semantic);
            // use std::convert::TryFrom;
            // let mut vox_fs_file = File::open(vox_file)?;
            // let mut vox_file_bytes = Vec::new();
            // vox_fs_file.read_to_end(&mut vox_file_bytes)?;
            // let (_, syntaxical_vox_file) = vox::syntax::VoxFile::parse_flat(&vox_file_bytes).unwrap();
            // let semantic_vox_file = vox::semantic::VoxFile::try_from(syntaxical_vox_file)?;
            // println!("{:#?}", semantic_vox_file);
        }
        Subcommand::Convert { bin_file, teardown_folder, mod_folder, level_name } => {
            let scene = parse_file(bin_file)?;
            write_scene(&scene, teardown_folder, mod_folder, &level_name, &mut VoxStore::default())?;
        }
        Subcommand::ConvertAll { teardown_folder, mods_folder } => {
            let data_folder = teardown_folder.join("data");
            let mut created_mods = HashSet::new();
            let mut vox_store = Default::default();
            for file in fs::read_dir(data_folder.join("bin"))? {
                let file = file?;
                println!("Reading {}", file.file_name().to_string_lossy());
                let scene = parse_file(file.path())?;
                let registry = &scene.registry.0;
                let mut level_path = PathBuf::from(registry.get("game.levelpath").expect("levels should have game.levelpath registry entry"));
                level_path.set_extension("");
                // Example: lee
                let level_name = level_path.file_name().unwrap();
                // Example: lee_tower
                let level_id = registry.get("game.levelid").expect("levels should have game.levelid registry entry");
                if *level_id == "" { continue; }
                let mod_dir = mods_folder.join(level_name);
                if !created_mods.insert(level_name.to_owned()) {
                    // continue
                }
                write_scene(&scene, &teardown_folder, &mod_dir, level_id, &mut vox_store)?;
            }
            for mod_ in created_mods.iter() {
                fs::write(mods_folder.join(mod_).join("main.xml"), "")?;
                fs::write(mods_folder.join(mod_).join("info.txt"), format!(
"name = {}
author = Tuxedo Labs
description = ", mod_.to_string_lossy()))?;
            }
        }
        Subcommand::PrintEnv { bin_file: path } => {
            let scene = parse_file(path)?;
            println!("{:#?}", scene.environment);
            // for entity in scene.iter_entities() {
            //     if let EntityKind::Script(script) = &entity.kind {
            //         println!("{:?}", script.params);
            //     }
            // }
        }
    }
    Ok(())
}