// SPDX-FileCopyrightText: 2026 KIM Hyunjae
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fs::OpenOptions;
use std::io::ErrorKind;

use fd_lock::RwLock;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("expected lock file path argument");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
        .expect("failed to open lock file");
    let mut lock = RwLock::new(file);

    match lock.try_write() {
        Ok(_guard) => println!("released"),
        Err(error) if error.kind() == ErrorKind::WouldBlock => println!("held"),
        Err(error) => panic!("failed to probe lock state: {error}"),
    }
}
