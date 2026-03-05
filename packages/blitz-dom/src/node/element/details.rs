use crate::mutator::SpecialOp;
use crate::node::element::slot::SlotDocumentMutatorExt;
use crate::node::{
    ShadowRootInit, ShadowRootMode, ShadowRootStylesheet, SlotAssignmentMode, SpecialElementData,
    UiElement,
};
use crate::{Attribute, BaseDocument, DocumentMutator, local_name, qual_name};
use markup5ever::QualName;
use servo_arc::Arc as ServoArc;
use std::str::FromStr;
use style::media_queries::MediaList;
use style::servo_arc;
use style::shared_lock::SharedRwLock;
use style::stylesheets::{Origin, Stylesheet, UrlExtraData};
use url::Url;

#[derive(Debug, Clone)]
pub struct DetailsElement {
    summary_slot: usize,
    content_slot: usize,

    default_summary: usize,
}
pub(crate) fn create_shadow_tree(mutr: &mut DocumentMutator, this_id: usize) {
    let summary_slot = mutr.create_element(
        qual_name!("slot", html),
        vec![Attribute {
            name: qual_name!("name", html),
            value: "".to_string(),
        }],
    );

    let default_summary = mutr.create_element(qual_name!("summary"), vec![]);
    let default_summary_text = mutr.create_text_node("Details Default");

    mutr.append_children(default_summary, &[default_summary_text]);
    mutr.append_children(summary_slot, &[default_summary]);

    let content_slot = mutr.create_element(qual_name!("slot"), vec![]);

    let shadow = mutr
        .attach_shadow(
            this_id,
            ShadowRootInit {
                mode: ShadowRootMode::Closed,
                clonable: false,
                delegates_focus: false,
                reference_target: None,
                serializable: false,
                slot_assignment: SlotAssignmentMode::Manual,
            },
            UiElement::Details,
        )
        .expect("Should be able to attach shadow with know init");

    mutr.append_children(shadow, &[summary_slot, content_slot]);

    mutr.doc.nodes[this_id]
        .element_data_mut()
        .expect("This should be an details element")
        .special_data = SpecialElementData::Details(DetailsElement {
        summary_slot,
        content_slot,
        default_summary,
    });
}

pub(crate) fn update_contents(mutr: &mut DocumentMutator, this_id: usize) {
    let this_node = &mut mutr.doc.nodes[this_id];

    let details_data = this_node
        .element_data()
        .expect("Should be an element")
        .details_data()
        .expect("Should be slot element")
        .clone();

    let children = std::mem::take(&mut this_node.children);
    let maybe_summary_id = children
        .iter()
        .copied()
        .filter(|&child_id| {
            mutr.doc.nodes[child_id]
                .data
                .is_element_with_tag_name(&local_name!(summary))
        })
        .next();
    if let Some(summary_id) = maybe_summary_id {
        mutr.assign(details_data.summary_slot, [summary_id]);
    }

    let mut content_iter = children
        .iter()
        .copied()
        .filter(|&child_id| {
            !mutr.doc.nodes[child_id]
                .data
                .is_element_with_tag_name(&local_name!(summary))
        })
        .inspect(|u| println!("{u}"))
        .collect::<Vec<_>>();

    mutr.doc.nodes[this_id].children = children;
    mutr.assign(details_data.content_slot, content_iter.drain(..));
}

static DETAILS_CSS: &'static str = r"
slot:not([name]) {
    visibility: hidden;
}
:host([open]) slot:not([name]) {
    visibility: revert;
}

summary {
  display: list-item;
  counter-increment: list-item 0;
  list-style: disclosure-closed inside;
}
:host([open]) summary {
  list-style-type: disclosure-open;
}
";
pub fn details_stylesheet(lock: SharedRwLock) -> ShadowRootStylesheet {
    let stylesheet = Stylesheet::from_str(
        DETAILS_CSS,
        UrlExtraData(ServoArc::new(
            Url::from_str("blitz://styles/details.css").expect("Should be an valid Url."),
        )),
        Origin::Author,
        ServoArc::new(lock.wrap(MediaList::empty())),
        lock,
        None,
        None,
        selectors::context::QuirksMode::NoQuirks,
        style::stylesheets::AllowImportRules::Yes,
    );
    ShadowRootStylesheet {
        sheet: ServoArc::new(stylesheet),
    }
}
