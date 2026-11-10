//! Pinned, prepared INT8 assets only. No download or model conversion happens.
use anyhow::{ensure, Result};
use std::path::Path;
use syousetu_translate::storage::sha256_file;

pub const REPOSITORY: &str = "solavr/small100-ctranslate2-int8";
pub const REVISION: &str = "3c5af8e17d5c4f35bda849b7ffceaa1dfc62eb84";
pub const FILES: [(&str, &str); 4] = [
    (
        "model.bin",
        "24e8b51720b7267cfd8faacedce50128bf11c4327f039632a494deb5e1c421f6",
    ),
    (
        "sentencepiece.bpe.model",
        "d8f7c76ed2a5e0822be39f0a4f95a55eb19c78f4593ce609e2edbc2aea4d380a",
    ),
    (
        "shared_vocabulary.json",
        "18916ac68f9ac8fbd5250ca603dcb3861aac8672924d93a8dadc82eb3147f941",
    ),
    (
        "config.json",
        "72901fbd8abd89fb5cf4a388f26fc681f5c4c58a1e1a88b30b879f107270e7ee",
    ),
];

pub fn verify(root: &Path) -> Result<()> {
    for (name, digest) in FILES {
        ensure!(
            sha256_file(&root.join(name))? == digest,
            "model hash mismatch: {name}"
        );
    }
    Ok(())
}
