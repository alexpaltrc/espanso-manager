/*
 * This file is part of EspansoManager.
 *
 * Copyright (C) 2026 Alex Palacios
 *
 * EspansoManager is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * EspansoManager is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with EspansoManager.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Stamps the Windows version resource onto the executable. That is all this does.
//!
//! Without it the binary reports **nothing** — no company, no product, no description, no
//! version. Measured on the 0.0.1 build: every one of those fields came back empty, while
//! `espansod.exe` sitting next to it in the same folder reports a product and a version. An
//! executable that identifies itself is easier to recognise in Task Manager, and a wholly
//! anonymous unsigned binary scores worse with antivirus heuristics than a described one.
//!
//! **This is not a signature and must not be described as one.** Anyone can write any string
//! here; nothing below is verified by anybody. A real Authenticode signature is a separate
//! problem with a separate cost, and this file makes no progress on it.
//!
//! A failure here is a warning, not an error, and that is deliberate: the resource is cosmetic,
//! and a machine without the Windows SDK should still be able to build a working app. The
//! warning is loud so that the omission cannot pass unnoticed.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // The crate is Windows-only, but a build script still runs wherever cargo runs.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let mut res = winresource::WindowsResource::new();

    // Set explicitly rather than left to default: winresource would otherwise take the crate
    // name, `espanso_manager`, which is not what this program is called anywhere a user looks.
    res.set("ProductName", "EspansoManager");
    res.set("FileDescription", "EspansoManager");
    res.set("InternalName", "EspansoManager");
    res.set("OriginalFilename", "EspansoManager.exe");
    res.set("Comments", "Desktop manager for espanso text expansions. Portable.");

    // Alex Palacios has no company, and inventing one would put a false statement in the single
    // field an antivirus vendor might actually read. His own name is the true answer.
    res.set("CompanyName", "Alex Palacios");

    // ASCII only, on purpose: these strings go through `rc.exe`, where a stray non-ASCII byte
    // fails in ways that are tedious to diagnose for text nobody reads closely.
    //
    // FileVersion and ProductVersion are deliberately NOT set here. winresource derives both
    // from `version` in Cargo.toml, so 0.0.1 keeps living in exactly one place.

    if let Err(e) = res.compile() {
        println!("cargo:warning=version resource not embedded ({e}). The build is fine and the app will run, but the exe will report no company, product or version. This needs rc.exe from the Windows SDK.");
    }
}
