#![feature(
    array_chunks,
    bool_to_option,
    optimize_attribute
)]
use std::{
    error::Error as StdError,
    fs::File,
    io::{self, Cursor, Read},
    path::Path,
};

use anyhow::Result;
use flate2::read::ZlibDecoder;
use owning_ref::OwningHandle;
use structr::{get_end_path, write_debug_json, Parse, ParseError, Parser};
use thiserror::Error;

mod format;
#[cfg(feature = "mesh")]
mod mesh;
pub use format::*;

#[derive(Debug, Error)]
enum Error {
    #[error(".vox error")]
    IO(#[from] io::Error),
}

fn read_bytes<P: AsRef<Path>>(path: P) -> Result<Vec<u8>, io::Error> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn decompress_if_needed(bytes: Vec<u8>) -> Result<Vec<u8>, io::Error> {
    Ok(if bytes.starts_with(Scene::MAGIC) {
        bytes
    } else {
        let mut new_bytes = Vec::with_capacity(bytes.len());
        ZlibDecoder::new(Cursor::new(bytes)).read_to_end(&mut new_bytes)/*.map_err(|err| format!("Decompressing after magic mismatch: {:?}", err))*/?;
        new_bytes
    })
}

pub fn read_to_uncompressed<P: AsRef<Path>>(path: P) -> Result<Vec<u8>, io::Error> {
    decompress_if_needed(read_bytes(path)?)
}

pub fn parse_uncompressed(bytes: &[u8]) -> Result<Scene<'_>, ParseError<'_>> {
    Scene::parse(&mut Parser::new(bytes))
}

pub type OwnedScene = OwningHandle<Vec<u8>, Box<Scene<'static>>>;

pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<OwnedScene> {
    let uncompressed = read_to_uncompressed(path)?;
    OwningHandle::try_new(uncompressed, |uncompressed_ref| {
        // Safety: I have no idea.
        unsafe { Ok(Box::new(parse_uncompressed(&*uncompressed_ref)?)) }
    })
}

pub fn test_file<P: AsRef<Path>>(path: P, debug: bool) -> Result<(), Box<dyn StdError>> {
    let uncompressed = read_to_uncompressed(path)?;
    let mut parser = Parser::new(&uncompressed);
    let _scene = match Scene::parse(&mut parser) {
        Ok(ok) => ok,
        Err(err) => {
            println!("Error: {:?}", err.kind);
            if debug {
                write_debug_json(&parser.context)?;
            }
            let end_path = get_end_path(&parser.context);
            for element in end_path {
                print!(".{}", element);
            }
            println!();
            Err(err).map_err(|err| format!("{:?}", err))?
        }
    };
    if debug {
        write_debug_json(&parser.context)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub struct PaletteIndex(pub u8, bool);
