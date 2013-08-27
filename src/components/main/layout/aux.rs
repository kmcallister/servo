/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Code for managing the layout data in the DOM.

use script::dom::node::{AbstractNode, LayoutView, LayoutData};
use servo_util::tree::TreeNodeRef;

use extra::arc::RWArc;

/// Functionality useful for querying the layout-specific data on DOM nodes.
pub trait LayoutAuxMethods {
    fn initialize_layout_data(self) -> Option<RWArc<LayoutData>>;
    fn initialize_style_for_subtree(self, refs: &mut ~[RWArc<LayoutData>]);
}

impl LayoutAuxMethods for AbstractNode<LayoutView> {
    /// If none exists, creates empty layout data for the node (the reader-auxiliary
    /// box in the COW model) and populates it with an empty style object.
    fn initialize_layout_data(self) -> Option<RWArc<LayoutData>> {
        if self.has_layout_data() {
            do self.write_layout_data |data| {
                data.boxes.display_list = None;
                data.boxes.range = None;
            }
            None
        } else {
            let data = RWArc::new(LayoutData::new());
            self.set_layout_data(data.clone());
            Some(data)
        }
    }

    /// Initializes layout data and styles for a Node tree, if any nodes do not have
    /// this data already. Append created layout data to the task's GC roots.
    fn initialize_style_for_subtree(self, refs: &mut ~[RWArc<LayoutData>]) {
        let _ = for n in self.traverse_preorder() {
            match n.initialize_layout_data() {
                Some(r) => refs.push(r),
                None => {}
            }
        };
    }
}

