/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! The `RenderBox` type, which represents the leaves of the layout tree.

use css::node_style::StyledNode;
use layout::context::LayoutContext;
use layout::display_list_builder::{DisplayListBuilder, ExtraDisplayListData, ToGfxColor};
use layout::float_context::{ClearType, ClearLeft, ClearRight, ClearBoth};
use layout::model::{BoxModel, MaybeAuto};
use layout::text;

use std::cast;
use std::cell::Cell;
use std::cmp::ApproxEq;
use std::managed;
use std::num::Zero;
use geom::{Point2D, Rect, Size2D, SideOffsets2D};
use gfx::display_list::{BaseDisplayItem, BorderDisplayItem, BorderDisplayItemClass};
use gfx::display_list::{DisplayList, ImageDisplayItem, ImageDisplayItemClass};
use gfx::display_list::{SolidColorDisplayItem, SolidColorDisplayItemClass, TextDisplayItem};
use gfx::display_list::{TextDisplayItemClass};
use gfx::font::{FontStyle, FontWeight300};
use gfx::geometry::Au;
use gfx::text::text_run::TextRun;
use newcss::color::rgb;
use newcss::complete::CompleteStyle;
use newcss::units::{Em, Px};
use newcss::units::{Cursive, Fantasy, Monospace, SansSerif, Serif};
use newcss::values::{CSSBorderStyleDashed, CSSBorderStyleSolid};
use newcss::values::{CSSClearNone, CSSClearLeft, CSSClearRight, CSSClearBoth};
use newcss::values::{CSSFontFamilyFamilyName, CSSFontFamilyGenericFamily};
use newcss::values::{CSSFontSizeLength, CSSFontStyleItalic, CSSFontStyleNormal};
use newcss::values::{CSSFontStyleOblique, CSSTextAlign, CSSTextDecoration, CSSLineHeight, CSSVerticalAlign};
use newcss::values::{CSSTextDecorationNone, CSSFloatNone, CSSPositionStatic};
use newcss::values::{CSSDisplayInlineBlock, CSSDisplayInlineTable};
use newcss::values::{CSSLineHeightNormal, CSSLineHeightNumber, CSSLineHeightLength};
use newcss::values::{CSSLineHeightPercentage};
use script::dom::node::{AbstractNode, LayoutView};
use servo_net::image::holder::ImageHolder;
use servo_net::local_image_cache::LocalImageCache;
use servo_util::range::*;
use extra::url::Url;

/// Boxes (`struct Box`) are the leaves of the layout tree. They cannot position themselves. In
/// general, boxes do not have a simple correspondence with CSS boxes in the specification:
///
/// * Several boxes may correspond to the same CSS box or DOM node. For example, a CSS text box
/// broken across two lines is represented by two boxes.
///
/// * Some CSS boxes are not created at all, such as some anonymous block boxes induced by inline
///   boxes with block-level sibling boxes. In that case, Servo uses an `InlineFlow` with
///   `BlockFlow` siblings; the `InlineFlow` is block-level, but not a block container. It is
///   positioned as if it were a block box, but its children are positioned according to inline
///   flow.
///
/// A `GenericBox` is an empty box that contributes only borders, margins, padding, and
/// backgrounds. It is analogous to a CSS nonreplaced content box.
///
/// A box's type influences how its styles are interpreted during layout. For example, replaced
/// content such as images are resized differently from tables, text, or other content. Different
/// types of boxes may also contain custom data; for example, text boxes contain text.
pub trait RenderBox {
    /// Returns the `RenderBoxBase` struct.
    fn base<'a>(&'a self) -> &'a RenderBoxBase;

    /// Returns the `RenderBoxBase` struct.
    fn mut_base<'a>(&'a mut self) -> &'a mut RenderBoxBase;

    /// Returns the class of render box that this is.
    fn class(&self) -> RenderBoxClass;

    /// If this is an image render box, returns the underlying object. Fails otherwise.
    ///
    /// FIXME(pcwalton): Ugly. Replace with a real downcast operation.
    fn as_image_render_box(@mut self) -> @mut ImageRenderBox {
        fail!("as_text_render_box() called on a non-text-render-box")
    }

    /// If this is a text render box, returns the underlying object. Fails otherwise.
    ///
    /// FIXME(pcwalton): Ugly. Replace with a real downcast operation.
    fn as_text_render_box(@mut self) -> @mut TextRenderBox {
        fail!("as_text_render_box() called on a non-text-render-box")
    }

    /// If this is an unscanned text render box, returns the underlying object. Fails otherwise.
    ///
    /// FIXME(pcwalton): Ugly. Replace with a real downcast operation.
    fn as_unscanned_text_render_box(@mut self) -> @mut UnscannedTextRenderBox {
        fail!("as_unscanned_text_render_box() called on a non-unscanned-text-render-box")
    }

    /// Cleans up all memory associated with this render box.
    fn teardown(&self) {}

    /// Returns true if this element is an unscanned text box that consists entirely of whitespace.
    fn is_whitespace_only(&self) -> bool {
        false
    }

    /// Attempts to split this box so that its width is no more than `max_width`. Fails if this box
    /// is an unscanned text box.
    fn split_to_width(@mut self, _: Au, _: bool) -> SplitBoxResult;

    /// Determines whether this box can merge with another box.
    fn can_merge_with_box(&self, other: @mut RenderBox) -> bool {
        false
    }

    /// Returns the *minimum width* and *preferred width* of this render box as defined by CSS 2.1.
    fn minimum_and_preferred_widths(&mut self) -> (Au, Au);

    fn box_height(&mut self) -> Au;

    /// Assigns the appropriate width.
    fn assign_width(&mut self);

    fn debug_str(&self) -> ~str {
        ~"???"
    }
}

impl Clone for @mut RenderBox {
    fn clone(&self) -> @mut RenderBox {
        *self
    }
}

pub trait RenderBoxUtils {
    /// Returns true if this element is replaced content. This is true for images, form elements,
    /// and so on.
    fn is_replaced(self) -> bool;
    
    /// Returns true if this element can be split. This is true for text boxes.
    fn can_split(self) -> bool;
    
    /// Returns the amount of left and right "fringe" used by this box. This is based on margins,
    /// borders, padding, and width.
    fn get_used_width(self) -> (Au, Au);
    
    /// Returns the amount of left and right "fringe" used by this box. This should be based on
    /// margins, borders, padding, and width.
    fn get_used_height(self) -> (Au, Au);

    /// Adds the display items necessary to paint the background of this render box to the display
    /// list if necessary.
    fn paint_background_if_applicable<E:ExtraDisplayListData>(
                                      self,
                                      list: &Cell<DisplayList<E>>,
                                      absolute_bounds: &Rect<Au>);

    /// Adds the display items necessary to paint the borders of this render box to a display list
    /// if necessary.
    fn paint_borders_if_applicable<E:ExtraDisplayListData>(
                                   self,
                                   list: &Cell<DisplayList<E>>,
                                   abs_bounds: &Rect<Au>);

    /// Adds the display items for this render box to the given display list.
    ///
    /// Arguments:
    /// * `builder`: The display list builder, which manages the coordinate system and options.
    /// * `dirty`: The dirty rectangle in the coordinate system of the owning flow.
    /// * `origin`: The total offset from the display list root flow to the owning flow of this
    ///   box.
    /// * `list`: The display list to which items should be appended.
    ///
    /// TODO: To implement stacking contexts correctly, we need to create a set of display lists,
    /// one per layer of the stacking context (CSS 2.1 § 9.9.1). Each box is passed the list set
    /// representing the box's stacking context. When asked to construct its constituent display
    /// items, each box puts its display items into the correct stack layer according to CSS 2.1
    /// Appendix E. Finally, the builder flattens the list.
    fn build_display_list<E:ExtraDisplayListData>(
                          self,
                          _: &DisplayListBuilder,
                          dirty: &Rect<Au>,
                          offset: &Point2D<Au>,
                          list: &Cell<DisplayList<E>>);
}

/// A box that represents a generic render box.
pub struct GenericRenderBox {
    base: RenderBoxBase,
}

impl GenericRenderBox {
    pub fn new(base: RenderBoxBase) -> GenericRenderBox {
        GenericRenderBox {
            base: base,
        }
    }
}

impl RenderBox for GenericRenderBox {
    fn base<'a>(&'a self) -> &'a RenderBoxBase {
        &self.base
    }

    fn mut_base<'a>(&'a mut self) -> &'a mut RenderBoxBase {
        &mut self.base
    }

    fn class(&self) -> RenderBoxClass {
        GenericRenderBoxClass
    }

    fn minimum_and_preferred_widths(&mut self) -> (Au, Au) {
        let guessed_width = self.base.guess_width();
        (guessed_width, guessed_width)
    }

    fn split_to_width(@mut self, _: Au, _: bool) -> SplitBoxResult {
        CannotSplit(self as @mut RenderBox)
    }

    fn box_height(&mut self) -> Au {
        Au::new(0)
    }

    fn assign_width(&mut self) {
        // FIXME(pcwalton): This seems clownshoes; can we remove?
        self.base.position.size.width = Au::from_px(45)
    }
}

/// A box that represents a (replaced content) image and its accompanying borders, shadows, etc.
pub struct ImageRenderBox {
    base: RenderBoxBase,
    image: ImageHolder,
}

impl ImageRenderBox {
    pub fn new(base: RenderBoxBase, image_url: Url, local_image_cache: @mut LocalImageCache)
               -> ImageRenderBox {
        assert!(base.node.is_image_element());

        ImageRenderBox {
            base: base,
            image: ImageHolder::new(image_url, local_image_cache),
        }
    }

    // Calculate the width of an image, accounting for the width attribute
    // TODO: This could probably go somewhere else
    pub fn image_width(&mut self) -> Au {
        let attr_width: Option<int> = do self.base.node.with_imm_element |elt| {
            match elt.get_attr("width") {
                Some(width) => {
                    FromStr::from_str(width)
                }
                None => {
                    None
                }
            }
        };

        // TODO: Consult margins and borders?
        let px_width = if attr_width.is_some() {
            attr_width.unwrap()
        } else {
            self.image.get_size().unwrap_or(Size2D(0, 0)).width
        };

        Au::from_px(px_width)
    }

    // Calculate the height of an image, accounting for the height attribute
    // TODO: This could probably go somewhere else
    pub fn image_height(&mut self) -> Au {
        let attr_height: Option<int> = do self.base.node.with_imm_element |elt| {
            match elt.get_attr("height") {
                Some(height) => {
                    FromStr::from_str(height)
                }
                None => {
                    None
                }
            }
        };

        // TODO: Consult margins and borders?
        let px_height = if attr_height.is_some() {
            attr_height.unwrap()
        } else {
            self.image.get_size().unwrap_or(Size2D(0, 0)).height
        };

        Au::from_px(px_height)
    }

    /// If this is an image render box, returns the underlying object. Fails otherwise.
    ///
    /// FIXME(pcwalton): Ugly. Replace with a real downcast operation.
    fn as_image_render_box(@mut self) -> @mut ImageRenderBox {
        self
    }
}

impl RenderBox for ImageRenderBox {
    fn base<'a>(&'a self) -> &'a RenderBoxBase {
        &self.base
    }

    fn mut_base<'a>(&'a mut self) -> &'a mut RenderBoxBase {
        &mut self.base
    }

    fn class(&self) -> RenderBoxClass {
        ImageRenderBoxClass
    }

    fn split_to_width(@mut self, _: Au, _: bool) -> SplitBoxResult {
        CannotSplit(self as @mut RenderBox)
    }

    fn minimum_and_preferred_widths(&mut self) -> (Au, Au) {
        let guessed_width = self.base.guess_width();
        let image_width = self.image_width();
        (guessed_width + image_width, guessed_width + image_width)
    }

    fn box_height(&mut self) -> Au {
        let size = self.image.get_size();
        let height = Au::from_px(size.unwrap_or(Size2D(0, 0)).height);
        self.base.position.size.height = height;
        debug!("box_height: found image height: %?", height);
        height
    }

    fn assign_width(&mut self) {
        let width = self.image_width();
        self.base.position.size.width = width;
    }

    /// If this is an image render box, returns the underlying object. Fails otherwise.
    ///
    /// FIXME(pcwalton): Ugly. Replace with a real downcast operation.
    fn as_image_render_box(@mut self) -> @mut ImageRenderBox {
        self
    }
}

/// A box representing a single run of text with a distinct style. A `TextRenderBox` may be split
/// into two or more boxes across line breaks. Several `TextBox`es may correspond to a
/// single DOM text node. Split text boxes are implemented by referring to subsets of a master
/// `TextRun` object.
pub struct TextRenderBox {
    base: RenderBoxBase,
    run: @TextRun,
    range: Range,
}

impl TextRenderBox {
    fn calculate_line_height(&self, font_size: Au) -> Au { 
        match self.base().line_height() {
            CSSLineHeightNormal => font_size.scale_by(1.14f),
            CSSLineHeightNumber(l) => font_size.scale_by(l),
            CSSLineHeightLength(Em(l)) => font_size.scale_by(l),
            CSSLineHeightLength(Px(l)) => Au::from_frac_px(l),
            CSSLineHeightPercentage(p) => font_size.scale_by(p / 100.0f)
        }
    }
}

impl RenderBox for TextRenderBox {
    fn base<'a>(&'a self) -> &'a RenderBoxBase {
        &self.base
    }

    fn mut_base<'a>(&'a mut self) -> &'a mut RenderBoxBase {
        &mut self.base
    }

    fn class(&self) -> RenderBoxClass {
        TextRenderBoxClass
    }

    fn teardown(&self) {
        self.run.teardown();
    }

    fn minimum_and_preferred_widths(&mut self) -> (Au, Au) {
        let guessed_width = self.base.guess_width();
        let min_width = self.run.min_width_for_range(&self.range);

        let mut max_line_width = Au::new(0);
        for line_range in self.run.iter_natural_lines_for_range(&self.range) {
            let line_metrics = self.run.metrics_for_range(&line_range);
            max_line_width = Au::max(max_line_width, line_metrics.advance_width);
        }

        (guessed_width + min_width, guessed_width + max_line_width)
    }

    fn box_height(&mut self) -> Au {
        let range = &self.range;
        let run = &self.run;

        // Compute the height based on the line-height and font size
        let text_bounds = run.metrics_for_range(range).bounding_box;
        let em_size = text_bounds.size.height;
        let line_height = self.calculate_line_height(em_size);

        line_height
    }

    fn assign_width(&mut self) {
        // Text boxes are preinitialized.
    }

    /// Attempts to split this box so that its width is no more than `max_width`. Fails if this box
    /// is an unscanned text box.
    fn split_to_width(@mut self, max_width: Au, starts_line: bool) -> SplitBoxResult {
        let mut pieces_processed_count: uint = 0;
        let mut remaining_width: Au = max_width;
        let mut left_range = Range::new(self.range.begin(), 0);
        let mut right_range: Option<Range> = None;

        debug!("split_to_width: splitting text box (strlen=%u, range=%?, avail_width=%?)",
               self.run.text.len(),
               self.range,
               max_width);

        for (glyphs, offset, slice_range) in self.run.iter_slices_for_range(&self.range) {
            debug!("split_to_width: considering slice (offset=%?, range=%?, remain_width=%?)",
                   offset,
                   slice_range,
                   remaining_width);

            let metrics = self.run.metrics_for_slice(glyphs, &slice_range);
            let advance = metrics.advance_width;
            let should_continue: bool;

            if advance <= remaining_width {
                should_continue = true;

                if starts_line && pieces_processed_count == 0 && glyphs.is_whitespace() {
                    debug!("split_to_width: case=skipping leading trimmable whitespace");
                    left_range.shift_by(slice_range.length() as int);
                } else {
                    debug!("split_to_width: case=enlarging span");
                    remaining_width = remaining_width - advance;
                    left_range.extend_by(slice_range.length() as int);
                }
            } else {    // The advance is more than the remaining width.
                should_continue = false;
                let slice_begin = offset + slice_range.begin();
                let slice_end = offset + slice_range.end();

                if glyphs.is_whitespace() {
                    // If there are still things after the trimmable whitespace, create the
                    // right chunk.
                    if slice_end < self.range.end() {
                        debug!("split_to_width: case=skipping trimmable trailing \
                                whitespace, then split remainder");
                        let right_range_end = self.range.end() - slice_end;
                        right_range = Some(Range::new(slice_end, right_range_end));
                    } else {
                        debug!("split_to_width: case=skipping trimmable trailing \
                                whitespace");
                    }
                } else if slice_begin < self.range.end() {
                    // There are still some things left over at the end of the line. Create
                    // the right chunk.
                    let right_range_end = self.range.end() - slice_begin;
                    right_range = Some(Range::new(slice_begin, right_range_end));
                    debug!("split_to_width: case=splitting remainder with right range=%?",
                           right_range);
                }
            }

            pieces_processed_count += 1;

            if !should_continue {
                break
            }
        }

        let left_box = if left_range.length() > 0 {
            let new_text_box = @mut text::adapt_textbox_with_range(&mut self.base,
                                                                   self.run,
                                                                   left_range);
            Some(new_text_box as @mut RenderBox)
        } else {
            None
        };

        let right_box = do right_range.map_default(None) |range: &Range| {
            let new_text_box = @mut text::adapt_textbox_with_range(&mut self.base,
                                                                   self.run,
                                                                   *range);
            Some(new_text_box as @mut RenderBox)
        };

        if pieces_processed_count == 1 || left_box.is_none() {
            SplitDidNotFit(left_box, right_box)
        } else {
            SplitDidFit(left_box, right_box)
        }
    }
}

/// The data for an unscanned text box.
pub struct UnscannedTextRenderBox {
    base: RenderBoxBase,
    text: ~str,

    // Cache font-style and text-decoration to check whether
    // this box can merge with another render box.
    font_style: Option<FontStyle>,
    text_decoration: Option<CSSTextDecoration>,
}

impl UnscannedTextRenderBox {
    /// Creates a new instance of `UnscannedTextRenderBox`.
    pub fn new(base: RenderBoxBase) -> UnscannedTextRenderBox {
        assert!(base.node.is_text());

        do base.node.with_imm_text |text_node| {
            // FIXME: Don't copy text; atomically reference count it instead.
            // FIXME(pcwalton): If we're just looking at node data, do we have to ensure this is
            // a text node?
            UnscannedTextRenderBox {
                base: base,
                text: text_node.element.data.to_str(),
                font_style: None,
                text_decoration: None,
            }
        }
    }

    /// Copies out the text from an unscanned text box.
    pub fn raw_text(&self) -> ~str {
        self.text.clone()
    }
}

impl RenderBox for UnscannedTextRenderBox {
    fn base<'a>(&'a self) -> &'a RenderBoxBase {
        &self.base
    }

    fn mut_base<'a>(&'a mut self) -> &'a mut RenderBoxBase {
        &mut self.base
    }

    fn class(&self) -> RenderBoxClass {
        UnscannedTextRenderBoxClass
    }

    fn is_whitespace_only(&self) -> bool {
        self.text.is_whitespace()
    }

    fn can_merge_with_box(&self, other: @mut RenderBox) -> bool {
        if other.class() == UnscannedTextRenderBoxClass {
            let this_base = self.base();
            let other_base = other.base();
            return this_base.font_style() == other_base.font_style() &&
                this_base.text_decoration() == other_base.text_decoration()
        }
        false
    }

    fn box_height(&mut self) -> Au {
        fail!("can't get height of unscanned text box")
    }

    /// Attempts to split this box so that its width is no more than `max_width`. Fails if this box
    /// is an unscanned text box.
    fn split_to_width(@mut self, _: Au, _: bool) -> SplitBoxResult {
        fail!("WAT: shouldn't be an unscanned text box here.")
    }

    /// Returns the *minimum width* and *preferred width* of this render box as defined by CSS 2.1.
    fn minimum_and_preferred_widths(&mut self) -> (Au, Au) {
        fail!("WAT: shouldn't be an unscanned text box here.")
    }

    fn assign_width(&mut self) {
        fail!("WAT: shouldn't be an unscanned text box here.")
    }

    /// If this is an unscanned text render box, returns the underlying object. Fails otherwise.
    ///
    /// FIXME(pcwalton): Ugly. Replace with a real downcast operation.
    fn as_unscanned_text_render_box(@mut self) -> @mut UnscannedTextRenderBox {
        self
    }
}

#[deriving(Eq)]
pub enum RenderBoxClass {
    GenericRenderBoxClass,
    ImageRenderBoxClass,
    TextRenderBoxClass,
    UnscannedTextRenderBoxClass,
}

/// Represents the outcome of attempting to split a box.
pub enum SplitBoxResult {
    CannotSplit(@mut RenderBox),
    // in general, when splitting the left or right side can
    // be zero length, due to leading/trailing trimmable whitespace
    SplitDidFit(Option<@mut RenderBox>, Option<@mut RenderBox>),
    SplitDidNotFit(Option<@mut RenderBox>, Option<@mut RenderBox>)
}

/// Data common to all boxes.
pub struct RenderBoxBase {
    /// The DOM node that this `RenderBox` originates from.
    node: AbstractNode<LayoutView>,

    /// The position of this box relative to its owning flow.
    position: Rect<Au>,

    /// The core parameters (border, padding, margin) used by the box model.
    model: BoxModel,

    /// A debug ID.
    ///
    /// TODO(#87) Make this only present in debug builds.
    id: int
}

impl RenderBoxBase {
    /// Constructs a new `RenderBoxBase` instance.
    pub fn new(node: AbstractNode<LayoutView>, id: int)
               -> RenderBoxBase {
        RenderBoxBase {
            node: node,
            position: Au::zero_rect(),
            model: Zero::zero(),
            id: id,
        }
    }

    pub fn id(&self) -> int {
        0
    }

    fn guess_width(&self) -> Au {
        let style = self.style();
        let font_size = style.font_size();
        let width = MaybeAuto::from_width(style.width(),
                                          Au(0),
                                          font_size).specified_or_zero();
        let margin_left = MaybeAuto::from_margin(style.margin_left(),
                                                 Au(0),
                                                 font_size).specified_or_zero();
        let margin_right = MaybeAuto::from_margin(style.margin_right(),
                                                  Au(0),
                                                  font_size).specified_or_zero();
        let padding_left = self.model.compute_padding_length(style.padding_left(),
                                                             Au(0),
                                                             font_size);
        let padding_right = self.model.compute_padding_length(style.padding_right(),
                                                              Au(0),
                                                              font_size);
        let border_left = self.model.compute_border_width(style.border_left_width(),
                                                          font_size);
        let border_right = self.model.compute_border_width(style.border_right_width(),
                                                           font_size);

        width + margin_left + margin_right + padding_left + padding_right + 
            border_left + border_right
    }

    pub fn compute_padding(&mut self, containing_block_width: Au) {
        self.model.compute_padding(self.node.style(), containing_block_width);
    }

    pub fn get_noncontent_width(&self) -> Au {
        self.model.border.left + self.model.padding.left + self.model.border.right +
            self.model.padding.right
    }

    /// The box formed by the content edge as defined in CSS 2.1 § 8.1. Coordinates are relative to
    /// the owning flow.
    pub fn content_box(&self) -> Rect<Au> {
        let origin = Point2D(self.position.origin.x +
                             self.model.border.left +
                             self.model.padding.left,
                             self.position.origin.y);
        let size = Size2D(self.position.size.width - self.get_noncontent_width(), 
                          self.position.size.height);
        Rect(origin, size)
    }

    /// The box formed by the border edge as defined in CSS 2.1 § 8.1. Coordinates are relative to
    /// the owning flow.
    pub fn border_box(&self) -> Rect<Au> {
        // TODO: Actually compute the content box, padding, and border.
        self.content_box()
    }

    /// The box formed by the margin edge as defined in CSS 2.1 § 8.1. Coordinates are relative to
    /// the owning flow.
    pub fn margin_box(&self) -> Rect<Au> {
        // TODO: Actually compute the content_box, padding, border, and margin.
        self.content_box()
    }

    /// Returns the nearest ancestor-or-self `Element` to the DOM node that this render box
    /// represents.
    ///
    /// If there is no ancestor-or-self `Element` node, fails.
    pub fn nearest_ancestor_element(&self) -> AbstractNode<LayoutView> {
        let mut node = self.node;
        while !node.is_element() {
            match node.parent_node() {
                None => fail!("no nearest element?!"),
                Some(parent) => node = parent,
            }
        }
        node
    }

    #[inline]
    pub fn clear(&self) -> Option<ClearType> {
        let style = self.node.style();
        match style.clear() {
            CSSClearNone => None,
            CSSClearLeft => Some(ClearLeft),
            CSSClearRight => Some(ClearRight),
            CSSClearBoth => Some(ClearBoth)
        }
    }

    /// Converts this node's computed style to a font style used for rendering.
    pub fn font_style(&self) -> FontStyle {
        let my_style = self.nearest_ancestor_element().style();

        debug!("(font style) start: %?", self.nearest_ancestor_element().type_id());

        // FIXME: Too much allocation here.
        let font_families = do my_style.font_family().map |family| {
            match *family {
                CSSFontFamilyFamilyName(ref family_str) => (*family_str).clone(),
                CSSFontFamilyGenericFamily(Serif)       => ~"serif",
                CSSFontFamilyGenericFamily(SansSerif)   => ~"sans-serif",
                CSSFontFamilyGenericFamily(Cursive)     => ~"cursive",
                CSSFontFamilyGenericFamily(Fantasy)     => ~"fantasy",
                CSSFontFamilyGenericFamily(Monospace)   => ~"monospace",
            }
        };
        let font_families = font_families.connect(", ");
        debug!("(font style) font families: `%s`", font_families);

        let font_size = match my_style.font_size() {
            CSSFontSizeLength(Px(length)) => length,
            // todo: this is based on a hard coded font size, should be the parent element's font size
            CSSFontSizeLength(Em(length)) => length * 16f, 
            _ => 16f // px units
        };
        debug!("(font style) font size: `%fpx`", font_size);

        let (italic, oblique) = match my_style.font_style() {
            CSSFontStyleNormal => (false, false),
            CSSFontStyleItalic => (true, false),
            CSSFontStyleOblique => (false, true),
        };

        FontStyle {
            pt_size: font_size,
            weight: FontWeight300,
            italic: italic,
            oblique: oblique,
            families: font_families,
        }
    }

    pub fn style(&self) -> CompleteStyle {
        self.node.style()
    }

    /// Returns the text alignment of the computed style of the nearest ancestor-or-self `Element`
    /// node.
    pub fn text_align(&self) -> CSSTextAlign {
        self.nearest_ancestor_element().style().text_align()
    }

    pub fn line_height(self) -> CSSLineHeight {
        self.nearest_ancestor_element().style().line_height()
    }

    pub fn vertical_align(self) -> CSSVerticalAlign {
        self.nearest_ancestor_element().style().vertical_align()
    }

    /// Returns the text decoration of the computed style of the nearest `Element` node
    pub fn text_decoration(self) -> CSSTextDecoration {
        /// Computes the propagated value of text-decoration, as specified in CSS 2.1 § 16.3.1
        /// TODO: make sure this works with anonymous box generation.
        fn get_propagated_text_decoration(element: AbstractNode<LayoutView>) -> CSSTextDecoration {
            //Skip over non-element nodes in the DOM
            if(!element.is_element()){
                return match element.parent_node() {
                    None => CSSTextDecorationNone,
                    Some(parent) => get_propagated_text_decoration(parent),
                };
            }

            //FIXME: is the root param on display() important?
            let display_in_flow = match element.style().display(false) {
                CSSDisplayInlineTable | CSSDisplayInlineBlock => false,
                _ => true,
            };

            let position = element.style().position();
            let float = element.style().float();

            let in_flow = (position == CSSPositionStatic) && (float == CSSFloatNone) &&
                display_in_flow;

            let text_decoration = element.style().text_decoration();

            if(text_decoration == CSSTextDecorationNone && in_flow){
                match element.parent_node() {
                    None => CSSTextDecorationNone,
                    Some(parent) => get_propagated_text_decoration(parent),
                }
            }
            else {
                text_decoration
            }
        }
        get_propagated_text_decoration(self.nearest_ancestor_element())
    }

}

impl RenderBoxUtils for @mut RenderBox {
    fn is_replaced(self) -> bool {
        self.class() == ImageRenderBoxClass
    }

    fn can_split(self) -> bool {
        self.class() == TextRenderBoxClass
    }

    /// Returns the amount of left and right "fringe" used by this box. This is based on margins,
    /// borders, padding, and width.
    fn get_used_width(self) -> (Au, Au) {
        // TODO: This should actually do some computation! See CSS 2.1, Sections 10.3 and 10.4.
        (Au::new(0), Au::new(0))
    }

    /// Returns the amount of left and right "fringe" used by this box. This should be based on
    /// margins, borders, padding, and width.
    fn get_used_height(self) -> (Au, Au) {
        // TODO: This should actually do some computation! See CSS 2.1, Sections 10.5 and 10.6.
        (Au::new(0), Au::new(0))
    }

    /// Adds the display items necessary to paint the background of this render box to the display
    /// list if necessary.
    fn paint_background_if_applicable<E:ExtraDisplayListData>(
                                      self,
                                      list: &Cell<DisplayList<E>>,
                                      absolute_bounds: &Rect<Au>) {
        // FIXME: This causes a lot of background colors to be displayed when they are clearly not
        // needed. We could use display list optimization to clean this up, but it still seems
        // inefficient. What we really want is something like "nearest ancestor element that
        // doesn't have a render box".
        let nearest_ancestor_element = self.base().nearest_ancestor_element();

        let background_color = nearest_ancestor_element.style().background_color();
        if !background_color.alpha.approx_eq(&0.0) {
            do list.with_mut_ref |list| {
                let solid_color_display_item = ~SolidColorDisplayItem {
                    base: BaseDisplayItem {
                        bounds: *absolute_bounds,
                        extra: ExtraDisplayListData::new(self),
                    },
                    color: background_color.to_gfx_color(),
                };

                list.append_item(SolidColorDisplayItemClass(solid_color_display_item))
            }
        }
    }

    /// Adds the display items necessary to paint the borders of this render box to a display list
    /// if necessary.
    fn paint_borders_if_applicable<E:ExtraDisplayListData>(
                                   self,
                                   list: &Cell<DisplayList<E>>,
                                   abs_bounds: &Rect<Au>) {
        // Fast path.
        let base = self.base();
        let border = base.model.border;
        if border.is_zero() {
            return
        }

        let (top_color, right_color, bottom_color, left_color) = (base.style().border_top_color(), base.style().border_right_color(), base.style().border_bottom_color(), base.style().border_left_color());
        let (top_style, right_style, bottom_style, left_style) = (base.style().border_top_style(), base.style().border_right_style(), base.style().border_bottom_style(), base.style().border_left_style());
        // Append the border to the display list.
        do list.with_mut_ref |list| {
            let border_display_item = ~BorderDisplayItem {
                base: BaseDisplayItem {
                    bounds: *abs_bounds,
                    extra: ExtraDisplayListData::new(self),
                },
                border: SideOffsets2D::new(border.top,
                                           border.right,
                                           border.bottom,
                                           border.left),
                color: SideOffsets2D::new(top_color.to_gfx_color(),
                                          right_color.to_gfx_color(),
                                          bottom_color.to_gfx_color(),
                                          left_color.to_gfx_color()),
                style: SideOffsets2D::new(top_style,
                                          right_style,
                                          bottom_style,
                                          left_style)
            };

            list.append_item(BorderDisplayItemClass(border_display_item))
        }
    }

    /// Adds the display items for this render box to the given display list.
    ///
    /// Arguments:
    /// * `builder`: The display list builder, which manages the coordinate system and options.
    /// * `dirty`: The dirty rectangle in the coordinate system of the owning flow.
    /// * `origin`: The total offset from the display list root flow to the owning flow of this
    ///   box.
    /// * `list`: The display list to which items should be appended.
    ///
    /// TODO: To implement stacking contexts correctly, we need to create a set of display lists,
    /// one per layer of the stacking context (CSS 2.1 § 9.9.1). Each box is passed the list set
    /// representing the box's stacking context. When asked to construct its constituent display
    /// items, each box puts its display items into the correct stack layer according to CSS 2.1
    /// Appendix E. Finally, the builder flattens the list.
    fn build_display_list<E:ExtraDisplayListData>(
                          self,
                          _: &DisplayListBuilder,
                          dirty: &Rect<Au>,
                          offset: &Point2D<Au>,
                          list: &Cell<DisplayList<E>>) {
        let base = self.base();
        let box_bounds = base.position;
        let absolute_box_bounds = box_bounds.translate(offset);
        debug!("RenderBox::build_display_list at rel=%?, abs=%?: %s",
               box_bounds, absolute_box_bounds, self.debug_str());
        debug!("RenderBox::build_display_list: dirty=%?, offset=%?", dirty, offset);

        if absolute_box_bounds.intersects(dirty) {
            debug!("RenderBox::build_display_list: intersected. Adding display item...");
        } else {
            debug!("RenderBox::build_display_list: Did not intersect...");
            return;
        }

        match self.class() {
            UnscannedTextRenderBoxClass => fail!("Shouldn't see unscanned boxes here."),
            TextRenderBoxClass => {
                let text_box = self.as_text_render_box();

                // Add the background to the list, if applicable.
                self.paint_background_if_applicable(list, &absolute_box_bounds);

                let nearest_ancestor_element = base.nearest_ancestor_element();
                let color = nearest_ancestor_element.style().color().to_gfx_color();

                // Create the text box.
                do list.with_mut_ref |list| {
                    let text_display_item = ~TextDisplayItem {
                        base: BaseDisplayItem {
                            bounds: absolute_box_bounds,
                            extra: ExtraDisplayListData::new(self),
                        },
                        // FIXME(pcwalton): Allocation? Why?!
                        text_run: ~text_box.run.serialize(),
                        range: text_box.range,
                        color: color,
                    };

                    list.append_item(TextDisplayItemClass(text_display_item))
                }

                // Draw debug frames for text bounds.
                //
                // FIXME(pcwalton): This is a bit of an abuse of the logging infrastructure. We
                // should have a real `SERVO_DEBUG` system.
                debug!("%?", {
                    // Compute the text box bounds and draw a border surrounding them.
                    let debug_border = SideOffsets2D::new_all_same(Au::from_px(1));

                    do list.with_mut_ref |list| {
                        let border_display_item = ~BorderDisplayItem {
                            base: BaseDisplayItem {
                                bounds: absolute_box_bounds,
                                extra: ExtraDisplayListData::new(self),
                            },
                            border: debug_border,
                            color: SideOffsets2D::new_all_same(rgb(0, 0, 200).to_gfx_color()),
                            style: SideOffsets2D::new_all_same(CSSBorderStyleSolid)

                        };
                        list.append_item(BorderDisplayItemClass(border_display_item))
                    }

                    // Draw a rectangle representing the baselines.
                    //
                    // TODO(Issue #221): Create and use a Line display item for the baseline.
                    let ascent = text_box.run.metrics_for_range(
                        &text_box.range).ascent;
                    let baseline = Rect(absolute_box_bounds.origin + Point2D(Au(0), ascent),
                                        Size2D(absolute_box_bounds.size.width, Au(0)));

                    do list.with_mut_ref |list| {
                        let border_display_item = ~BorderDisplayItem {
                            base: BaseDisplayItem {
                                bounds: baseline,
                                extra: ExtraDisplayListData::new(self),
                            },
                            border: debug_border,
                            color: SideOffsets2D::new_all_same(rgb(0, 200, 0).to_gfx_color()),
                            style: SideOffsets2D::new_all_same(CSSBorderStyleDashed)

                        };
                        list.append_item(BorderDisplayItemClass(border_display_item))
                    }

                    ()
                });
            },
            GenericRenderBoxClass => {
                // Add the background to the list, if applicable.
                self.paint_background_if_applicable(list, &absolute_box_bounds);

                // FIXME(pcwalton): This is a bit of an abuse of the logging infrastructure. We
                // should have a real `SERVO_DEBUG` system.
                debug!("%?", {
                    let debug_border = SideOffsets2D::new_all_same(Au::from_px(1));

                    do list.with_mut_ref |list| {
                        let border_display_item = ~BorderDisplayItem {
                            base: BaseDisplayItem {
                                bounds: absolute_box_bounds,
                                extra: ExtraDisplayListData::new(self),
                            },
                            border: debug_border,
                            color: SideOffsets2D::new_all_same(rgb(0, 0, 200).to_gfx_color()),
                            style: SideOffsets2D::new_all_same(CSSBorderStyleSolid)

                        };
                        list.append_item(BorderDisplayItemClass(border_display_item))
                    }

                    ()
                });
            },
            ImageRenderBoxClass => {
                let image_box = self.as_image_render_box();

                // Add the background to the list, if applicable.
                self.paint_background_if_applicable(list, &absolute_box_bounds);

                match image_box.image.get_image() {
                    Some(image) => {
                        debug!("(building display list) building image box");

                        // Place the image into the display list.
                        do list.with_mut_ref |list| {
                            let image_display_item = ~ImageDisplayItem {
                                base: BaseDisplayItem {
                                    bounds: absolute_box_bounds,
                                    extra: ExtraDisplayListData::new(self),
                                },
                                image: image.clone(),
                            };
                            list.append_item(ImageDisplayItemClass(image_display_item))
                        }
                    }
                    None => {
                        // No image data at all? Do nothing.
                        //
                        // TODO: Add some kind of placeholder image.
                        debug!("(building display list) no image :(");
                    }
                }
            }
        }

        // Add a border, if applicable.
        //
        // TODO: Outlines.
        self.paint_borders_if_applicable(list, &absolute_box_bounds);
    }
<<<<<<< HEAD

    /// Adds the display items necessary to paint the background of this render box to the display
    /// list if necessary.
    pub fn paint_background_if_applicable<E:ExtraDisplayListData>(&self,
                                                              list: &Cell<DisplayList<E>>,
                                                              absolute_bounds: &Rect<Au>) {
        // FIXME: This causes a lot of background colors to be displayed when they are clearly not
        // needed. We could use display list optimization to clean this up, but it still seems
        // inefficient. What we really want is something like "nearest ancestor element that
        // doesn't have a render box".
        let nearest_ancestor_element = self.nearest_ancestor_element();

        let background_color = nearest_ancestor_element.style().background_color();
        if !background_color.alpha.approx_eq(&0.0) {
            do list.with_mut_ref |list| {
                let solid_color_display_item = ~SolidColorDisplayItem {
                    base: BaseDisplayItem {
                        bounds: *absolute_bounds,
                        extra: ExtraDisplayListData::new(*self),
                    },
                    color: background_color.to_gfx_color(),
                };

                list.append_item(SolidColorDisplayItemClass(solid_color_display_item))
            }
        }
    }

    pub fn clear(&self) -> Option<ClearType> {
        let style = self.style();
        match style.clear() {
            CSSClearNone => None,
            CSSClearLeft => Some(ClearLeft),
            CSSClearRight => Some(ClearRight),
            CSSClearBoth => Some(ClearBoth)
        }
    }

    /// Converts this node's computed style to a font style used for rendering.
    pub fn font_style(&self) -> FontStyle {
        fn get_font_style(element: AbstractNode<LayoutView>) -> FontStyle {
            let my_style = element.style();

            debug!("(font style) start: %?", element.type_id());

            // FIXME: Too much allocation here.
            let font_families = do my_style.font_family().map |family| {
                match *family {
                    CSSFontFamilyFamilyName(ref family_str) => (*family_str).clone(),
                    CSSFontFamilyGenericFamily(Serif)       => ~"serif",
                    CSSFontFamilyGenericFamily(SansSerif)   => ~"sans-serif",
                    CSSFontFamilyGenericFamily(Cursive)     => ~"cursive",
                    CSSFontFamilyGenericFamily(Fantasy)     => ~"fantasy",
                    CSSFontFamilyGenericFamily(Monospace)   => ~"monospace",
                }
            };
            let font_families = font_families.connect(", ");
            debug!("(font style) font families: `%s`", font_families);

            let font_size = match my_style.font_size() {
                CSSFontSizeLength(Px(length)) => length,
                // todo: this is based on a hard coded font size, should be the parent element's font size
                CSSFontSizeLength(Em(length)) => length * 16f,
                _ => 16f // px units
            };
            debug!("(font style) font size: `%fpx`", font_size);

            let (italic, oblique) = match my_style.font_style() {
                CSSFontStyleNormal => (false, false),
                CSSFontStyleItalic => (true, false),
                CSSFontStyleOblique => (false, true),
            };

            FontStyle {
                pt_size: font_size,
                weight: FontWeight300,
                italic: italic,
                oblique: oblique,
                families: font_families,
            }
        }

        let font_style_cached = match *self {
            UnscannedTextRenderBoxClass(ref box) => {
                match box.font_style {
                    Some(ref style) => Some(style.clone()),
                    None => None
                }
            }
            _ => None
        };

        if font_style_cached.is_some() {
            return font_style_cached.unwrap();
        } else {
            let font_style = get_font_style(self.nearest_ancestor_element());
            match *self {
                UnscannedTextRenderBoxClass(ref box) => {
                    box.font_style = Some(font_style.clone());
                }
                _ => ()
            }
            return font_style;
        }
    }

    /// Returns the text alignment of the computed style of the nearest ancestor-or-self `Element`
    /// node.
    pub fn text_align(&self) -> CSSTextAlign {
        self.nearest_ancestor_element().style().text_align()
    }

    pub fn line_height(&self) -> CSSLineHeight {
        self.nearest_ancestor_element().style().line_height()
    }

    pub fn vertical_align(&self) -> CSSVerticalAlign {
        self.nearest_ancestor_element().style().vertical_align()
    }

    /// Returns the text decoration of the computed style of the nearest `Element` node
    pub fn text_decoration(&self) -> CSSTextDecoration {
        /// Computes the propagated value of text-decoration, as specified in CSS 2.1 § 16.3.1
        /// TODO: make sure this works with anonymous box generation.
        fn get_propagated_text_decoration(element: AbstractNode<LayoutView>) -> CSSTextDecoration {
            //Skip over non-element nodes in the DOM
            if(!element.is_element()){
                return match element.parent_node() {
                    None => CSSTextDecorationNone,
                    Some(parent) => get_propagated_text_decoration(parent),
                };
            }

            //FIXME: is the root param on display() important?
            let display_in_flow = match element.style().display(false) {
                CSSDisplayInlineTable | CSSDisplayInlineBlock => false,
                _ => true,
            };

            let position = element.style().position();
            let float = element.style().float();

            let in_flow = (position == CSSPositionStatic) && (float == CSSFloatNone) &&
                display_in_flow;

            let text_decoration = element.style().text_decoration();

            if(text_decoration == CSSTextDecorationNone && in_flow){
                match element.parent_node() {
                    None => CSSTextDecorationNone,
                    Some(parent) => get_propagated_text_decoration(parent),
                }
            }
            else {
                text_decoration
            }
        }

        let text_decoration_cached = match *self {
            UnscannedTextRenderBoxClass(ref box) => {
                match box.text_decoration {
                    Some(ref decoration) => Some(decoration.clone()),
                    None => None
                }
            }
            _ => None
        };

        if text_decoration_cached.is_some() {
            return text_decoration_cached.unwrap();
        } else {
            let text_decoration = get_propagated_text_decoration(self.nearest_ancestor_element());
            match *self {
                UnscannedTextRenderBoxClass(ref box) => {
                    box.text_decoration = Some(text_decoration.clone());
                }
                _ => ()
            }
            return text_decoration;
        }
    }

    /// Dumps this node, for debugging.
    pub fn dump(&self) {
        self.dump_indent(0);
    }

    /// Dumps a render box for debugging, with indentation.
    pub fn dump_indent(&self, indent: uint) {
        let mut string = ~"";
        for _ in range(0u, indent) {
            string.push_str("    ");
        }

        string.push_str(self.debug_str());
        debug!("%s", string);
    }

    /// Returns a debugging string describing this box.
    pub fn debug_str(&self) -> ~str {
        let representation = match *self {
            GenericRenderBoxClass(*) => ~"GenericRenderBox",
            ImageRenderBoxClass(*) => ~"ImageRenderBox",
            TextRenderBoxClass(text_box) => {
                fmt!("TextRenderBox(text=%s)", text_box.run.text.slice_chars(text_box.range.begin(),
                                                                             text_box.range.end()))
            }
            UnscannedTextRenderBoxClass(text_box) => {
                fmt!("UnscannedTextRenderBox(%s)", text_box.text)
            }
        };

        fmt!("box b%?: %s", self.id(), representation)
    }

    //
    // Painting
    //

    /// Adds the display items necessary to paint the borders of this render box to a display list
    /// if necessary.
    pub fn paint_borders_if_applicable<E:ExtraDisplayListData>(&self,
                                                               list: &Cell<DisplayList<E>>,
                                                               abs_bounds: &Rect<Au>) {
        // Fast path.
        let border = do self.with_base |base| {
            base.model.border
        };
        if border.is_zero() {
            return
        }

        let (top_color, right_color, bottom_color, left_color) = (self.style().border_top_color(), self.style().border_right_color(), self.style().border_bottom_color(), self.style().border_left_color());
        let (top_style, right_style, bottom_style, left_style) = (self.style().border_top_style(), self.style().border_right_style(), self.style().border_bottom_style(), self.style().border_left_style());
        // Append the border to the display list.
        do list.with_mut_ref |list| {
            let border_display_item = ~BorderDisplayItem {
                base: BaseDisplayItem {
                    bounds: *abs_bounds,
                    extra: ExtraDisplayListData::new(*self),
                },
                border: SideOffsets2D::new(border.top,
                                           border.right,
                                           border.bottom,
                                           border.left),
                color: SideOffsets2D::new(top_color.to_gfx_color(),
                                          right_color.to_gfx_color(),
                                          bottom_color.to_gfx_color(),
                                          left_color.to_gfx_color()),
                style: SideOffsets2D::new(top_style,
                                          right_style,
                                          bottom_style,
                                          left_style)
            };

            list.append_item(BorderDisplayItemClass(border_display_item))
        }
    }
=======
>>>>>>> wip
}

