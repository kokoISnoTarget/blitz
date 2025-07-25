use crate::events::handle_dom_event;
use crate::layout::construct::collect_layout_children;
use crate::mutator::ViewportMut;
use crate::net::{Resource, StylesheetLoader};
use crate::node::{ImageData, NodeFlags, RasterImageData, SpecialElementData, Status, TextBrush};
use crate::stylo_to_cursor_icon::stylo_to_cursor_icon;
use crate::traversal::TreeTraverser;
use crate::url::DocumentUrl;
use crate::util::ImageType;
use crate::{
    DEFAULT_CSS, DocumentConfig, DocumentMutator, ElementData, EventDriver, Node, NodeData,
    NoopEventHandler, TextNodeData,
};
use app_units::Au;
use blitz_traits::devtools::DevtoolSettings;
use blitz_traits::events::{DomEvent, HitResult, UiEvent};
use blitz_traits::navigation::{DummyNavigationProvider, NavigationProvider};
use blitz_traits::net::{DummyNetProvider, NetProvider, SharedProvider};
use blitz_traits::shell::{ColorScheme, DummyShellProvider, ShellProvider, Viewport};
use cursor_icon::CursorIcon;
use markup5ever::local_name;
use parley::FontContext;
use peniko::{Blob, kurbo};
use selectors::{Element, matching::QuirksMode};
use slab::Slab;
use std::any::Any;
use std::collections::{BTreeMap, Bound, HashMap, HashSet};
use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Context as TaskContext;
use style::Atom;
use style::attr::{AttrIdentifier, AttrValue};
use style::data::{ElementData as StyloElementData, ElementStyles};
use style::media_queries::MediaType;
use style::properties::ComputedValues;
use style::properties::style_structs::Font;
use style::queries::values::PrefersColorScheme;
use style::selector_parser::ServoElementSnapshot;
use style::servo::media_queries::FontMetricsProvider;
use style::servo_arc::Arc as ServoArc;
use style::values::GenericAtomIdent;
use style::values::computed::Overflow;
use style::{
    dom::{TDocument, TNode},
    media_queries::{Device, MediaList},
    selector_parser::SnapshotMap,
    shared_lock::{SharedRwLock, StylesheetGuards},
    stylesheets::{AllowImportRules, DocumentStyleSheet, Origin, Stylesheet},
    stylist::Stylist,
};
use taffy::AvailableSpace;
use url::Url;

/// Abstraction over wrappers around [`BaseDocument`] to allow for them all to
/// be driven by [`blitz-shell`](https://docs.rs/blitz-shell)
pub trait Document: Deref<Target = BaseDocument> + DerefMut + 'static {
    /// Update the [`Document`] in response to a [`UiEvent`] (click, keypress, etc)
    fn handle_ui_event(&mut self, event: UiEvent) {
        let mut driver = EventDriver::new((*self).mutate(), NoopEventHandler);
        driver.handle_ui_event(event);
    }

    /// Poll any pending async operations, and flush changes to the underlying [`BaseDocument`]
    fn poll(&mut self, task_context: Option<TaskContext>) -> bool {
        // Default implementation does nothing
        let _ = task_context;
        false
    }

    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// Get the [`Document`]'s id
    fn id(&self) -> usize {
        self.id
    }
}

// TODO: implement a proper font metrics provider
#[derive(Debug, Clone)]
struct DummyFontMetricsProvider;
impl FontMetricsProvider for DummyFontMetricsProvider {
    fn query_font_metrics(
        &self,
        _vertical: bool,
        _font: &Font,
        _base_size: style::values::computed::CSSPixelLength,
        _flags: style::values::computed::font::QueryFontMetricsFlags,
    ) -> style::font_metrics::FontMetrics {
        Default::default()
    }

    fn base_size_for_generic(
        &self,
        generic: style::values::computed::font::GenericFontFamily,
    ) -> style::values::computed::Length {
        let size = match generic {
            style::values::computed::font::GenericFontFamily::Monospace => 13.0,
            _ => 16.0,
        };
        style::values::computed::Length::from(Au::from_f32_px(size))
    }
}

pub struct BaseDocument {
    /// ID of the document
    id: usize,

    // Config
    /// Base url for resolving linked resources (stylesheets, images, fonts, etc)
    pub(crate) url: DocumentUrl,
    // Devtool settings. Currently used to render debug overlays
    pub(crate) devtool_settings: DevtoolSettings,
    // Viewport details such as the dimensions, HiDPI scale, and zoom factor,
    pub(crate) viewport: Viewport,
    // Scroll within our viewport
    pub(crate) viewport_scroll: kurbo::Point,

    /// A slab-backed tree of nodes
    ///
    /// We pin the tree to a guarantee to the nodes it creates that the tree is stable in memory.
    /// There is no way to create the tree - publicly or privately - that would invalidate that invariant.
    pub(crate) nodes: Box<Slab<Node>>,

    // Stylo
    /// The Stylo engine
    pub(crate) stylist: Stylist,
    /// Stylo shared lock
    pub(crate) guard: SharedRwLock,
    /// Stylo invalidation map. We insert into this map prior to mutating nodes.
    pub(crate) snapshots: SnapshotMap,

    // Parley contexts
    /// A Parley font context
    pub(crate) font_ctx: parley::FontContext,
    /// A Parley layout context
    pub(crate) layout_ctx: parley::LayoutContext<TextBrush>,

    /// The node which is currently hovered (if any)
    pub(crate) hover_node_id: Option<usize>,
    /// The node which is currently focussed (if any)
    pub(crate) focus_node_id: Option<usize>,
    /// The node which is currently active (if any)
    pub(crate) active_node_id: Option<usize>,
    /// The node which recieved a mousedown event (if any)
    pub(crate) mousedown_node_id: Option<usize>,
    /// Whether there are active animations (so we should re-render every frame)
    pub(crate) is_animating: bool,

    /// Map of node ID's for fast lookups
    pub(crate) nodes_to_id: HashMap<String, usize>,
    /// Map of `<style>` and `<link>` node IDs to their associated stylesheet
    pub(crate) nodes_to_stylesheet: BTreeMap<usize, DocumentStyleSheet>,
    /// Stylesheets added by the useragent
    /// where the key is the hashed CSS
    pub(crate) ua_stylesheets: HashMap<String, DocumentStyleSheet>,
    /// Map from form control node ID's to their associated forms node ID's
    pub(crate) controls_to_form: HashMap<usize, usize>,
    /// Set of changed nodes for updating the accessibility tree
    pub(crate) changed_nodes: HashSet<usize>,

    // Service providers
    /// Network provider. Can be used to fetch assets.
    pub net_provider: Arc<dyn NetProvider<Resource>>,
    /// Navigation provider. Can be used to navigate to a new page (bubbles up the event
    /// on e.g. clicking a Link)
    pub navigation_provider: Arc<dyn NavigationProvider>,
    /// Shell provider. Can be used to request a redraw or set the cursor icon
    pub shell_provider: Arc<dyn ShellProvider>,
}

pub(crate) fn make_device(viewport: &Viewport) -> Device {
    let width = viewport.window_size.0 as f32 / viewport.scale();
    let height = viewport.window_size.1 as f32 / viewport.scale();
    let viewport_size = euclid::Size2D::new(width, height);
    let device_pixel_ratio = euclid::Scale::new(viewport.scale());

    Device::new(
        MediaType::screen(),
        selectors::matching::QuirksMode::NoQuirks,
        viewport_size,
        device_pixel_ratio,
        Box::new(DummyFontMetricsProvider),
        ComputedValues::initial_values_with_font_override(Font::initial_values()),
        match viewport.color_scheme {
            ColorScheme::Light => PrefersColorScheme::Light,
            ColorScheme::Dark => PrefersColorScheme::Dark,
        },
    )
}

impl BaseDocument {
    /// Create a new (empty) [`BaseDocument`] with the specified configuration
    pub fn new(config: DocumentConfig) -> Self {
        static ID_GENERATOR: AtomicUsize = AtomicUsize::new(1);

        let id = ID_GENERATOR.fetch_add(1, Ordering::SeqCst);
        let viewport = config.viewport.unwrap_or_default();
        let device = make_device(&viewport);
        let stylist = Stylist::new(device, QuirksMode::NoQuirks);
        let snapshots = SnapshotMap::new();
        let nodes = Box::new(Slab::new());
        let guard = SharedRwLock::new();
        let nodes_to_id = HashMap::new();

        // Make sure we turn on stylo features
        style_config::set_bool("layout.flexbox.enabled", true);
        style_config::set_bool("layout.grid.enabled", true);
        style_config::set_bool("layout.legacy_layout", true);
        style_config::set_bool("layout.unimplemented", true);
        style_config::set_bool("layout.columns.enabled", true);

        let base_url = config
            .base_url
            .and_then(|url| DocumentUrl::from_str(&url).ok())
            .unwrap_or_default();

        let font_ctx = config.font_ctx.unwrap_or_else(|| {
            let mut font_ctx = FontContext::default();
            font_ctx
                .collection
                .register_fonts(Blob::new(Arc::new(crate::BULLET_FONT) as _), None);
            font_ctx
        });

        let net_provider = config
            .net_provider
            .unwrap_or_else(|| Arc::new(DummyNetProvider));
        let navigation_provider = config
            .navigation_provider
            .unwrap_or_else(|| Arc::new(DummyNavigationProvider));
        let shell_provider = config
            .shell_provider
            .unwrap_or_else(|| Arc::new(DummyShellProvider));

        let mut doc = Self {
            id,
            guard,
            nodes,
            stylist,
            snapshots,
            nodes_to_id,
            viewport,
            devtool_settings: DevtoolSettings::default(),
            viewport_scroll: kurbo::Point::ZERO,
            url: base_url,
            ua_stylesheets: HashMap::new(),
            nodes_to_stylesheet: BTreeMap::new(),
            font_ctx,
            layout_ctx: parley::LayoutContext::new(),

            hover_node_id: None,
            focus_node_id: None,
            active_node_id: None,
            mousedown_node_id: None,
            is_animating: false,
            changed_nodes: HashSet::new(),
            controls_to_form: HashMap::new(),
            net_provider,
            navigation_provider,
            shell_provider,
        };

        // Initialise document with root Document node
        doc.create_node(NodeData::Document);
        doc.root_node_mut().flags.insert(NodeFlags::IS_IN_DOCUMENT);

        match config.ua_stylesheets {
            Some(stylesheets) => {
                for ss in &stylesheets {
                    doc.add_user_agent_stylesheet(ss);
                }
            }
            None => doc.add_user_agent_stylesheet(DEFAULT_CSS),
        }

        // Stylo data on the root node container is needed to render the node
        let stylo_element_data = StyloElementData {
            styles: ElementStyles {
                primary: Some(
                    ComputedValues::initial_values_with_font_override(Font::initial_values())
                        .to_arc(),
                ),
                ..Default::default()
            },
            ..Default::default()
        };
        *doc.root_node().stylo_element_data.borrow_mut() = Some(stylo_element_data);

        doc
    }

    /// Set the Document's networking provider
    pub fn set_net_provider(&mut self, net_provider: SharedProvider<Resource>) {
        self.net_provider = net_provider;
    }

    /// Set the Document's navigation provider
    pub fn set_navigation_provider(&mut self, navigation_provider: Arc<dyn NavigationProvider>) {
        self.navigation_provider = navigation_provider;
    }

    /// Set the Document's shell provider
    pub fn set_shell_provider(&mut self, shell_provider: Arc<dyn ShellProvider>) {
        self.shell_provider = shell_provider;
    }

    /// Set base url for resolving linked resources (stylesheets, images, fonts, etc)
    pub fn set_base_url(&mut self, url: &str) {
        self.url = DocumentUrl::from(Url::parse(url).unwrap());
    }

    pub fn guard(&self) -> &SharedRwLock {
        &self.guard
    }

    pub fn tree(&self) -> &Slab<Node> {
        &self.nodes
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn get_node(&self, node_id: usize) -> Option<&Node> {
        self.nodes.get(node_id)
    }

    pub fn get_node_mut(&mut self, node_id: usize) -> Option<&mut Node> {
        self.nodes.get_mut(node_id)
    }

    pub fn get_focussed_node_id(&self) -> Option<usize> {
        self.focus_node_id
            .or(self.try_root_element().map(|el| el.id))
    }

    pub fn mutate<'doc>(&'doc mut self) -> DocumentMutator<'doc> {
        DocumentMutator::new(self)
    }

    pub fn handle_dom_event<F: FnMut(DomEvent)>(
        &mut self,
        event: &mut DomEvent,
        dispatch_event: F,
    ) {
        handle_dom_event(self, event, dispatch_event)
    }

    pub fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Find the label's bound input elements:
    /// the element id referenced by the "for" attribute of a given label element
    /// or the first input element which is nested in the label
    /// Note that although there should only be one bound element,
    /// we return all possibilities instead of just the first
    /// in order to allow the caller to decide which one is correct
    pub fn label_bound_input_element(&self, label_node_id: usize) -> Option<&Node> {
        let label_element = self.nodes[label_node_id].element_data()?;
        if let Some(target_element_dom_id) = label_element.attr(local_name!("for")) {
            TreeTraverser::new(self)
                .filter_map(|id| {
                    let node = self.get_node(id)?;
                    let element_data = node.element_data()?;
                    if element_data.name.local != local_name!("input") {
                        return None;
                    }
                    let id = element_data.id.as_ref()?;
                    if *id == *target_element_dom_id {
                        Some(node)
                    } else {
                        None
                    }
                })
                .next()
        } else {
            TreeTraverser::new_with_root(self, label_node_id)
                .filter_map(|child_id| {
                    let node = self.get_node(child_id)?;
                    let element_data = node.element_data()?;
                    if element_data.name.local == local_name!("input") {
                        Some(node)
                    } else {
                        None
                    }
                })
                .next()
        }
    }

    pub fn toggle_checkbox(el: &mut ElementData) -> bool {
        let Some(is_checked) = el.checkbox_input_checked_mut() else {
            return false;
        };
        *is_checked = !*is_checked;

        *is_checked
    }

    pub fn toggle_radio(&mut self, radio_set_name: String, target_radio_id: usize) {
        for i in 0..self.nodes.len() {
            let node = &mut self.nodes[i];
            if let Some(node_data) = node.data.downcast_element_mut() {
                if node_data.attr(local_name!("name")) == Some(&radio_set_name) {
                    let was_clicked = i == target_radio_id;
                    let Some(is_checked) = node_data.checkbox_input_checked_mut() else {
                        continue;
                    };
                    *is_checked = was_clicked;
                }
            }
        }
    }

    pub fn root_node(&self) -> &Node {
        &self.nodes[0]
    }

    pub fn root_node_mut(&mut self) -> &mut Node {
        &mut self.nodes[0]
    }

    pub fn try_root_element(&self) -> Option<&Node> {
        TDocument::as_node(&self.root_node()).first_element_child()
    }

    pub fn root_element(&self) -> &Node {
        TDocument::as_node(&self.root_node())
            .first_element_child()
            .unwrap()
            .as_element()
            .unwrap()
    }

    pub fn create_node(&mut self, node_data: NodeData) -> usize {
        let slab_ptr = self.nodes.as_mut() as *mut Slab<Node>;
        let guard = self.guard.clone();

        let entry = self.nodes.vacant_entry();
        let id = entry.key();
        entry.insert(Node::new(slab_ptr, id, guard, node_data));

        // Mark the new node as changed.
        self.changed_nodes.insert(id);
        id
    }

    /// Whether the document has been mutated
    pub fn has_changes(&self) -> bool {
        self.changed_nodes.is_empty()
    }

    pub fn create_text_node(&mut self, text: &str) -> usize {
        let content = text.to_string();
        let data = NodeData::Text(TextNodeData::new(content));
        self.create_node(data)
    }

    pub fn deep_clone_node(&mut self, node_id: usize) -> usize {
        // Load existing node
        let node = &self.nodes[node_id];
        let data = node.data.clone();
        let children = node.children.clone();

        // Create new node
        let new_node_id = self.create_node(data);

        // Recursively clone children
        let new_children: Vec<usize> = children
            .into_iter()
            .map(|child_id| self.deep_clone_node(child_id))
            .collect();
        for &child_id in &new_children {
            self.nodes[child_id].parent = Some(new_node_id);
        }
        self.nodes[new_node_id].children = new_children;

        new_node_id
    }

    pub(crate) fn remove_and_drop_pe(&mut self, node_id: usize) -> Option<Node> {
        fn remove_pe_ignoring_parent(doc: &mut BaseDocument, node_id: usize) -> Option<Node> {
            let mut node = doc.nodes.try_remove(node_id);
            if let Some(node) = &mut node {
                for &child in &node.children {
                    remove_pe_ignoring_parent(doc, child);
                }
            }
            node
        }

        let node = remove_pe_ignoring_parent(self, node_id);

        // Update child_idx values
        if let Some(parent_id) = node.as_ref().and_then(|node| node.parent) {
            let parent = &mut self.nodes[parent_id];
            parent.children.retain(|id| *id != node_id);
        }

        node
    }

    pub(crate) fn resolve_url(&self, raw: &str) -> url::Url {
        self.url.resolve_relative(raw).unwrap_or_else(|| {
            panic!(
                "to be able to resolve {raw} with the base_url: {:?}",
                *self.url
            )
        })
    }

    pub fn print_tree(&self) {
        crate::util::walk_tree(0, self.root_node());
    }

    pub fn print_subtree(&self, node_id: usize) {
        crate::util::walk_tree(0, &self.nodes[node_id]);
    }

    pub fn process_style_element(&mut self, target_id: usize) {
        let css = self.nodes[target_id].text_content();
        let css = html_escape::decode_html_entities(&css);
        let sheet = self.make_stylesheet(&css, Origin::Author);
        self.add_stylesheet_for_node(sheet, target_id);
    }

    pub fn remove_user_agent_stylesheet(&mut self, contents: &str) {
        if let Some(sheet) = self.ua_stylesheets.remove(contents) {
            self.stylist.remove_stylesheet(sheet, &self.guard.read());
        }
    }

    pub fn add_user_agent_stylesheet(&mut self, css: &str) {
        let sheet = self.make_stylesheet(css, Origin::UserAgent);
        self.ua_stylesheets.insert(css.to_string(), sheet.clone());
        self.stylist.append_stylesheet(sheet, &self.guard.read());
    }

    pub fn make_stylesheet(&self, css: impl AsRef<str>, origin: Origin) -> DocumentStyleSheet {
        let data = Stylesheet::from_str(
            css.as_ref(),
            self.url.url_extra_data(),
            origin,
            ServoArc::new(self.guard.wrap(MediaList::empty())),
            self.guard.clone(),
            Some(&StylesheetLoader(self.id, self.net_provider.clone())),
            None,
            QuirksMode::NoQuirks,
            AllowImportRules::Yes,
        );

        DocumentStyleSheet(ServoArc::new(data))
    }

    pub fn upsert_stylesheet_for_node(&mut self, node_id: usize) {
        let raw_styles = self.nodes[node_id].text_content();
        let sheet = self.make_stylesheet(raw_styles, Origin::Author);
        self.add_stylesheet_for_node(sheet, node_id);
    }

    pub fn add_stylesheet_for_node(&mut self, stylesheet: DocumentStyleSheet, node_id: usize) {
        let old = self.nodes_to_stylesheet.insert(node_id, stylesheet.clone());

        if let Some(old) = old {
            self.stylist.remove_stylesheet(old, &self.guard.read())
        }

        // Store data on element
        let element = &mut self.nodes[node_id].element_data_mut().unwrap();
        element.special_data = SpecialElementData::Stylesheet(stylesheet.clone());

        // TODO: Nodes could potentially get reused so ordering by node_id might be wrong.
        let insertion_point = self
            .nodes_to_stylesheet
            .range((Bound::Excluded(node_id), Bound::Unbounded))
            .next()
            .map(|(_, sheet)| sheet);

        if let Some(insertion_point) = insertion_point {
            self.stylist.insert_stylesheet_before(
                stylesheet,
                insertion_point.clone(),
                &self.guard.read(),
            )
        } else {
            self.stylist
                .append_stylesheet(stylesheet, &self.guard.read())
        }
    }

    pub fn load_resource(&mut self, resource: Resource) {
        match resource {
            Resource::Css(node_id, css) => {
                self.add_stylesheet_for_node(css, node_id);
            }
            Resource::Image(node_id, kind, width, height, image_data) => {
                let node = self.get_node_mut(node_id).unwrap();

                match kind {
                    ImageType::Image => {
                        node.element_data_mut().unwrap().special_data =
                            SpecialElementData::Image(Box::new(ImageData::Raster(
                                RasterImageData::new(width, height, image_data),
                            )));

                        // Clear layout cache
                        node.cache.clear();
                    }
                    ImageType::Background(idx) => {
                        if let Some(Some(bg_image)) = node
                            .element_data_mut()
                            .and_then(|el| el.background_images.get_mut(idx))
                        {
                            bg_image.status = Status::Ok;
                            bg_image.image =
                                ImageData::Raster(RasterImageData::new(width, height, image_data))
                        }
                    }
                }
            }
            #[cfg(feature = "svg")]
            Resource::Svg(node_id, kind, tree) => {
                let node = self.get_node_mut(node_id).unwrap();

                match kind {
                    ImageType::Image => {
                        node.element_data_mut().unwrap().special_data =
                            SpecialElementData::Image(Box::new(ImageData::Svg(tree)));

                        // Clear layout cache
                        node.cache.clear();
                    }
                    ImageType::Background(idx) => {
                        if let Some(Some(bg_image)) = node
                            .element_data_mut()
                            .and_then(|el| el.background_images.get_mut(idx))
                        {
                            bg_image.status = Status::Ok;
                            bg_image.image = ImageData::Svg(tree);
                        }
                    }
                }
            }
            Resource::Font(bytes) => {
                // TODO: Implement FontInfoOveride
                // TODO: Investigate eliminating double-box
                self.font_ctx
                    .collection
                    .register_fonts(Blob::new(Arc::new(bytes)) as _, None);
            }
            Resource::None => {
                // Do nothing
            }
            _ => {}
        }
    }

    pub fn snapshot_node(&mut self, node_id: usize) {
        let node = &mut self.nodes[node_id];
        let opaque_node_id = TNode::opaque(&&*node);
        node.has_snapshot = true;
        node.snapshot_handled
            .store(false, std::sync::atomic::Ordering::SeqCst);

        // TODO: handle invalidations other than hover
        if let Some(_existing_snapshot) = self.snapshots.get_mut(&opaque_node_id) {
            // Do nothing
            // TODO: update snapshot
        } else {
            let attrs: Option<Vec<_>> = node.attrs().map(|attrs| {
                attrs
                    .iter()
                    .map(|attr| {
                        let ident = AttrIdentifier {
                            local_name: GenericAtomIdent(attr.name.local.clone()),
                            name: GenericAtomIdent(attr.name.local.clone()),
                            namespace: GenericAtomIdent(attr.name.ns.clone()),
                            prefix: None,
                        };

                        let value = if attr.name.local == local_name!("id") {
                            AttrValue::Atom(Atom::from(&*attr.value))
                        } else if attr.name.local == local_name!("class") {
                            let classes = attr
                                .value
                                .split_ascii_whitespace()
                                .map(Atom::from)
                                .collect();
                            AttrValue::TokenList(attr.value.clone(), classes)
                        } else {
                            AttrValue::String(attr.value.clone())
                        };

                        (ident, value)
                    })
                    .collect()
            });

            let changed_attrs = attrs
                .as_ref()
                .map(|attrs| attrs.iter().map(|attr| attr.0.name.clone()).collect())
                .unwrap_or_default();

            self.snapshots.insert(
                opaque_node_id,
                ServoElementSnapshot {
                    state: Some(node.element_state),
                    attrs,
                    changed_attrs,
                    class_changed: true,
                    id_changed: true,
                    other_attributes_changed: true,
                },
            );
        }
    }

    pub fn snapshot_node_and(&mut self, node_id: usize, cb: impl FnOnce(&mut Node)) {
        self.snapshot_node(node_id);
        cb(&mut self.nodes[node_id]);
    }

    /// Restyle the tree and then relayout it
    pub fn resolve(&mut self) {
        if TDocument::as_node(&&self.nodes[0])
            .first_element_child()
            .is_none()
        {
            println!("No DOM - not resolving");
            return;
        }

        // we need to resolve stylist first since it will need to drive our layout bits
        self.resolve_stylist();

        // Fix up tree for layout (insert anonymous blocks as necessary, etc)
        self.resolve_layout_children();

        // Merge stylo into taffy
        self.flush_styles_to_layout(self.root_element().id);

        // Next we resolve layout with the data resolved by stlist
        self.resolve_layout();
    }

    // Takes (x, y) co-ordinates (relative to the )
    pub fn hit(&self, x: f32, y: f32) -> Option<HitResult> {
        if TDocument::as_node(&&self.nodes[0])
            .first_element_child()
            .is_none()
        {
            println!("No DOM - not resolving");
            return None;
        }

        self.root_element().hit(x, y)
    }

    pub fn focus_next_node(&mut self) -> Option<usize> {
        let focussed_node_id = self.get_focussed_node_id()?;
        let id = self.next_node(&self.nodes[focussed_node_id], |node| node.is_focussable())?;
        self.set_focus_to(id);
        Some(id)
    }

    /// Clear the focussed node
    pub fn clear_focus(&mut self) {
        if let Some(id) = self.focus_node_id {
            self.snapshot_node_and(id, |node| node.blur());
            self.focus_node_id = None;
        }
    }

    pub fn set_mousedown_node_id(&mut self, node_id: Option<usize>) {
        self.mousedown_node_id = node_id;
    }
    pub fn set_focus_to(&mut self, focus_node_id: usize) -> bool {
        if Some(focus_node_id) == self.focus_node_id {
            return false;
        }

        println!("Focussed node {focus_node_id}");

        // Remove focus from the old node
        if let Some(id) = self.focus_node_id {
            self.snapshot_node_and(id, |node| node.blur());
        }

        // Focus the new node
        self.snapshot_node_and(focus_node_id, |node| node.focus());

        self.focus_node_id = Some(focus_node_id);

        true
    }

    pub fn active_node(&mut self) -> bool {
        let Some(hover_node_id) = self.get_hover_node_id() else {
            return false;
        };

        if let Some(active_node_id) = self.active_node_id {
            if active_node_id == hover_node_id {
                return true;
            }
            self.unactive_node();
        }

        let active_node_id = Some(hover_node_id);

        let node_path = self.maybe_node_layout_ancestors(active_node_id);
        for &id in node_path.iter() {
            self.snapshot_node_and(id, |node| node.active());
        }

        self.active_node_id = active_node_id;

        true
    }

    pub fn unactive_node(&mut self) -> bool {
        let Some(active_node_id) = self.active_node_id.take() else {
            return false;
        };

        let node_path = self.maybe_node_layout_ancestors(Some(active_node_id));
        for &id in node_path.iter() {
            self.snapshot_node_and(id, |node| node.unactive());
        }

        true
    }

    pub fn set_hover_to(&mut self, x: f32, y: f32) -> bool {
        let hit = self.hit(x, y);
        let hover_node_id = hit.map(|hit| hit.node_id);

        // Return early if the new node is the same as the already-hovered node
        if hover_node_id == self.hover_node_id {
            return false;
        }

        let old_node_path = self.maybe_node_layout_ancestors(self.hover_node_id);
        let new_node_path = self.maybe_node_layout_ancestors(hover_node_id);
        let same_count = old_node_path
            .iter()
            .zip(&new_node_path)
            .take_while(|(o, n)| o == n)
            .count();
        for &id in old_node_path.iter().skip(same_count) {
            self.snapshot_node_and(id, |node| node.unhover());
        }
        for &id in new_node_path.iter().skip(same_count) {
            self.snapshot_node_and(id, |node| node.hover());
        }

        self.hover_node_id = hover_node_id;

        // Update the cursor
        let cursor = self.get_cursor().unwrap_or_default();
        self.shell_provider.set_cursor(cursor);

        // Request redraw
        self.shell_provider.request_redraw();

        true
    }

    pub fn get_hover_node_id(&self) -> Option<usize> {
        self.hover_node_id
    }

    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.set_stylist_device(make_device(&self.viewport));
        self.scroll_viewport_by(0.0, 0.0); // Clamp scroll offset
    }

    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    pub fn viewport_mut(&mut self) -> ViewportMut<'_> {
        ViewportMut::new(self)
    }

    pub fn zoom_by(&mut self, increment: f32) {
        *self.viewport.zoom_mut() += increment;
        self.set_viewport(self.viewport.clone());
    }

    pub fn zoom_to(&mut self, zoom: f32) {
        *self.viewport.zoom_mut() = zoom;
        self.set_viewport(self.viewport.clone());
    }

    pub fn get_viewport(&self) -> Viewport {
        self.viewport.clone()
    }

    pub fn devtools(&self) -> &DevtoolSettings {
        &self.devtool_settings
    }

    pub fn devtools_mut(&mut self) -> &mut DevtoolSettings {
        &mut self.devtool_settings
    }

    pub fn is_animating(&self) -> bool {
        self.is_animating
    }

    /// Update the device and reset the stylist to process the new size
    pub fn set_stylist_device(&mut self, device: Device) {
        let origins = {
            let guard = &self.guard;
            let guards = StylesheetGuards {
                author: &guard.read(),
                ua_or_user: &guard.read(),
            };
            self.stylist.set_device(device, &guards)
        };
        self.stylist.force_stylesheet_origins_dirty(origins);
    }

    pub fn stylist_device(&mut self) -> &Device {
        self.stylist.device()
    }

    /// Ensure that the layout_children field is populated for all nodes
    pub fn resolve_layout_children(&mut self) {
        resolve_layout_children_recursive(self, self.root_node().id);

        fn resolve_layout_children_recursive(doc: &mut BaseDocument, node_id: usize) {
            // if doc.nodes[node_id].layout_children.borrow().is_none() {
            let mut layout_children = Vec::new();
            let mut anonymous_block: Option<usize> = None;
            collect_layout_children(doc, node_id, &mut layout_children, &mut anonymous_block);

            // Recurse into newly collected layout children
            for child_id in layout_children.iter().copied() {
                resolve_layout_children_recursive(doc, child_id);
                doc.nodes[child_id].layout_parent.set(Some(node_id));
            }

            *doc.nodes[node_id].layout_children.borrow_mut() = Some(layout_children.clone());
            *doc.nodes[node_id].paint_children.borrow_mut() = Some(layout_children);
            // }
        }
    }

    /// Walk the nodes now that they're properly styled and transfer their styles to the taffy style system
    ///
    /// TODO: update taffy to use an associated type instead of slab key
    /// TODO: update taffy to support traited styles so we don't even need to rely on taffy for storage
    pub fn resolve_layout(&mut self) {
        let size = self.stylist.device().au_viewport_size();

        let available_space = taffy::Size {
            width: AvailableSpace::Definite(size.width.to_f32_px()),
            height: AvailableSpace::Definite(size.height.to_f32_px()),
        };

        let root_element_id = taffy::NodeId::from(self.root_element().id);

        // println!("\n\nRESOLVE LAYOUT\n===========\n");

        taffy::compute_root_layout(self, root_element_id, available_space);
        taffy::round_layout(self, root_element_id);

        // println!("\n\n");
        // taffy::print_tree(self, root_node_id)
    }

    pub fn get_cursor(&self) -> Option<CursorIcon> {
        // todo: cache this on the node itself
        let node = &self.nodes[self.get_hover_node_id()?];

        let style = node.primary_styles()?;
        let keyword = stylo_to_cursor_icon(style.clone_cursor().keyword);

        // Return cursor from style if it is non-auto
        if keyword != CursorIcon::Default {
            return Some(keyword);
        }

        // Return text cursor for text nodes and text inputs
        if node.is_text_node()
            || node
                .element_data()
                .is_some_and(|e| e.text_input_data().is_some())
        {
            return Some(CursorIcon::Text);
        }

        // Use "pointer" cursor if any ancestor is a link
        let mut maybe_node = Some(node);
        while let Some(node) = maybe_node {
            if node.is_link() {
                return Some(CursorIcon::Pointer);
            }

            maybe_node = node.layout_parent.get().map(|node_id| node.with(node_id));
        }

        // Else fallback to default cursor
        Some(CursorIcon::Default)
    }

    /// Scroll a node by given x and y
    /// Will bubble scrolling up to parent node once it can no longer scroll further
    /// If we're already at the root node, bubbles scrolling up to the viewport
    pub fn scroll_node_by(&mut self, node_id: usize, x: f64, y: f64) {
        let Some(node) = self.nodes.get_mut(node_id) else {
            return;
        };

        let is_html_or_body = node.data.downcast_element().is_some_and(|e| {
            let tag = &e.name.local;
            tag == "html" || tag == "body"
        });

        let (can_x_scroll, can_y_scroll) = node
            .primary_styles()
            .map(|styles| {
                (
                    matches!(styles.clone_overflow_x(), Overflow::Scroll | Overflow::Auto),
                    matches!(styles.clone_overflow_y(), Overflow::Scroll | Overflow::Auto)
                        || (styles.clone_overflow_y() == Overflow::Visible && is_html_or_body),
                )
            })
            .unwrap_or((false, false));

        let new_x = node.scroll_offset.x - x;
        let new_y = node.scroll_offset.y - y;

        let mut bubble_x = 0.0;
        let mut bubble_y = 0.0;

        let scroll_width = node.final_layout.scroll_width() as f64;
        let scroll_height = node.final_layout.scroll_height() as f64;

        // If we're past our scroll bounds, transfer remainder of scrolling to parent/viewport
        if !can_x_scroll {
            bubble_x = x
        } else if new_x < 0.0 {
            bubble_x = -new_x;
            node.scroll_offset.x = 0.0;
        } else if new_x > scroll_width {
            bubble_x = scroll_width - new_x;
            node.scroll_offset.x = scroll_width;
        } else {
            node.scroll_offset.x = new_x;
        }

        if !can_y_scroll {
            bubble_y = y
        } else if new_y < 0.0 {
            bubble_y = -new_y;
            node.scroll_offset.y = 0.0;
        } else if new_y > scroll_height {
            bubble_y = scroll_height - new_y;
            node.scroll_offset.y = scroll_height;
        } else {
            node.scroll_offset.y = new_y;
        }

        if bubble_x != 0.0 || bubble_y != 0.0 {
            if let Some(parent) = node.parent {
                self.scroll_node_by(parent, bubble_x, bubble_y);
            } else {
                self.scroll_viewport_by(bubble_x, bubble_y);
            }
        }
    }

    /// Scroll the viewport by the given values
    pub fn scroll_viewport_by(&mut self, x: f64, y: f64) {
        let content_size = self.root_element().final_layout.size;
        let new_scroll = (self.viewport_scroll.x - x, self.viewport_scroll.y - y);
        let window_width = self.viewport.window_size.0 as f64 / self.viewport.scale() as f64;
        let window_height = self.viewport.window_size.1 as f64 / self.viewport.scale() as f64;
        self.viewport_scroll.x = f64::max(
            0.0,
            f64::min(new_scroll.0, content_size.width as f64 - window_width),
        );
        self.viewport_scroll.y = f64::max(
            0.0,
            f64::min(new_scroll.1, content_size.height as f64 - window_height),
        )
    }

    pub fn viewport_scroll(&self) -> kurbo::Point {
        self.viewport_scroll
    }

    pub fn set_viewport_scroll(&mut self, scroll: kurbo::Point) {
        self.viewport_scroll = scroll;
    }

    pub fn find_title_node(&self) -> Option<&Node> {
        TreeTraverser::new(self)
            .find(|node_id| {
                self.nodes[*node_id]
                    .data
                    .is_element_with_tag_name(&local_name!("title"))
            })
            .map(|node_id| &self.nodes[node_id])
    }

    pub(crate) fn compute_is_animating(&self) -> bool {
        TreeTraverser::new(self).any(|node_id| {
            let node = &self.nodes[node_id];
            let Some(element) = node.element_data() else {
                return false;
            };
            if element.name.local == local_name!("canvas") && element.has_attr(local_name!("src")) {
                return true;
            }

            false
        })
    }
}

impl AsRef<BaseDocument> for BaseDocument {
    fn as_ref(&self) -> &BaseDocument {
        self
    }
}

impl AsMut<BaseDocument> for BaseDocument {
    fn as_mut(&mut self) -> &mut BaseDocument {
        self
    }
}
