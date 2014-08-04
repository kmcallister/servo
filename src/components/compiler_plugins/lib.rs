/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

#![crate_name = "compiler_plugins"]
#![crate_type = "dylib"]

#![feature(macro_rules, plugin_registrar, phase)]

extern crate syntax;

#[phase(plugin, link)]
extern crate rustc;

#[cfg(test)]
extern crate sync;

use rustc::plugin::Registry;

mod macros;
mod js_managed_lint;

// NB: This needs to be public or we get a linker error.
#[plugin_registrar]
pub fn plugin_registrar(reg: &mut Registry) {
    reg.register_lint_pass(box js_managed_lint::UnrootedJSManaged);
}
