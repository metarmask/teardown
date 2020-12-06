#![allow(dead_code)]
use std::{collections::HashSet, fs::File, io::Read, path::{Path, PathBuf}};

use teardown_bin_format::{compute_hash_str, EntityKind};
use vox::syntax::{ChunkKind, Dict, VoxFile};

fn read_file_bytes<P: AsRef<Path>>(path: P) -> Vec<u8> {
    let mut file = File::open(path).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    bytes
}

fn load_vox_file<P: AsRef<Path>>(path: P) -> VoxFile {
    let bytes = read_file_bytes(path);
    let (_, vox_file) = VoxFile::parse_flat(&bytes).unwrap();
    vox_file
}

fn get_folder_of_this_file() -> PathBuf {
    // This file from workspace
    let mut path = PathBuf::from(file!());
    // The folder of this file, from workspace
    path.pop();
    // Absolute path to crate
    let mut current_dir = std::env::current_dir().unwrap();
    // Absolute path to workspace
    current_dir.pop();
    // Absolute path to the folder of this file
    current_dir.join(path)
}

fn into_vox_materials(vox_file: VoxFile) -> Vec<Dict> {
    let mut materials = Vec::new();
    for child in vox_file.main_chunk.children {
        match child.kind {
            ChunkKind::Material(material) => {
                if materials.len() == 0 && material.id == 1 {
                    materials.push(Dict::new())
                }
                materials.push(material.props);
            }
            _ => {}
        }
    }
    materials
}

// Filmic tone mapping is turned off in asset pack assets, which explains
// differences in appearance
fn material_test(vox: &str, bin: &str) {
    let folder = get_folder_of_this_file();
    let bytes =  read_file_bytes(folder.join(bin));
    let scene = teardown_bin_format::parse_uncompressed(&bytes).unwrap();
    let shape = scene.iter_entities().find_map(|entity| match &entity.kind { EntityKind::Shape(shape) => {
        Some(shape)
    }, _ => None }).unwrap();
    let palette = &scene.palettes[shape.palette as usize];
    let palette_hash_str = compute_hash_str(&palette.materials);
    teardown_editor_format::write_scene(&scene, folder.join("."), "mod").unwrap();
    let og_vox = into_vox_materials(load_vox_file(folder.join(vox)));
    let created_vox = into_vox_materials(load_vox_file(folder.join(format!("./mod/{}.vox", palette_hash_str))));
    let mut used_indices = HashSet::new();
    for (_, index) in shape.iter_voxels() {
        used_indices.insert(index);
    }
    let mut keys = HashSet::new();
    for key in og_vox.iter().chain(created_vox.iter()).flat_map(|dict| dict.keys()) {
        keys.insert(key);
    }
    let mut used_indices = used_indices.into_iter().collect::<Vec<_>>();
    used_indices.sort();
    for index in used_indices {
        let index = index as usize;
        println!("### index: {}", index);
        println!("{:#?}", palette.materials[index]);
        for key in keys.iter() {
            let og_dict = &og_vox[index];
            let created_dict = &created_vox[index];
            let lr = (og_dict.get(*key), created_dict.get(*key));
            if let (None, None) = lr {
                continue
            }
            print!("{:<8}: ", key);
            match lr {
                (None, None) => {unreachable!()}
                (Some(_), None) | (None, Some(_)) => {
                    println!("{:>15}, {:<15}", format!("{:?}", lr.0), format!("{:?}", lr.1));
                }
                (Some(left), Some(right)) => {
                    if left == right {
                        println!("{:^30}", left);
                    } else {
                        println!("{:>15}, {:<15}", left, right);
                    }
                }
            }
            
        }
        // assert_eq!(og_vox[index], created_vox[index])
    }
    // load_vox_file("mod_folder/mod/")
}

// #[test]
// fn test_container_red() {
//     material_test("container_red.vox", "container_red.bin")
// }
