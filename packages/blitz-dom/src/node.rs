use atomic_refcell::{AtomicRef, AtomicRefCell};
use color::{AlphaColor, Srgb};
use keyboard_types::Modifiers;
use markup5ever::{LocalName, QualName, local_name};
use parley::{Cluster, FontContext, LayoutContext};
use peniko::kurbo;
use selectors::matching::{ElementSelectorFlags, QuirksMode};
use slab::Slab;
use std::cell::{Cell, RefCell};
use std::fmt::Write;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use style::Atom;
use style::invalidation::element::restyle_hints::RestyleHint;
use style::properties::ComputedValues;
use style::properties::generated::longhands::position::computed_value::T as Position;
use style::selector_parser::PseudoElement;
use style::stylesheets::UrlExtraData;
use style::values::computed::Display;
use style::values::specified::box_::{DisplayInside, DisplayOutside};
use style::{
    data::ElementData,
    properties::{PropertyDeclarationBlock, parse_style_attribute},
    servo_arc::Arc as ServoArc,
    shared_lock::{Locked, SharedRwLock},
    stylesheets::CssRuleType,
};
use style_dom::ElementState;
use style_traits::values::ToCss;
use taffy::{
    Cache,
    prelude::{Layout, Style},
};
use url::Url;

use crate::layout::table::TableContext;
use blitz_traits::{BlitzMouseButtonEvent, DomEventData, HitResult};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayOuter {
    Block,
    Inline,
    None,
}

// todo: might be faster to migrate this to ecs and split apart at a different boundary
pub struct Node {
    // The actual tree we belong to. This is unsafe!!
    tree: *mut Slab<Node>,

    /// Our Id
    pub id: usize,
    /// Our parent's ID
    pub parent: Option<usize>,
    // What are our children?
    pub children: Vec<usize>,
    /// Our parent in the layout hierachy: a separate list that includes anonymous collections of inline elements
    pub layout_parent: Cell<Option<usize>>,
    /// A separate child list that includes anonymous collections of inline elements
    pub layout_children: RefCell<Option<Vec<usize>>>,
    /// The same as layout_children, but sorted by z-index
    pub paint_children: RefCell<Option<Vec<usize>>>,

    /// Node type (Element, TextNode, etc) specific data
    pub data: NodeData,

    // This little bundle of joy is our style data from stylo and a lock guard that allows access to it
    // TODO: See if guard can be hoisted to a higher level
    pub stylo_element_data: AtomicRefCell<Option<ElementData>>,
    pub selector_flags: AtomicRefCell<ElementSelectorFlags>,
    pub guard: SharedRwLock,
    pub element_state: ElementState,

    // Pseudo element nodes
    pub before: Option<usize>,
    pub after: Option<usize>,

    // Taffy layout data:
    pub style: Style,
    pub has_snapshot: bool,
    pub snapshot_handled: AtomicBool,
    pub display_outer: DisplayOuter,
    pub cache: Cache,
    pub unrounded_layout: Layout,
    pub final_layout: Layout,
    pub scroll_offset: kurbo::Point,

    // Flags
    pub is_inline_root: bool,
    pub is_table_root: bool,
}

impl Node {
    pub(crate) fn new(
        tree: *mut Slab<Node>,
        id: usize,
        guard: SharedRwLock,
        data: NodeData,
    ) -> Self {
        Self {
            tree,

            id,
            parent: None,
            children: vec![],
            layout_parent: Cell::new(None),
            layout_children: RefCell::new(None),
            paint_children: RefCell::new(None),

            data,
            stylo_element_data: Default::default(),
            selector_flags: AtomicRefCell::new(ElementSelectorFlags::empty()),
            guard,
            element_state: ElementState::empty(),

            before: None,
            after: None,

            style: Default::default(),
            has_snapshot: false,
            snapshot_handled: AtomicBool::new(false),
            display_outer: DisplayOuter::Block,
            cache: Cache::new(),
            unrounded_layout: Layout::new(),
            final_layout: Layout::new(),
            scroll_offset: kurbo::Point::ZERO,
            is_inline_root: false,
            is_table_root: false,
        }
    }

    pub fn pe_by_index(&self, index: usize) -> Option<usize> {
        match index {
            0 => self.after,
            1 => self.before,
            _ => panic!("Invalid pseudo element index"),
        }
    }

    pub fn set_pe_by_index(&mut self, index: usize, value: Option<usize>) {
        match index {
            0 => self.after = value,
            1 => self.before = value,
            _ => panic!("Invalid pseudo element index"),
        }
    }

    pub(crate) fn display_style(&self) -> Option<Display> {
        Some(self.primary_styles().as_ref()?.clone_display())
    }

    pub fn is_or_contains_block(&self) -> bool {
        let style = self.primary_styles();
        let style = style.as_ref();

        // Ignore out-of-flow items
        let position = style
            .map(|s| s.clone_position())
            .unwrap_or(Position::Relative);
        let is_in_flow = matches!(
            position,
            Position::Static | Position::Relative | Position::Sticky
        );
        if !is_in_flow {
            return false;
        }
        let display = style
            .map(|s| s.clone_display())
            .unwrap_or(Display::inline());
        match display.outside() {
            DisplayOutside::None => false,
            DisplayOutside::Block => true,
            _ => {
                if display.inside() == DisplayInside::Flow {
                    self.children
                        .iter()
                        .copied()
                        .any(|child_id| self.tree()[child_id].is_or_contains_block())
                } else {
                    false
                }
            }
        }
    }

    pub fn is_focussable(&self) -> bool {
        self.data
            .downcast_element()
            .map(|el| el.is_focussable)
            .unwrap_or(false)
    }

    pub fn set_restyle_hint(&mut self, hint: RestyleHint) {
        if let Some(element_data) = self.stylo_element_data.borrow_mut().as_mut() {
            element_data.hint.insert(hint);
        }
    }

    pub fn hover(&mut self) {
        self.element_state.insert(ElementState::HOVER);
        self.set_restyle_hint(RestyleHint::restyle_subtree());
    }

    pub fn unhover(&mut self) {
        self.element_state.remove(ElementState::HOVER);
        self.set_restyle_hint(RestyleHint::restyle_subtree());
    }

    pub fn is_hovered(&self) -> bool {
        self.element_state.contains(ElementState::HOVER)
    }

    pub fn focus(&mut self) {
        self.element_state
            .insert(ElementState::FOCUS | ElementState::FOCUSRING);
        self.set_restyle_hint(RestyleHint::restyle_subtree());
    }

    pub fn blur(&mut self) {
        self.element_state
            .remove(ElementState::FOCUS | ElementState::FOCUSRING);
        self.set_restyle_hint(RestyleHint::restyle_subtree());
    }

    pub fn is_focussed(&self) -> bool {
        self.element_state.contains(ElementState::FOCUS)
    }

    pub fn active(&mut self) {
        self.element_state.insert(ElementState::ACTIVE);
        self.set_restyle_hint(RestyleHint::restyle_subtree());
    }

    pub fn unactive(&mut self) {
        self.element_state.remove(ElementState::ACTIVE);
        self.set_restyle_hint(RestyleHint::restyle_subtree());
    }

    pub fn is_active(&self) -> bool {
        self.element_state.contains(ElementState::ACTIVE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeKind {
    Document,
    Element,
    AnonymousBlock,
    Text,
    Comment,
}

/// The different kinds of nodes in the DOM.
#[derive(Debug, Clone)]
pub enum NodeData {
    /// The `Document` itself - the root node of a HTML document.
    Document,

    /// An element with attributes.
    Element(ElementNodeData),

    /// An anonymous block box
    AnonymousBlock(ElementNodeData),

    /// A text node.
    Text(TextNodeData),

    /// A comment.
    Comment,
    // Comment { contents: String },

    // /// A `DOCTYPE` with name, public id, and system id. See
    // /// [document type declaration on wikipedia][https://en.wikipedia.org/wiki/Document_type_declaration]
    // Doctype { name: String, public_id: String, system_id: String },

    // /// A Processing instruction.
    // ProcessingInstruction { target: String, contents: String },
}

impl NodeData {
    pub fn downcast_element(&self) -> Option<&ElementNodeData> {
        match self {
            Self::Element(data) => Some(data),
            Self::AnonymousBlock(data) => Some(data),
            _ => None,
        }
    }

    pub fn downcast_element_mut(&mut self) -> Option<&mut ElementNodeData> {
        match self {
            Self::Element(data) => Some(data),
            Self::AnonymousBlock(data) => Some(data),
            _ => None,
        }
    }

    pub fn is_element_with_tag_name(&self, name: &impl PartialEq<LocalName>) -> bool {
        let Some(elem) = self.downcast_element() else {
            return false;
        };
        *name == elem.name.local
    }

    pub fn attrs(&self) -> Option<&[Attribute]> {
        Some(&self.downcast_element()?.attrs)
    }

    pub fn attr(&self, name: impl PartialEq<LocalName>) -> Option<&str> {
        self.downcast_element()?.attr(name)
    }

    pub fn has_attr(&self, name: impl PartialEq<LocalName>) -> bool {
        self.downcast_element()
            .is_some_and(|elem| elem.has_attr(name))
    }

    pub fn kind(&self) -> NodeKind {
        match self {
            NodeData::Document => NodeKind::Document,
            NodeData::Element(_) => NodeKind::Element,
            NodeData::AnonymousBlock(_) => NodeKind::AnonymousBlock,
            NodeData::Text(_) => NodeKind::Text,
            NodeData::Comment => NodeKind::Comment,
        }
    }
}

/// A tag attribute, e.g. `class="test"` in `<div class="test" ...>`.
///
/// The namespace on the attribute name is almost always ns!("").
/// The tokenizer creates all attributes this way, but the tree
/// builder will adjust certain attribute names inside foreign
/// content (MathML, SVG).
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub struct Attribute {
    /// The name of the attribute (e.g. the `class` in `<div class="test">`)
    pub name: QualName,
    /// The value of the attribute (e.g. the `"test"` in `<div class="test">`)
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct ElementNodeData {
    /// The elements tag name, namespace and prefix
    pub name: QualName,

    /// The elements id attribute parsed as an atom (if it has one)
    pub id: Option<Atom>,

    /// The element's attributes
    pub attrs: Vec<Attribute>,

    /// Whether the element is focussable
    pub is_focussable: bool,

    /// The element's parsed style attribute (used by stylo)
    pub style_attribute: Option<ServoArc<Locked<PropertyDeclarationBlock>>>,

    /// Heterogeneous data that depends on the element's type.
    /// For example:
    ///   - The image data for \<img\> elements.
    ///   - The parley Layout for inline roots.
    ///   - The text editor for input/textarea elements
    pub node_specific_data: NodeSpecificData,

    pub background_images: Vec<Option<BackgroundImageData>>,

    /// Parley text layout (elements with inline inner display mode only)
    pub inline_layout_data: Option<Box<TextLayout>>,

    //Data associated with display: list-item. Note that this display mode
    // does not exclude inline_layout_data
    pub list_item_data: Option<Box<ListItemLayout>>,

    /// The element's template contents (\<template\> elements only)
    pub template_contents: Option<usize>,
    // /// Whether the node is a [HTML integration point] (https://html.spec.whatwg.org/multipage/#html-integration-point)
    // pub mathml_annotation_xml_integration_point: bool,
}

impl ElementNodeData {
    pub fn new(name: QualName, attrs: Vec<Attribute>) -> Self {
        let id_attr_atom = attrs
            .iter()
            .find(|attr| &attr.name.local == "id")
            .map(|attr| attr.value.as_ref())
            .map(|value: &str| Atom::from(value));

        let mut data = ElementNodeData {
            name,
            id: id_attr_atom,
            attrs,
            is_focussable: false,
            style_attribute: Default::default(),
            inline_layout_data: None,
            list_item_data: None,
            node_specific_data: NodeSpecificData::None,
            template_contents: None,
            background_images: Vec::new(),
        };
        data.flush_is_focussable();
        data
    }

    pub fn attrs(&self) -> &[Attribute] {
        &self.attrs
    }

    pub fn attr(&self, name: impl PartialEq<LocalName>) -> Option<&str> {
        let attr = self.attrs.iter().find(|attr| name == attr.name.local)?;
        Some(&attr.value)
    }

    pub fn attr_parsed<T: FromStr>(&self, name: impl PartialEq<LocalName>) -> Option<T> {
        let attr = self.attrs.iter().find(|attr| name == attr.name.local)?;
        attr.value.parse::<T>().ok()
    }

    /// Detects the presence of the attribute, treating *any* value as truthy.
    pub fn has_attr(&self, name: impl PartialEq<LocalName>) -> bool {
        self.attrs.iter().any(|attr| name == attr.name.local)
    }

    pub fn image_data(&self) -> Option<&ImageData> {
        match &self.node_specific_data {
            NodeSpecificData::Image(data) => Some(&**data),
            _ => None,
        }
    }

    pub fn image_data_mut(&mut self) -> Option<&mut ImageData> {
        match self.node_specific_data {
            NodeSpecificData::Image(ref mut data) => Some(&mut **data),
            _ => None,
        }
    }

    pub fn raster_image_data(&self) -> Option<&RasterImageData> {
        match self.image_data()? {
            ImageData::Raster(data) => Some(data),
            _ => None,
        }
    }

    pub fn raster_image_data_mut(&mut self) -> Option<&mut RasterImageData> {
        match self.image_data_mut()? {
            ImageData::Raster(data) => Some(data),
            _ => None,
        }
    }

    #[cfg(feature = "svg")]
    pub fn svg_data(&self) -> Option<&usvg::Tree> {
        match self.image_data()? {
            ImageData::Svg(data) => Some(data),
            _ => None,
        }
    }

    #[cfg(feature = "svg")]
    pub fn svg_data_mut(&mut self) -> Option<&mut usvg::Tree> {
        match self.image_data_mut()? {
            ImageData::Svg(data) => Some(data),
            _ => None,
        }
    }

    pub fn text_input_data(&self) -> Option<&TextInputData> {
        match &self.node_specific_data {
            NodeSpecificData::TextInput(data) => Some(data),
            _ => None,
        }
    }

    pub fn text_input_data_mut(&mut self) -> Option<&mut TextInputData> {
        match &mut self.node_specific_data {
            NodeSpecificData::TextInput(data) => Some(data),
            _ => None,
        }
    }

    pub fn checkbox_input_checked(&self) -> Option<bool> {
        match self.node_specific_data {
            NodeSpecificData::CheckboxInput(checked) => Some(checked),
            _ => None,
        }
    }

    pub fn checkbox_input_checked_mut(&mut self) -> Option<&mut bool> {
        match self.node_specific_data {
            NodeSpecificData::CheckboxInput(ref mut checked) => Some(checked),
            _ => None,
        }
    }

    pub fn flush_is_focussable(&mut self) {
        let disabled: bool = self.attr_parsed(local_name!("disabled")).unwrap_or(false);
        let tabindex: Option<i32> = self.attr_parsed(local_name!("tabindex"));

        self.is_focussable = !disabled
            && match tabindex {
                Some(index) => index >= 0,
                None => {
                    // Some focusable HTML elements have a default tabindex value of 0 set under the hood by the user agent.
                    // These elements are:
                    //   - <a> or <area> with href attribute
                    //   - <button>, <frame>, <iframe>, <input>, <object>, <select>, <textarea>, and SVG <a> element
                    //   - <summary> element that provides summary for a <details> element.

                    if [local_name!("a"), local_name!("area")].contains(&self.name.local) {
                        self.attr(local_name!("href")).is_some()
                    } else {
                        const DEFAULT_FOCUSSABLE_ELEMENTS: [LocalName; 6] = [
                            local_name!("button"),
                            local_name!("input"),
                            local_name!("select"),
                            local_name!("textarea"),
                            local_name!("frame"),
                            local_name!("iframe"),
                        ];
                        DEFAULT_FOCUSSABLE_ELEMENTS.contains(&self.name.local)
                    }
                }
            }
    }

    pub fn flush_style_attribute(&mut self, guard: &SharedRwLock, base_url: Option<Url>) {
        self.style_attribute = self.attr(local_name!("style")).map(|style_str| {
            let url = UrlExtraData::from(base_url.clone().unwrap_or_else(|| {
                "data:text/css;charset=utf-8;base64,"
                    .parse::<Url>()
                    .unwrap()
            }));

            ServoArc::new(guard.wrap(parse_style_attribute(
                style_str,
                &url,
                None,
                QuirksMode::NoQuirks,
                CssRuleType::Style,
            )))
        });
    }

    pub fn take_inline_layout(&mut self) -> Option<Box<TextLayout>> {
        std::mem::take(&mut self.inline_layout_data)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RasterImageData {
    /// The width of the image
    pub width: u32,
    /// The height of the image
    pub height: u32,
    /// The raw image data in RGBA8 format
    pub data: Arc<Vec<u8>>,
}
impl RasterImageData {
    pub fn new(width: u32, height: u32, data: Arc<Vec<u8>>) -> Self {
        Self {
            width,
            height,
            data,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ImageData {
    Raster(RasterImageData),
    #[cfg(feature = "svg")]
    Svg(Box<usvg::Tree>),
    None,
}
#[cfg(feature = "svg")]
impl From<usvg::Tree> for ImageData {
    fn from(value: usvg::Tree) -> Self {
        Self::Svg(Box::new(value))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Ok,
    Error,
    Loading,
}

#[derive(Debug, Clone)]
pub struct BackgroundImageData {
    /// The url of the background image
    pub url: ServoArc<Url>,
    /// The loading status of the background image
    pub status: Status,
    /// The image data
    pub image: ImageData,
}

impl BackgroundImageData {
    pub fn new(url: ServoArc<Url>) -> Self {
        Self {
            url,
            status: Status::Loading,
            image: ImageData::None,
        }
    }
}

pub struct TextInputData {
    /// A parley TextEditor instance
    pub editor: Box<parley::PlainEditor<TextBrush>>,
    /// Whether the input is a singleline or multiline input
    pub is_multiline: bool,
}

// FIXME: Implement Clone for PlainEditor
impl Clone for TextInputData {
    fn clone(&self) -> Self {
        TextInputData::new(self.is_multiline)
    }
}

impl TextInputData {
    pub fn new(is_multiline: bool) -> Self {
        let editor = Box::new(parley::PlainEditor::new(16.0));
        Self {
            editor,
            is_multiline,
        }
    }

    pub fn set_text(
        &mut self,
        font_ctx: &mut FontContext,
        layout_ctx: &mut LayoutContext<TextBrush>,
        text: &str,
    ) {
        if self.editor.text() != text {
            self.editor.set_text(text);
            self.editor.driver(font_ctx, layout_ctx).refresh_layout();
        }
    }
}

/// Heterogeneous data that depends on the element's type.
#[derive(Clone)]
pub enum NodeSpecificData {
    /// The element's image content (\<img\> element's only)
    Image(Box<ImageData>),
    /// Pre-computed table layout data
    TableRoot(Arc<TableContext>),
    /// Parley text editor (text inputs)
    TextInput(TextInputData),
    /// Checkbox checked state
    CheckboxInput(bool),
    /// No data (for nodes that don't need any node-specific data)
    None,
}

impl std::fmt::Debug for NodeSpecificData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeSpecificData::Image(data) => match **data {
                ImageData::Raster(_) => f.write_str("NodeSpecificData::Image(Raster)"),
                #[cfg(feature = "svg")]
                ImageData::Svg(_) => f.write_str("NodeSpecificData::Image(Svg)"),
                ImageData::None => f.write_str("NodeSpecificData::Image(None)"),
            },
            NodeSpecificData::TableRoot(_) => f.write_str("NodeSpecificData::TableRoot"),
            NodeSpecificData::TextInput(_) => f.write_str("NodeSpecificData::TextInput"),
            NodeSpecificData::CheckboxInput(_) => f.write_str("NodeSpecificData::CheckboxInput"),
            NodeSpecificData::None => f.write_str("NodeSpecificData::None"),
        }
    }
}

impl Default for NodeSpecificData {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone)]
pub struct ListItemLayout {
    pub marker: Marker,
    pub position: ListItemLayoutPosition,
}

//We seperate chars from strings in order to optimise rendering - ie not needing to
//construct a whole parley layout for simple char markers
#[derive(Debug, PartialEq, Clone)]
pub enum Marker {
    Char(char),
    String(String),
}

//Value depends on list-style-position, determining whether a seperate layout is created for it
#[derive(Clone)]
pub enum ListItemLayoutPosition {
    Inside,
    Outside(Box<parley::Layout<TextBrush>>),
}

impl std::fmt::Debug for ListItemLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ListItemLayout - marker {:?}", self.marker)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
/// Parley Brush type for Blitz which contains a `peniko::Brush` and a Blitz node id
pub struct TextBrush {
    /// The node id for the span
    pub id: usize,
    /// Peniko brush for the span (represents text color)
    pub brush: peniko::Brush,
}

impl TextBrush {
    pub(crate) fn from_peniko_brush(brush: peniko::Brush) -> Self {
        Self { id: 0, brush }
    }
    pub(crate) fn from_color(color: AlphaColor<Srgb>) -> Self {
        Self::from_peniko_brush(peniko::Brush::Solid(color))
    }
    pub(crate) fn from_id_and_color(id: usize, color: AlphaColor<Srgb>) -> Self {
        Self {
            id,
            brush: peniko::Brush::Solid(color),
        }
    }
}

#[derive(Clone)]
pub struct TextLayout {
    pub text: String,
    pub layout: parley::layout::Layout<TextBrush>,
}

impl std::fmt::Debug for TextLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TextLayout")
    }
}

#[derive(Debug, Clone)]
pub struct TextNodeData {
    /// The textual content of the text node
    pub content: String,
}

impl TextNodeData {
    pub fn new(content: String) -> Self {
        Self { content }
    }
}

/*
-> Computed styles
-> Layout
-----> Needs to happen only when styles are computed
*/

// type DomRefCell<T> = RefCell<T>;

// pub struct DomData {
//     // ... we can probs just get away with using the html5ever types directly. basically just using the servo dom, but without the bindings
//     local_name: html5ever::LocalName,
//     tag_name: html5ever::QualName,
//     namespace: html5ever::Namespace,
//     prefix: DomRefCell<Option<html5ever::Prefix>>,
//     attrs: DomRefCell<Vec<Attr>>,
//     // attrs: DomRefCell<Vec<Dom<Attr>>>,
//     id_attribute: DomRefCell<Option<Atom>>,
//     is: DomRefCell<Option<LocalName>>,
//     // style_attribute: DomRefCell<Option<Arc<Locked<PropertyDeclarationBlock>>>>,
//     // attr_list: MutNullableDom<NamedNodeMap>,
//     // class_list: MutNullableDom<DOMTokenList>,
//     state: Cell<ElementState>,
// }

impl Node {
    pub fn tree(&self) -> &Slab<Node> {
        unsafe { &*self.tree }
    }

    #[track_caller]
    pub fn with(&self, id: usize) -> &Node {
        self.tree().get(id).unwrap()
    }

    pub fn print_tree(&self, level: usize) {
        println!(
            "{} {} {:?} {} {:?}",
            "  ".repeat(level),
            self.id,
            self.parent,
            self.node_debug_str().replace('\n', ""),
            self.children
        );
        // println!("{} {:?}", "  ".repeat(level), self.children);
        for child_id in self.children.iter() {
            let child = self.with(*child_id);
            child.print_tree(level + 1)
        }
    }

    // Get the index of the current node in the parents child list
    pub fn child_index(&self) -> Option<usize> {
        self.tree()[self.parent?]
            .children
            .iter()
            .position(|id| *id == self.id)
    }

    // Get the nth node in the parents child list
    pub fn forward(&self, n: usize) -> Option<&Node> {
        let child_idx = self.child_index().unwrap_or(0);
        self.tree()[self.parent?]
            .children
            .get(child_idx + n)
            .map(|id| self.with(*id))
    }

    pub fn backward(&self, n: usize) -> Option<&Node> {
        let child_idx = self.child_index().unwrap_or(0);
        if child_idx < n {
            return None;
        }

        self.tree()[self.parent?]
            .children
            .get(child_idx - n)
            .map(|id| self.with(*id))
    }

    pub fn is_element(&self) -> bool {
        matches!(self.data, NodeData::Element { .. })
    }

    pub fn is_anonymous(&self) -> bool {
        matches!(self.data, NodeData::AnonymousBlock { .. })
    }

    pub fn is_text_node(&self) -> bool {
        matches!(self.data, NodeData::Text { .. })
    }

    pub fn element_data(&self) -> Option<&ElementNodeData> {
        match self.data {
            NodeData::Element(ref data) => Some(data),
            NodeData::AnonymousBlock(ref data) => Some(data),
            _ => None,
        }
    }

    pub fn element_data_mut(&mut self) -> Option<&mut ElementNodeData> {
        match self.data {
            NodeData::Element(ref mut data) => Some(data),
            NodeData::AnonymousBlock(ref mut data) => Some(data),
            _ => None,
        }
    }

    pub fn text_data(&self) -> Option<&TextNodeData> {
        match self.data {
            NodeData::Text(ref data) => Some(data),
            _ => None,
        }
    }

    pub fn text_data_mut(&mut self) -> Option<&mut TextNodeData> {
        match self.data {
            NodeData::Text(ref mut data) => Some(data),
            _ => None,
        }
    }

    pub fn node_debug_str(&self) -> String {
        let mut s = String::new();

        match &self.data {
            NodeData::Document => write!(s, "DOCUMENT"),
            // NodeData::Doctype { name, .. } => write!(s, "DOCTYPE {name}"),
            NodeData::Text(data) => {
                let bytes = data.content.as_bytes();
                write!(
                    s,
                    "TEXT {}",
                    &std::str::from_utf8(bytes.split_at(10.min(bytes.len())).0)
                        .unwrap_or("INVALID UTF8")
                )
            }
            NodeData::Comment => write!(
                s,
                "COMMENT",
                // &std::str::from_utf8(data.contents.as_bytes().split_at(10).0).unwrap_or("INVALID UTF8")
            ),
            NodeData::AnonymousBlock(_) => write!(s, "AnonymousBlock"),
            NodeData::Element(data) => {
                let name = &data.name;
                let class = self.attr(local_name!("class")).unwrap_or("");
                if !class.is_empty() {
                    write!(
                        s,
                        "<{} class=\"{}\"> ({:?})",
                        name.local, class, self.display_outer
                    )
                } else {
                    write!(s, "<{}> ({:?})", name.local, self.display_outer)
                }
            } // NodeData::ProcessingInstruction { .. } => write!(s, "ProcessingInstruction"),
        }
        .unwrap();
        s
    }

    pub fn outer_html(&self) -> String {
        let mut output = String::new();
        self.write_outer_html(&mut output);
        output
    }

    pub fn write_outer_html(&self, writer: &mut String) {
        let has_children = !self.children.is_empty();
        let current_color = self
            .primary_styles()
            .map(|style| style.clone_color())
            .map(|color| color.to_css_string());

        match &self.data {
            NodeData::Document => {}
            NodeData::Comment => {}
            NodeData::AnonymousBlock(_) => {}
            // NodeData::Doctype { name, .. } => write!(s, "DOCTYPE {name}"),
            NodeData::Text(data) => {
                writer.push_str(data.content.as_str());
            }
            NodeData::Element(data) => {
                writer.push('<');
                writer.push_str(&data.name.local);

                for attr in data.attrs() {
                    writer.push(' ');
                    writer.push_str(&attr.name.local);
                    writer.push_str("=\"");
                    #[allow(clippy::unnecessary_unwrap)] // Convert to if-let chain once stabilised
                    if current_color.is_some() && attr.value.contains("currentColor") {
                        writer.push_str(
                            &attr
                                .value
                                .replace("currentColor", current_color.as_ref().unwrap()),
                        );
                    } else {
                        writer.push_str(&attr.value);
                    }
                    writer.push('"');
                }
                if !has_children {
                    writer.push_str(" /");
                }
                writer.push('>');

                if has_children {
                    for &child_id in &self.children {
                        self.tree()[child_id].write_outer_html(writer);
                    }

                    writer.push_str("</");
                    writer.push_str(&data.name.local);
                    writer.push('>');
                }
            }
        }
    }

    pub fn attrs(&self) -> Option<&[Attribute]> {
        Some(&self.element_data()?.attrs)
    }

    pub fn attr(&self, name: LocalName) -> Option<&str> {
        let attr = self.attrs()?.iter().find(|id| id.name.local == name)?;
        Some(&attr.value)
    }

    pub fn primary_styles(&self) -> Option<AtomicRef<'_, ComputedValues>> {
        let stylo_element_data = self.stylo_element_data.borrow();
        if stylo_element_data
            .as_ref()
            .and_then(|d| d.styles.get_primary())
            .is_some()
        {
            Some(AtomicRef::map(
                stylo_element_data,
                |data: &Option<ElementData>| -> &ComputedValues {
                    data.as_ref().unwrap().styles.get_primary().unwrap()
                },
            ))
        } else {
            None
        }
    }

    pub fn text_content(&self) -> String {
        let mut out = String::new();
        self.write_text_content(&mut out);
        out
    }

    fn write_text_content(&self, out: &mut String) {
        match &self.data {
            NodeData::Text(data) => {
                out.push_str(&data.content);
            }
            NodeData::Element(..) | NodeData::AnonymousBlock(..) => {
                for child_id in self.children.iter() {
                    self.with(*child_id).write_text_content(out);
                }
            }
            _ => {}
        }
    }

    pub fn flush_style_attribute(&mut self, base_url: Option<Url>) {
        if let NodeData::Element(ref mut elem_data) = self.data {
            elem_data.flush_style_attribute(&self.guard, base_url);
        }
    }

    pub fn order(&self) -> i32 {
        self.primary_styles()
            .map(|s| match s.pseudo() {
                Some(PseudoElement::Before) => i32::MIN,
                Some(PseudoElement::After) => i32::MAX,
                _ => s.clone_order(),
            })
            .unwrap_or(0)
    }

    pub fn z_index(&self) -> i32 {
        self.primary_styles()
            .map(|s| s.clone_z_index().integer_or(0))
            .unwrap_or(0)
    }

    /// Takes an (x, y) position (relative to the *parent's* top-left corner) and returns:
    ///    - None if the position is outside of this node's bounds
    ///    - Some(HitResult) if the position is within the node but doesn't match any children
    ///    - The result of recursively calling child.hit() on the the child element that is
    ///      positioned at that position if there is one.
    ///
    /// TODO: z-index
    /// (If multiple children are positioned at the position then a random one will be recursed into)
    pub fn hit(&self, x: f32, y: f32) -> Option<HitResult> {
        let mut x = x - self.final_layout.location.x + self.scroll_offset.x as f32;
        let mut y = y - self.final_layout.location.y + self.scroll_offset.y as f32;

        let size = self.final_layout.size;
        let matches_self = !(x < 0.0
            || x > size.width + self.scroll_offset.x as f32
            || y < 0.0
            || y > size.height + self.scroll_offset.y as f32);

        let content_size = self.final_layout.content_size;
        let matches_content = !(x < 0.0
            || x > content_size.width + self.scroll_offset.x as f32
            || y < 0.0
            || y > content_size.height + self.scroll_offset.y as f32);

        if !matches_self && !matches_content {
            return None;
        }

        if self.is_inline_root {
            let content_box_offset = taffy::Point {
                x: self.final_layout.padding.left + self.final_layout.border.left,
                y: self.final_layout.padding.top + self.final_layout.border.top,
            };
            x -= content_box_offset.x;
            y -= content_box_offset.y;
        }

        // Call `.hit()` on each child in turn. If any return `Some` then return that value. Else return `Some(self.id).
        self.paint_children
            .borrow()
            .iter()
            .flatten()
            .rev()
            .find_map(|&i| self.with(i).hit(x, y))
            .or_else(|| {
                if self.is_inline_root {
                    let element_data = &self.element_data().unwrap();
                    let layout = &element_data.inline_layout_data.as_ref().unwrap().layout;
                    let scale = layout.scale();

                    Cluster::from_point(layout, x * scale, y * scale).and_then(|(cluster, _)| {
                        let style_index = cluster.glyphs().next()?.style_index();
                        let node_id = layout.styles()[style_index].brush.id;
                        Some(HitResult { node_id, x, y })
                    })
                } else {
                    None
                }
            })
            .or(Some(HitResult {
                node_id: self.id,
                x,
                y,
            })
            .filter(|_| matches_self))
    }

    /// Computes the Document-relative coordinates of the Node
    pub fn absolute_position(&self, x: f32, y: f32) -> taffy::Point<f32> {
        let x = x + self.final_layout.location.x - self.scroll_offset.x as f32;
        let y = y + self.final_layout.location.y - self.scroll_offset.y as f32;

        // Recurse up the layout hierarchy
        self.layout_parent
            .get()
            .map(|i| self.with(i).absolute_position(x, y))
            .unwrap_or(taffy::Point { x, y })
    }

    /// Creates a synthetic click event
    pub fn synthetic_click_event(&self, mods: Modifiers) -> DomEventData {
        DomEventData::Click(self.synthetic_click_event_data(mods))
    }

    pub fn synthetic_click_event_data(&self, mods: Modifiers) -> BlitzMouseButtonEvent {
        let absolute_position = self.absolute_position(0.0, 0.0);
        let x = absolute_position.x + (self.final_layout.size.width / 2.0);
        let y = absolute_position.y + (self.final_layout.size.height / 2.0);

        BlitzMouseButtonEvent {
            x,
            y,
            mods,
            button: Default::default(),
            buttons: Default::default(),
        }
    }
}

/// It might be wrong to expose this since what does *equality* mean outside the dom?
impl PartialEq for Node {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Node {}

impl std::fmt::Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // FIXME: update to reflect changes to fields
        f.debug_struct("NodeData")
            .field("parent", &self.parent)
            .field("id", &self.id)
            .field("is_inline_root", &self.is_inline_root)
            .field("children", &self.children)
            .field("layout_children", &self.layout_children.borrow())
            // .field("style", &self.style)
            .field("node", &self.data)
            .field("stylo_element_data", &self.stylo_element_data)
            // .field("unrounded_layout", &self.unrounded_layout)
            // .field("final_layout", &self.final_layout)
            .finish()
    }
}
