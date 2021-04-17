mod graphical;

use std::{collections::HashSet, ffi::OsString, fs, iter, path::{Path, PathBuf}};
use iced::{Application, Settings};
use structopt::StructOpt;
use teardown_bin_format::{EntityKind, parse_file};
use teardown_editor_format::{VoxStore, SceneWriterBuilder};
use anyhow::{Context, Result};
use thiserror::Error;
use steamy_vdf as vdf;
use Error::UnexpectedVDF as VDFErr;

#[derive(Debug, Error)]
enum Error {
    #[cfg(target_os = "windows")]
    #[error("Unable to get the path to the main Steam directory from the registry")]
    SteamPathInRegistry,
    #[error("Steam dir not found at {0}")]
    NoMainSteamDir(PathBuf),
    #[error("Steam app {0} not found")]
    SteamAppNotFound(String),
    #[error("No home directory found")]
    NoHomeDir,
    #[error("Unexpected type/value when reading a Valve KeyValues file (sometimes .vdf)")]
    UnexpectedVDF
}

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
    command: Option<Subcommand>
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Options::from_args();
    if let Some(command) = args.command {
        match command {
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
                SceneWriterBuilder::default()
                    .vox_store(VoxStore::new(teardown_folder).unwrap())
                    .mod_dir(mod_folder)
                    .name(level_name)
                    .scene(&scene).build().unwrap().write_scene().unwrap();
            }
            Subcommand::ConvertAll { teardown_folder, mods_folder } => {
                let data_folder = teardown_folder.join("data");
                let mut created_mods = HashSet::new();
                let vox_store = VoxStore::new(teardown_folder).unwrap();
                let mut sound_files = HashSet::new();
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
                    let &level_id = registry.get("game.levelid").expect("levels should have game.levelid registry entry");
                    if level_id.is_empty() { continue; }
                    let mod_dir = mods_folder.join(level_name);
                    if !created_mods.insert(level_name.to_owned()) {
                        continue
                    }
                    for entity in scene.iter_entities() {
                        if let EntityKind::Light(light) = &entity.kind {
                            sound_files.insert(light.sound.path.to_owned() + " " + &light.sound.volume.to_string());
                            // println!("### {}", entity.desc);
                            // let materials = &scene.palettes[shape.palette as usize].materials;
                            // for material in materials.iter() {
                            //     match material.kind {
                            //         teardown_bin_format::MaterialKind::None => {}
                            //         kind => {
                            //             print!("{:?}:{:?} ", kind, material.replacable);
                            //         }
                            //     }
                            // }
                            // println!("")
                        }
                    }
                    SceneWriterBuilder::default()
                        .vox_store(vox_store.clone())
                        .mod_dir(mod_dir)
                        .scene(&scene).build().unwrap().write_scene().unwrap();
                    
                }
                // println!("{:?}", sound_files);
                for mod_ in &created_mods {
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
            }
        }
    } else {
        graphical::App::run(Settings::default())?;
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn get_steam_dir_path_through_registry() -> Result<PathBuf> {
    let registry_key = registry::Hive::CurrentUser.open(r"Software\Valve\Steam", registry::Security::Read).context(Error::SteamPathInRegistry)?;
    let path = registry_key.value("SteamPath").context(Error::SteamPathInRegistry)?;
    if let registry::Data::String(path) = path {
        Ok(path.to_os_string().into())
    } else {
        Err(anyhow::format_err!("Invalid registry data type")).context(Error::SteamPathInRegistry)
    }
}

#[cfg(target_os = "windows")]
fn get_steam_dir_path() -> Result<PathBuf> {
    get_steam_dir_path_through_registry().or_else(|err| {
        eprintln!("{}", err);
        Ok(r"C:\Program Files (x86)\Steam\".into())
    })
}

#[cfg(not(target_os = "windows"))]
fn get_steam_dir_path() -> Result<PathBuf> {
    let home_dir = dirs::home_dir().ok_or(Error::NoHomeDir)?;
    Ok(home_dir.join(".local/share/Steam"))
}

fn get_steam_dir() -> Result<PathBuf> {
    let path = get_steam_dir_path()?;
    if path.exists() {
        Ok(path)
    } else {
        Err(Error::NoMainSteamDir(path).into())
    }
}

fn get_extra_library_dirs(main_dir: &Path) -> Result<Vec<PathBuf>> {
    let library_dirs_path = main_dir.join(["steamapps", "libraryfolders.vdf"].iter().collect::<PathBuf>());
    let vdf = steamy_vdf::load(&library_dirs_path)?;
    let entries = vdf
        .as_table().context(VDFErr)?.get("LibraryFolders").context(VDFErr)?
        .as_table().context(VDFErr)?;
    let mut libraries = Vec::new();
    for (key, value) in entries.iter() {
        if key.parse::<i32>().is_ok() { libraries.push(value.as_str().context(VDFErr)?.into()) }
    }
    Ok(libraries)
}

fn get_steam_library_dirs() -> Result<Vec<PathBuf>> {
    let main_dir = get_steam_dir()?;
    let extras = get_extra_library_dirs(&main_dir)
        .context("Could not get extra library folders")
        .unwrap_or_else(|err| {
            eprintln!("{}", err);
            Vec::new()
        });
    Ok(iter::once(main_dir).chain(extras).collect())
}

struct SteamApp {
    manifest: vdf::Table,
    library_path: PathBuf
}

impl SteamApp {
    fn compat_drive(&self) -> Option<PathBuf> {
        if let Some(user_config) = self.manifest.get("UserConfig") {
            if let Some(override_dest) = user_config.as_table()?.get("platform_override_dest") {
                if override_dest.as_str()? == "linux" {
                    return Some(self.library_path.join(["steamapps", "compatdata", self.manifest.get("appid")?.as_str()?, "pfx", "drive_c"].iter().collect::<PathBuf>()))
                }
            }
        }
        None
    }

    fn install_dir(&self) -> PathBuf {
        self.library_path.join("steamapps").join("common").join(&self.manifest.get("installdir").unwrap().as_str().unwrap())
    }

    fn user_dir(&self) -> PathBuf {
        self.compat_drive().map_or_else(|| dirs::home_dir().expect("dirs::home_dir"), |compat_drive| {
            compat_drive.join("users").join("steamuser")
        })
    }
}

fn find_steam_app(app_id: &str) -> Result<SteamApp> {
    let library_dirs = get_steam_library_dirs()?;
    let manifest_file_name: OsString = format!("appmanifest_{}.acf", app_id).into();
    for library_dir in library_dirs {
        for dir_entry in library_dir.join("steamapps").read_dir()? {
            let dir_entry = dir_entry?;
            if dir_entry.file_name() == manifest_file_name {
                let manifest = steamy_vdf::load(dir_entry.path())?
                    .as_table().context(VDFErr)?.get("AppState").context(VDFErr)?
                    .as_table().context(VDFErr)?.to_owned();
                return Ok(SteamApp { library_path: library_dir, manifest })
            }
        }
    }
    Err(Error::SteamAppNotFound(app_id.to_owned()).into())
}

#[derive(Debug, Clone)]
struct Directories {
    mods: PathBuf,
    progress: PathBuf,
    main: PathBuf
}

fn find_teardown_dirs() -> Result<Directories> {
    let steam_app = find_steam_app("1167630")?;
    let user_dir = steam_app.user_dir();
    Ok(Directories {
        mods: user_dir.join("My Documents").join("Teardown").join("mods"),
        progress: user_dir.join("Local Settings").join("Application Data").join("Teardown"),
        main: steam_app.install_dir(),
    })
}
