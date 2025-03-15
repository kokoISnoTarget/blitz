use std::ops::DerefMut;

use super::*;

pub struct Document;
impl Document {
    pub fn new() -> Self {
        Document
    }
    pub fn object<'a>(self, scope: &mut HandleScope<'a>) -> v8::Local<'a, v8::Object> {
        #[cfg(feature = "tracing")]
        tracing::trace!("Document::object");

        let obj = v8::Object::new(scope);

        add_method(scope, &obj, "querySelector", Document::query_selector);

        obj
    }
    fn query_selector(
        scope: &mut HandleScope,
        args: FunctionCallbackArguments,
        mut rv: ReturnValue,
    ) {
        #[cfg(feature = "tracing")]
        tracing::trace!("Document::query_selector");

        let arg = args.get(0);
        let Some(str) = arg.to_string(scope) else {
            return;
        };
        let str = str.to_rust_string_lossy(scope);

        let doc = scope
            .get_slot::<BaseDocument>()
            .expect("BaseDocument should be part of scope");

        let node = match doc.query_selector(&str) {
            Ok(node) => node,
            Err(err) => {
                let err = v8::String::new(scope, &format!("{:?}", err)).unwrap();
                let exception = v8::Exception::syntax_error(scope, err);
                scope.throw_exception(exception);
                return;
            }
        };
        let Some(node_id) = node else {
            return;
        };
        rv.set(Element::new(node_id).object(scope).into());
    }
}
impl GarbageCollected for Document {
    fn trace(&self, _visitor: &v8::cppgc::Visitor) {}
}

/* [Exposed=Window]
interface Document : Node {
  constructor();

  [SameObject] readonly attribute DOMImplementation implementation;
  readonly attribute USVString URL;
  readonly attribute USVString documentURI;
  readonly attribute DOMString compatMode;
  readonly attribute DOMString characterSet;
  readonly attribute DOMString charset; // legacy alias of .characterSet
  readonly attribute DOMString inputEncoding; // legacy alias of .characterSet
  readonly attribute DOMString contentType;

  readonly attribute DocumentType? doctype;
  readonly attribute Element? documentElement;
  HTMLCollection getElementsByTagName(DOMString qualifiedName);
  HTMLCollection getElementsByTagNameNS(DOMString? namespace, DOMString localName);
  HTMLCollection getElementsByClassName(DOMString classNames);

  [CEReactions, NewObject] Element createElement(DOMString localName, optional (DOMString or ElementCreationOptions) options = {});
  [CEReactions, NewObject] Element createElementNS(DOMString? namespace, DOMString qualifiedName, optional (DOMString or ElementCreationOptions) options = {});
  [NewObject] DocumentFragment createDocumentFragment();
  [NewObject] Text createTextNode(DOMString data);
  [NewObject] CDATASection createCDATASection(DOMString data);
  [NewObject] Comment createComment(DOMString data);
  [NewObject] ProcessingInstruction createProcessingInstruction(DOMString target, DOMString data);

  [CEReactions, NewObject] Node importNode(Node node, optional boolean subtree = false);
  [CEReactions] Node adoptNode(Node node);

  [NewObject] Attr createAttribute(DOMString localName);
  [NewObject] Attr createAttributeNS(DOMString? namespace, DOMString qualifiedName);

  [NewObject] Event createEvent(DOMString interface); // legacy

  [NewObject] Range createRange();

  // NodeFilter.SHOW_ALL = 0xFFFFFFFF
  [NewObject] NodeIterator createNodeIterator(Node root, optional unsigned long whatToShow = 0xFFFFFFFF, optional NodeFilter? filter = null);
  [NewObject] TreeWalker createTreeWalker(Node root, optional unsigned long whatToShow = 0xFFFFFFFF, optional NodeFilter? filter = null);
};

[Exposed=Window]
interface XMLDocument : Document {};

dictionary ElementCreationOptions {
  DOMString is;
}; */

/*enum DocumentReadyState { "loading", "interactive", "complete" };
enum DocumentVisibilityState { "visible", "hidden" };
typedef (HTMLScriptElement or SVGScriptElement) HTMLOrSVGScriptElement;

[LegacyOverrideBuiltIns]
partial interface Document {
  static Document parseHTMLUnsafe((TrustedHTML or DOMString) html);

  // resource metadata management
  [PutForwards=href, LegacyUnforgeable] readonly attribute Location? location;
  attribute USVString domain;
  readonly attribute USVString referrer;
  attribute USVString cookie;
  readonly attribute DOMString lastModified;
  readonly attribute DocumentReadyState readyState;

  // DOM tree accessors
  getter object (DOMString name);
  [CEReactions] attribute DOMString title;
  [CEReactions] attribute DOMString dir;
  [CEReactions] attribute HTMLElement? body;
  readonly attribute HTMLHeadElement? head;
  [SameObject] readonly attribute HTMLCollection images;
  [SameObject] readonly attribute HTMLCollection embeds;
  [SameObject] readonly attribute HTMLCollection plugins;
  [SameObject] readonly attribute HTMLCollection links;
  [SameObject] readonly attribute HTMLCollection forms;
  [SameObject] readonly attribute HTMLCollection scripts;
  NodeList getElementsByName(DOMString elementName);
  readonly attribute HTMLOrSVGScriptElement? currentScript; // classic scripts in a document tree only

  // dynamic markup insertion
  [CEReactions] Document open(optional DOMString unused1, optional DOMString unused2); // both arguments are ignored
  WindowProxy? open(USVString url, DOMString name, DOMString features);
  [CEReactions] undefined close();
  [CEReactions] undefined write((TrustedHTML or DOMString)... text);
  [CEReactions] undefined writeln((TrustedHTML or DOMString)... text);

  // user interaction
  readonly attribute WindowProxy? defaultView;
  boolean hasFocus();
  [CEReactions] attribute DOMString designMode;
  [CEReactions] boolean execCommand(DOMString commandId, optional boolean showUI = false, optional DOMString value = "");
  boolean queryCommandEnabled(DOMString commandId);
  boolean queryCommandIndeterm(DOMString commandId);
  boolean queryCommandState(DOMString commandId);
  boolean queryCommandSupported(DOMString commandId);
  DOMString queryCommandValue(DOMString commandId);
  readonly attribute boolean hidden;
  readonly attribute DocumentVisibilityState visibilityState;

  // special event handler IDL attributes that only apply to Document objects
  [LegacyLenientThis] attribute EventHandler onreadystatechange;
  attribute EventHandler onvisibilitychange;

  // also has obsolete members
};
Document includes GlobalEventHandlers; */
