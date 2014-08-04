/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use syntax::ast;

use rustc::lint::{LintPass, LintArray, Context};
use rustc::middle::{ty, def};

declare_lint!(UNROOTED_JS_MANAGED, Deny,
    "warn about unrooted JS-managed pointers")

pub struct UnrootedJSManaged;

impl LintPass for UnrootedJSManaged {
    fn get_lints(&self) -> LintArray {
        lint_array!(UNROOTED_JS_MANAGED)
    }

    fn check_ty(&mut self, cx: &Context, ty: &ast::Ty) {
        match ty.node {
            ast::TyPath(_, _, id) => {
                match cx.tcx.def_map.borrow().get_copy(&id) {
                    def::DefTy(def_id) => {
                        if ty::has_attr(cx.tcx, def_id, "unrooted_js_managed") {
                            cx.span_lint(UNROOTED_JS_MANAGED, ty.span,
                                "unrooted JS<T> is not allowed here");
                        }
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    }
}
