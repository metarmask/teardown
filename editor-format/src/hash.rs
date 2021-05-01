use std::{
    collections::hash_map::DefaultHasher,
    convert::TryInto,
    hash::{Hash, Hasher},
};

const BASE64_CONFIG: base64::Config = base64::Config::new(base64::CharacterSet::UrlSafe, false);

pub fn compute_n<H: Hash>(to_hash: &H) -> u64 {
    let mut hasher = DefaultHasher::new();
    to_hash.hash(&mut hasher);
    hasher.finish()
}

pub fn n_to_str(n: u64) -> String {
    base64::encode_config(n.to_le_bytes(), BASE64_CONFIG)
}

pub fn str_to_n(string: &str) -> Result<u64, ()> {
    match base64::decode_config(string, BASE64_CONFIG) {
        Ok(bytes) => Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| ())?)),
        Err(_) => Err(()),
    }
}
