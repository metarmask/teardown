#![feature(array_map, array_chunks, iterator_fold_self)]
use std::{collections::hash_map::DefaultHasher, error::Error, hash::{Hash, Hasher}, io::{self, Cursor}, path::Path};

use flate2::read::ZlibDecoder;

use owning_ref::OwningHandle;
use structr::{Parse, ParseError, Parser, get_end_path, write_debug_json};

mod format;
pub use format::*;
#[cfg(feature="mesh")]
mod mesh;

use std::{
    fs::File,
    io::Read
};

fn read_bytes<P: AsRef<Path>>(path: P) -> Result<Vec<u8>, io::Error> {
    let mut file = File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn decompress_if_needed(bytes: Vec<u8>) -> Result<Vec<u8>, io::Error> {
    Ok(if bytes.starts_with(&Scene::MAGIC) {
        bytes
    } else {
        let mut new_bytes = Vec::with_capacity(bytes.len());
        ZlibDecoder::new(Cursor::new(bytes)).read_to_end(&mut new_bytes)/*.map_err(|err| format!("Decompressing after magic mismatch: {:?}", err))*/?;
        new_bytes
    })
}

pub fn read_to_uncompressed<P: AsRef<Path>>(path: P) -> Result<Vec<u8>, io::Error> {
    Ok(decompress_if_needed(read_bytes(path)?)?)
}

pub fn parse_uncompressed<'p>(bytes: &'p [u8]) -> Result<Scene<'p>, ParseError<'p>> {
    Ok(Scene::parse(&mut Parser::new(bytes))?)
}

pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<OwningHandle<Vec<u8>, Box<Scene<'static>>>, Box<dyn std::error::Error>> {
    let uncompressed = read_to_uncompressed(path)?;
    OwningHandle::try_new(uncompressed, |uncompressed_ref| {
        // Safety: I have no idea.
        unsafe {
            Ok(Box::new(parse_uncompressed(&*uncompressed_ref)?))
        }
    })
}

pub fn test_file<P: AsRef<Path>>(path: P, debug: bool) -> Result<(), Box<dyn Error>> {
    let uncompressed = read_to_uncompressed(path)?;
    let mut parser = Parser::new(&uncompressed);
    let _scene = match Scene::parse(&mut parser) {
        Ok(ok) => ok, Err(err) => {
            println!("Error: {:?}", err.kind);
            if debug {
                write_debug_json(&parser.context)?;
            }
            let end_path = get_end_path(&parser.context);
            for element in end_path {
                print!(".{}", element);
            }
            println!("");
            Err(err).unwrap()
        }
    };
    Ok(())
}

#[derive(Clone, Copy)]
pub struct PaletteIndex(pub u8);

const BASE64_CONFIG: base64::Config = base64::Config::new(base64::CharacterSet::UrlSafe, false);
pub fn compute_hash_str<H: Hash>(to_hash: &H) -> String {
    let mut hasher = DefaultHasher::new();
    to_hash.hash(&mut hasher);
    let hash_value_bytes = hasher.finish().to_le_bytes();
    base64::encode_config(hash_value_bytes, BASE64_CONFIG)
    // format!("{:016x}", hasher.finish())
}
