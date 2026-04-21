// Copyright 2026 KIM Hyunjae
//
// This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.

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
