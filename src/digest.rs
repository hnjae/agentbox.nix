// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use sha2::{Digest, Sha256};

pub(crate) fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    hex_lower(sha256_bytes(bytes))
}

pub(crate) fn hex_prefix(bytes: impl AsRef<[u8]>, len: usize) -> String {
    let mut output = hex_lower(bytes);
    debug_assert!(len <= output.len());
    output.truncate(len);
    output
}

pub(crate) fn hex_lower(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
