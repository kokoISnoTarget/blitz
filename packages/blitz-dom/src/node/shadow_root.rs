use crate::{LocalName, local_name};
use std::fmt::{Debug, Formatter};
use style::author_styles::AuthorStyles;
use style::media_queries::MediaList;
use style::servo_arc::Arc;
use style::shared_lock::SharedRwLockReadGuard;
use style::stylesheets::scope_rule::ImplicitScopeRoot;
use style::stylesheets::{Stylesheet, StylesheetContents, StylesheetInDocument};

/// https://dom.spec.whatwg.org/#interface-shadowroot
pub struct ShadowRootData {
    pub host: usize,
    pub mode: ShadowRootMode,
    pub slot_assignment_mode: SlotAssignmentMode,
    pub delegates_focus: bool,
    pub clonable: bool,
    pub serializable: bool,
    pub declarative: bool,

    pub styles: AuthorStyles<ShadowRootStylesheet>,
}

pub struct ShadowRootStylesheet {
    sheet: Arc<Stylesheet>,
}

impl StylesheetInDocument for ShadowRootStylesheet {
    fn enabled(&self) -> bool {
        self.sheet.enabled()
    }

    fn media<'a>(&'a self, guard: &'a SharedRwLockReadGuard) -> Option<&'a MediaList> {
        self.sheet.media(guard)
    }

    fn contents<'a>(&'a self, guard: &'a SharedRwLockReadGuard) -> &'a StylesheetContents {
        self.sheet.contents(guard)
    }

    fn implicit_scope_root(&self) -> Option<ImplicitScopeRoot> {
        None
    }
}

impl PartialEq for ShadowRootStylesheet {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.sheet, &other.sheet)
    }
}

impl Debug for ShadowRootStylesheet {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.sheet.fmt(f)
    }
}

impl Debug for ShadowRootData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShadowRootData")
            .field("host", &self.host)
            .field("mode", &self.mode)
            .field("slot_assignment_mode", &self.slot_assignment_mode)
            .field("delegates_focus", &self.delegates_focus)
            .field("clonable", &self.clonable)
            .field("serializable", &self.serializable)
            .field("declarative", &self.declarative)
            .finish_non_exhaustive()
    }
}
impl Clone for ShadowRootData {
    fn clone(&self) -> Self {
        todo!()
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum ShadowRootMode {
    Open,
    Closed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SlotAssignmentMode {
    Manual,
    Named,
}

pub struct ShadowRootInit {
    pub mode: ShadowRootMode,
    /// TODO
    pub clonable: bool,
    /// TODO
    pub delegates_focus: bool,
    /// TODO
    pub reference_target: Option<String>,
    /// TODO
    pub serializable: bool,
    /// TODO
    pub slot_assignment: SlotAssignmentMode,
}

#[derive(Debug)]
pub enum ShadowRootNotSupportedError {
    CantHaveShadowAttached,
    DisabledFeature,
    HasNonDeclarativeShadow,
    ModeDoesNotMatch,
}

pub(crate) fn valid_shadow_host_name(name: &LocalName) -> bool {
    // TODO: Allow valid custom element https://dom.spec.whatwg.org/#valid-shadow-host-name
    // https://html.spec.whatwg.org/multipage/custom-elements.html#valid-custom-element-name

    /* "article", "aside", "blockquote", "body", "div", "footer", "h1", "h2", "h3", "h4", "h5", "h6",
    "header", "main", "nav", "p", "section", or "span" */

    matches!(
        *name,
        local_name!("article")
            | local_name!("aside")
            | local_name!("blockquote")
            | local_name!("body")
            | local_name!("div")
            | local_name!("footer")
            | local_name!("h1")
            | local_name!("h2")
            | local_name!("h3")
            | local_name!("h4")
            | local_name!("h5")
            | local_name!("h6")
            | local_name!("header")
            | local_name!("main")
            | local_name!("nav")
            | local_name!("p")
            | local_name!("selection")
            | local_name!("span")
    )
}
