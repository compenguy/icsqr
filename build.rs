// SPDX-License-Identifier: CC0-1.0
// SPDX-FileCopyrightText: none
// Compile the Slint UI markup into Rust code before the main crate builds.
fn main() {
    slint_build::compile("ui/app.slint").expect("Slint build failed");
}
