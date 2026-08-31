use crate::{
    sand::{
        AccessibilityRole, AccessibilitySpec, AssetKind, BehaviorBinding, DeclarativeBehavior,
        DefinitionGraph, InputPort, Isolation, OutputPort, ProjectionKind, ProjectionManifest,
        RendererCapability, SAND_ABI_VERSION, SAND_SCHEMA_VERSION, SandAsset, SandCapability,
        SandDefinition, SandElement, SandPackage, SandValue, ValueType,
    },
    style::StyleLayer,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PRIMITIVE_COUNT: usize = 19;
pub const INSTALLED_GALLERY_HTML_PATH: &str = "gallery/index.html";
pub const INSTALLED_GALLERY_CSS_PATH: &str = "gallery/gallery.css";
pub const INSTALLED_GALLERY_JS_PATH: &str = "gallery/gallery.js";
pub const INSTALLED_GALLERY_ABI_JS_PATH: &str = "gallery/sand-abi.js";
pub const INSTALLED_GALLERY_COMPOSITION_JS_PATH: &str = "gallery/composition.js";
pub const INSTALLED_GALLERY_PROBES_JS_PATH: &str = "gallery/probes.js";

pub const INSTALLED_GALLERY_HTML: &str = r#"<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Installed Sand Gallery</title><link rel="stylesheet" href="gallery.css"></head>
<body><main data-lince-gallery data-sand-schema="1" data-sand-abi="1">
<header><button data-popup-probe>Popup refusal probe</button><span class="badge">INSTALLED HTML · declared bridge</span><h1>Primitive Sand Gallery</h1><p>One semantic package, ordinary accessible HTML.</p></header>
<section class="protein" aria-live="polite"><span>Protein input</span><strong data-protein-title>Awaiting mount</strong><small data-protein-description>No Record mounted yet.</small></section>
<section class="grid" aria-label="Primitive Sands">
<p data-sand-definition="text" data-sand-node="text:root">Plain semantic text.</p>
<h2 data-sand-definition="title" data-sand-node="title:root">Aleo semantic title</h2>
<output data-sand-definition="quantity" data-sand-node="quantity:root">−12.50</output>
<button class="icon" data-sand-definition="icon" data-sand-node="icon:root" aria-label="Search" title="Search">⌕</button>
<button data-sand-definition="button" data-sand-node="button:root" data-record="fixture-record">Open Record</button>
<span class="badge" data-sand-definition="badge" data-sand-node="badge:root">Declared</span>
<label data-sand-definition="field" data-sand-node="field:root">Field<input value="Editable value"></label>
<label data-sand-definition="textarea" data-sand-node="textarea:root">Textarea<textarea>Multiple lines remain ordinary text.</textarea></label>
<label data-sand-definition="checkbox" data-sand-node="checkbox:root"><input type="checkbox" checked> Checked</label>
<label data-sand-definition="radio" data-sand-node="radio:root"><input type="radio" name="choice" checked> Selected</label>
<details data-sand-definition="disclosure" data-sand-node="disclosure:root"><summary>Disclosure</summary><p>Visible semantic detail.</p></details>
<label data-sand-definition="select" data-sand-node="select:root">Select<select><option>Normal</option><option>Urgent</option></select></label>
<button data-sand-definition="tooltip" data-sand-node="tooltip:root" aria-describedby="gallery-tooltip">Hover or focus</button><span role="tooltip" id="gallery-tooltip">Bounded help</span>
<label class="invalid" data-sand-definition="validation-message" data-sand-node="validation-message:root">Invalid field<input aria-invalid="true" aria-describedby="validation-error" value="Bad value"><span id="validation-error">A valid value is required.</span></label>
<ul data-sand-definition="list" data-sand-node="list:root"><li>First Record</li><li>Second Record</li></ul>
<div class="row" data-sand-definition="row" data-sand-node="row:root"><span>Row</span><span>Aligned metadata</span></div>
<article data-sand-definition="card" data-sand-node="card:root"><strong>Card</strong><p>Firm paper, no ornamental motion.</p></article>
<section data-sand-definition="panel" data-sand-node="panel:root"><strong>Panel</strong><p>Opaque grouped content.</p></section>
<div class="stack" data-sand-definition="stack" data-sand-node="stack:root"><span>Stack item</span><span>Stack item</span></div>
</section>
<section class="composition" data-composition-workbench aria-labelledby="composition-title">
<span class="badge">RECURSIVE COMPOSITION · same Sand graph</span><h2 id="composition-title">Composition workbench</h2>
<p>Standalone Button and a Button nested twice inside a video-call Castle.</p>
<div class="room" data-sand-definition="video-call-room" data-sand-node="video-call-room:root">
<div data-sand-node="video-call-room:call"><div data-sand-definition="video-call" data-sand-node="video-call:root">
<div class="panel" data-sand-node="video-call:frame">Video call surface</div>
<button data-sand-node="video-call:open" data-composition-output="record-clicked" data-record="fixture-record">Open nested Record</button>
</div></div></div>
<div class="arrows" aria-label="Typed composition bindings"><span>READ Protein.title ──▶ room/call/open.label</span><span>EVENT room.record-clicked ──▶ standalone.record</span><span>WRITE room.record-clicked ══▶ Action record.open</span></div>
<div data-scoped-instances></div><output data-composition-status aria-live="polite">Awaiting scoped instances</output>
<template data-room-template><section id="room-root" aria-labelledby="room-title"><h3 id="room-title">Scoped room instance</h3><label for="room-input">Record</label><input id="room-input" aria-describedby="room-help"><span id="room-help">Instance-local help</span></section></template>
</section>
<pre data-event-log>No event yet.</pre></main><script type="module" src="gallery.js"></script><script type="module" src="probes.js"></script></body></html>"#;

pub const INSTALLED_GALLERY_CSS: &str = r#":root{color-scheme:dark;font:var(--lynx-text-size-body,14px)/var(--lynx-line-height-body,1.3) var(--lynx-font-body,system-ui);background:var(--lynx-surface-canvas,#101814);color:var(--lynx-ink-primary,#edf7f0)}*{box-sizing:border-box}body{margin:0;padding:var(--lynx-space-4,16px);background:var(--lynx-surface-canvas,#101814)}header{display:grid;gap:var(--lynx-space-1,4px)}h1,h2,h3,p{margin:0}h1,h2,h3{font-family:var(--lynx-font-heading,serif)}.grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:var(--lynx-space-2,8px);margin-top:var(--lynx-space-3,12px)}.grid>*{min-width:0}.protein,article,section[data-sand-definition],.stack,.composition{padding:var(--lynx-space-2,8px);background:var(--lynx-surface-raised,#17251d)}.protein{display:grid;margin-top:var(--lynx-space-3,12px)}button,input,textarea,select{font:inherit;color:inherit;background:var(--lynx-surface-input,#20382a);border:var(--lynx-border-hairline,.5px) solid var(--lynx-border,#75d89a);border-radius:var(--lynx-radius-control,2px);padding:var(--lynx-padding-control-y,3px) var(--lynx-padding-control-x,5px)}button:hover{background:var(--lynx-surface-hover,#294736)}button:focus-visible,input:focus-visible,textarea:focus-visible,select:focus-visible{outline:2px solid var(--lynx-focus,#9ef0ba);outline-offset:2px}button:active{background:var(--lynx-surface-active,#31543f)}label{display:grid;gap:var(--lynx-space-1,4px)}textarea{resize:vertical}.badge{display:inline-block;width:max-content;padding:2px var(--lynx-space-1,4px);border:var(--lynx-border-hairline,.5px) dashed var(--lynx-border,#75d89a);border-radius:var(--lynx-radius-control,2px)}output{font-family:var(--lynx-font-mono,monospace);font-variant-numeric:tabular-nums}.icon{width:2rem}.invalid input{border-color:var(--lynx-danger,#f07878)}.invalid span{color:var(--lynx-danger,#f07878);font-size:12px}.row{display:flex;justify-content:space-between;gap:var(--lynx-space-1,4px)}.stack{display:grid;gap:var(--lynx-space-1,4px)}[role=tooltip]{background:var(--lynx-ink-primary,#edf7f0);color:var(--lynx-surface-canvas,#101814);padding:var(--lynx-space-1,4px)}.composition{display:grid;gap:var(--lynx-space-2,8px);margin-top:var(--lynx-space-4,16px)}.room{border:var(--lynx-border-hairline,.5px) solid var(--lynx-border,#75d89a);padding:var(--lynx-space-2,8px)}.arrows{display:grid;gap:2px;color:var(--lynx-ink-secondary,#b5c9bc);font-family:var(--lynx-font-mono,monospace);font-size:12px}[data-scoped-instances]{display:grid;grid-template-columns:1fr 1fr;gap:var(--lynx-space-2,8px)}[data-scoped-instances]>section{border:var(--lynx-border-hairline,.5px) dashed var(--lynx-border,#75d89a);padding:var(--lynx-space-2,8px)}pre{white-space:pre-wrap;color:var(--lynx-ink-secondary,#b5c9bc)}@media(max-width:480px){.grid,[data-scoped-instances]{grid-template-columns:1fr}}"#;

pub const INSTALLED_GALLERY_ABI_JS: &str = r#"const exact=(value,keys)=>{if(value===null||typeof value!=='object'||Array.isArray(value))throw new TypeError('object required');const actual=Object.keys(value).sort();const expected=[...keys].sort();if(actual.length!==expected.length||actual.some((key,index)=>key!==expected[index]))throw new TypeError('unknown or missing field')};const typed=(value,type)=>{exact(value,['type','value']);if(value.type!==type)throw new TypeError('value type mismatch');if(type==='text'&&typeof value.value!=='string')throw new TypeError('text required');if(type==='record'&&(typeof value.value!=='string'||value.value.length===0))throw new TypeError('Record required');return value.value};export function decodeMount(envelope){exact(envelope,['abi_version','instance_uid','sequence','event']);if(envelope.abi_version!==1||!Number.isSafeInteger(envelope.sequence)||envelope.sequence<1||typeof envelope.instance_uid!=='string')throw new TypeError('invalid envelope');const event=envelope.event;exact(event,['event','definition','inputs','configuration','presented']);if(event.event!=='mount'||typeof event.presented!=='boolean')throw new TypeError('mount required');exact(event.definition,['uid','revision']);if(event.definition.uid!=='button'||event.definition.revision!==1)throw new TypeError('definition mismatch');exact(event.inputs,['label','description','record']);exact(event.configuration,[]);return{label:typed(event.inputs.label,'text'),description:typed(event.inputs.description,'text'),record:typed(event.inputs.record,'record')}}"#;

pub const INSTALLED_GALLERY_COMPOSITION_JS: &str = r#"const references=['for','aria-labelledby','aria-describedby','aria-controls','aria-owns'];const listeners=new WeakMap;const scope=(instance,local)=>`${instance}--${local.replace(/[^A-Za-z0-9_-]/g,'-')}`;function instantiate(template,instance){const fragment=template.content.cloneNode(true);const identities=new Map;for(const node of fragment.querySelectorAll('[id]')){const local=node.id;const scoped=scope(instance,local);identities.set(local,scoped);node.id=scoped}for(const node of fragment.querySelectorAll('*'))for(const attribute of references)if(node.hasAttribute(attribute))node.setAttribute(attribute,node.getAttribute(attribute).split(/\s+/).map(value=>identities.get(value)||value).join(' '));fragment.firstElementChild.dataset.instanceUid=instance;return{fragment,ids:[...identities.values()]}}export function mountComposition(root,emit){const template=root.querySelector('[data-room-template]');const destination=root.querySelector('[data-scoped-instances]');const status=root.querySelector('[data-composition-status]');const ids=[];for(const instance of ['room-native','room-html']){const mounted=instantiate(template,instance);ids.push(...mounted.ids);destination.append(mounted.fragment)}const handler=event=>{const source=event.target.closest('[data-composition-output]');if(source)emit({op:'emit_event',event:source.dataset.compositionOutput,payload_json:JSON.stringify({record_uid:source.dataset.record,node_path:['call','open']})})};destination.addEventListener('click',handler);listeners.set(destination,handler);const unique=new Set(ids).size===ids.length;status.textContent=`${destination.children.length} instances · ${ids.length} scoped DOM ids · ${unique?'unique':'COLLISION'}`;window.__linceCompositionFacts={instances:destination.children.length,ids:ids.length,unique};return`${destination.children.length}/${ids.length} scoped ${unique?'unique':'COLLISION'}`}export function teardownComposition(root){const destination=root.querySelector('[data-scoped-instances]');const handler=listeners.get(destination);if(handler)destination.removeEventListener('click',handler);listeners.delete(destination);destination.replaceChildren();root.querySelector('[data-composition-status]').textContent='unmounted';delete window.__linceCompositionFacts}"#;

pub const INSTALLED_GALLERY_JS: &str = r#"import{decodeMount}from'./sand-abi.js';import{mountComposition,teardownComposition}from'./composition.js';const root=document.querySelector('[data-lince-gallery]');const log=root.querySelector('[data-event-log]');let sequence=0;let abiRefusals=0;let storageState='unavailable';let styleState='unprojected';let mountedTitle='awaiting';let compositionState='awaiting';let compositionTeardowns=0;try{sequence=Number(localStorage.getItem('sequence')||0);localStorage.setItem('sequence',String(sequence));storageState='available'}catch(error){storageState=`refused:${error.name}`}function setTitle(){document.title=`Installed Sand Gallery · ABI v1 · ABI refusals ${abiRefusals} · Protein ${mountedTitle} · composition ${compositionState} · teardown ${compositionTeardowns} · storage ${storageState} · style ${styleState}`}function bridge(request){sequence+=1;try{localStorage.setItem('sequence',String(sequence))}catch(error){storageState=`refused:${error.name}`}setTitle();if(typeof window.linceBridge!=='function'){log.textContent='REFUSED: bridge absent';return}window.linceBridge(JSON.stringify({schema_version:1,sequence,request}))}window.__linceReply=value=>{log.textContent=JSON.stringify(value,null,2)};window.__linceApplyStyle=(contract,css)=>{document.documentElement.style.cssText=css;document.documentElement.dataset.linceStyleContract=String(contract);styleState=`v${contract}:${getComputedStyle(document.documentElement).getPropertyValue('--lynx-accent').trim()||'missing'}`;setTitle()};window.__linceMount=envelope=>{try{const decoded=decodeMount(envelope);root.querySelector('[data-protein-title]').textContent=decoded.label;root.querySelector('[data-protein-description]').textContent=decoded.description;mountedTitle=decoded.label;setTitle();return true}catch(error){abiRefusals+=1;log.textContent=`REFUSED: ${error.message}`;setTitle();return false}};root.querySelector('[data-sand-definition="button"]').addEventListener('click',event=>{const record=event.currentTarget.dataset.record;bridge({op:'emit_event',event:'record-clicked',payload_json:JSON.stringify({record_uid:record})})});root.querySelector('[data-popup-probe]').addEventListener('click',()=>window.open('https://example.com/'));window.__linceProbe=()=>{bridge({op:'protein_subscribe',subscription_id:'visible-records',protein_id:'current-records',columns:['uid','title','description']});root.querySelector('[data-sand-definition="button"]').click();bridge({op:'request_action',action:'record.open',payload_json:'{"record_uid":"fixture-record"}'})};window.__linceRefusalProbe=()=>bridge({op:'unknown_operation'});compositionState=mountComposition(root,request=>bridge(request));teardownComposition(root);compositionTeardowns+=1;compositionState=mountComposition(root,request=>bridge(request));setTimeout(window.__linceProbe,250);setTimeout(()=>root.querySelector('[data-composition-output]')?.click(),350);setInterval(()=>bridge({op:'emit_event',event:'record-clicked',payload_json:'{"record_uid":"off-camera-heartbeat"}'}),500);setTitle();"#;

pub const INSTALLED_GALLERY_PROBES_JS: &str = r#"let mediaState='starting';async function startLoopback(){try{const canvas=document.createElement('canvas');canvas.width=32;canvas.height=32;const context=canvas.getContext('2d');const timer=setInterval(()=>{context.fillStyle='#3730A3';context.fillRect(0,0,32,32)},100);const stream=canvas.captureStream(10);const sender=new RTCPeerConnection({iceServers:[]});const receiver=new RTCPeerConnection({iceServers:[]});sender.onicecandidate=event=>event.candidate&&receiver.addIceCandidate(event.candidate);receiver.onicecandidate=event=>event.candidate&&sender.addIceCandidate(event.candidate);receiver.ontrack=()=>{mediaState='loopback-live'};for(const track of stream.getTracks())sender.addTrack(track,stream);const offer=await sender.createOffer();await sender.setLocalDescription(offer);await receiver.setRemoteDescription(offer);const answer=await receiver.createAnswer();await receiver.setLocalDescription(answer);await sender.setRemoteDescription(answer);window.__linceWebRtc={canvas,stream,sender,receiver,timer}}catch(error){mediaState=`failed:${error.name}`}}fetch('https://example.com/',{mode:'no-cors'}).catch(()=>{});startLoopback();window.__linceWebRtcTitle=setInterval(()=>{if(!document.title.includes('WebRTC'))document.title=`${document.title} · WebRTC ${mediaState}`},50);"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GalleryVisualState {
    Default,
    Hover,
    FocusVisible,
    Active,
    Selected,
    Disabled,
    ReadOnly,
    Invalid,
    Loading,
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveGalleryState {
    focus: usize,
    state: GalleryVisualState,
    field_value: String,
    checked: bool,
    disclosure_open: bool,
    selected_option: usize,
    activations: u64,
    emitted_events: u64,
    last_event: String,
}

impl PrimitiveGalleryState {
    pub fn new() -> Self {
        Self {
            focus: 4,
            state: GalleryVisualState::FocusVisible,
            field_value: "Editable value".into(),
            checked: true,
            disclosure_open: true,
            selected_option: 0,
            activations: 0,
            emitted_events: 0,
            last_event: "none".into(),
        }
    }

    pub fn definitions(&self) -> &'static [PrimitiveSpec] {
        &PRIMITIVES
    }

    pub fn focused(&self) -> &'static PrimitiveSpec {
        &PRIMITIVES[self.focus]
    }

    pub fn focused_index(&self) -> usize {
        self.focus
    }

    pub fn state(&self) -> GalleryVisualState {
        self.state
    }

    pub fn activations(&self) -> u64 {
        self.activations
    }

    pub fn emitted_events(&self) -> u64 {
        self.emitted_events
    }

    pub fn last_event(&self) -> &str {
        &self.last_event
    }

    pub fn focus_next(&mut self, reverse: bool) {
        self.focus = if reverse {
            self.focus.checked_sub(1).unwrap_or(PRIMITIVES.len() - 1)
        } else {
            (self.focus + 1) % PRIMITIVES.len()
        };
        self.state = GalleryVisualState::FocusVisible;
    }

    pub fn focus_at(&mut self, index: usize, state: GalleryVisualState) {
        if index < PRIMITIVES.len() {
            self.focus = index;
            self.state = state;
        }
    }

    pub fn cycle_state(&mut self) {
        self.state = match self.state {
            GalleryVisualState::Default => GalleryVisualState::Hover,
            GalleryVisualState::Hover => GalleryVisualState::FocusVisible,
            GalleryVisualState::FocusVisible => GalleryVisualState::Active,
            GalleryVisualState::Active => GalleryVisualState::Selected,
            GalleryVisualState::Selected => GalleryVisualState::Disabled,
            GalleryVisualState::Disabled => GalleryVisualState::ReadOnly,
            GalleryVisualState::ReadOnly => GalleryVisualState::Invalid,
            GalleryVisualState::Invalid => GalleryVisualState::Loading,
            GalleryVisualState::Loading => GalleryVisualState::Empty,
            GalleryVisualState::Empty => GalleryVisualState::Default,
        };
    }

    pub fn activate(&mut self) {
        if matches!(
            self.state,
            GalleryVisualState::Disabled
                | GalleryVisualState::ReadOnly
                | GalleryVisualState::Loading
        ) {
            return;
        }
        self.activations = self.activations.saturating_add(1);
        match self.focused().uid {
            "button" => {
                self.emitted_events = self.emitted_events.saturating_add(1);
                self.last_event = "record-clicked".into();
            }
            "checkbox" => self.checked = !self.checked,
            "disclosure" => self.disclosure_open = !self.disclosure_open,
            "select" => self.selected_option = (self.selected_option + 1) % 2,
            _ => {}
        }
        self.state = GalleryVisualState::Active;
    }

    pub fn input_text(&mut self, text: &str) {
        if matches!(self.focused().uid, "field" | "textarea")
            && !matches!(
                self.state,
                GalleryVisualState::Disabled
                    | GalleryVisualState::ReadOnly
                    | GalleryVisualState::Loading
            )
        {
            self.field_value.push_str(text);
            self.activations = self.activations.saturating_add(1);
        }
    }

    pub fn exercise(&mut self) {
        self.focus = PRIMITIVES
            .iter()
            .position(|primitive| primitive.uid == "button")
            .unwrap_or_default();
        self.state = GalleryVisualState::FocusVisible;
        self.activate();
        for _ in 0..GalleryVisualState::Empty as usize + 1 {
            self.cycle_state();
        }
        self.state = GalleryVisualState::FocusVisible;
    }

    pub fn status_line(&self) -> String {
        format!(
            "focus {} · state {:?} · actions {} · events {} · last {} · field {} · checked {} · disclosure {} · option {}",
            self.focused().label,
            self.state,
            self.activations,
            self.emitted_events,
            self.last_event,
            self.field_value,
            self.checked,
            self.disclosure_open,
            self.selected_option + 1,
        )
    }
}

impl Default for PrimitiveGalleryState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimitiveSpec {
    pub uid: &'static str,
    pub label: &'static str,
    pub element: SandElement,
    pub role: AccessibilityRole,
    pub interactive: bool,
}

pub const PRIMITIVES: [PrimitiveSpec; PRIMITIVE_COUNT] = [
    PrimitiveSpec {
        uid: "text",
        label: "Text",
        element: SandElement::Text,
        role: AccessibilityRole::Text,
        interactive: false,
    },
    PrimitiveSpec {
        uid: "title",
        label: "Title",
        element: SandElement::Heading,
        role: AccessibilityRole::Heading,
        interactive: false,
    },
    PrimitiveSpec {
        uid: "quantity",
        label: "Quantity",
        element: SandElement::Quantity,
        role: AccessibilityRole::Status,
        interactive: false,
    },
    PrimitiveSpec {
        uid: "icon",
        label: "Icon button",
        element: SandElement::Icon,
        role: AccessibilityRole::Button,
        interactive: true,
    },
    PrimitiveSpec {
        uid: "button",
        label: "Button",
        element: SandElement::Button,
        role: AccessibilityRole::Button,
        interactive: true,
    },
    PrimitiveSpec {
        uid: "badge",
        label: "Badge",
        element: SandElement::Badge,
        role: AccessibilityRole::Status,
        interactive: false,
    },
    PrimitiveSpec {
        uid: "field",
        label: "Field",
        element: SandElement::Field,
        role: AccessibilityRole::TextInput,
        interactive: true,
    },
    PrimitiveSpec {
        uid: "textarea",
        label: "Textarea",
        element: SandElement::Textarea,
        role: AccessibilityRole::MultilineTextInput,
        interactive: true,
    },
    PrimitiveSpec {
        uid: "checkbox",
        label: "Checkbox",
        element: SandElement::Checkbox,
        role: AccessibilityRole::Checkbox,
        interactive: true,
    },
    PrimitiveSpec {
        uid: "radio",
        label: "Radio",
        element: SandElement::Radio,
        role: AccessibilityRole::Radio,
        interactive: true,
    },
    PrimitiveSpec {
        uid: "disclosure",
        label: "Disclosure",
        element: SandElement::Disclosure,
        role: AccessibilityRole::Disclosure,
        interactive: true,
    },
    PrimitiveSpec {
        uid: "select",
        label: "Select menu",
        element: SandElement::Select,
        role: AccessibilityRole::Select,
        interactive: true,
    },
    PrimitiveSpec {
        uid: "tooltip",
        label: "Tooltip",
        element: SandElement::Tooltip,
        role: AccessibilityRole::Tooltip,
        interactive: true,
    },
    PrimitiveSpec {
        uid: "validation-message",
        label: "Validation message",
        element: SandElement::ValidationMessage,
        role: AccessibilityRole::Alert,
        interactive: false,
    },
    PrimitiveSpec {
        uid: "list",
        label: "List",
        element: SandElement::List,
        role: AccessibilityRole::List,
        interactive: false,
    },
    PrimitiveSpec {
        uid: "row",
        label: "Row",
        element: SandElement::Row,
        role: AccessibilityRole::Row,
        interactive: false,
    },
    PrimitiveSpec {
        uid: "card",
        label: "Card",
        element: SandElement::Card,
        role: AccessibilityRole::Article,
        interactive: false,
    },
    PrimitiveSpec {
        uid: "panel",
        label: "Panel",
        element: SandElement::Panel,
        role: AccessibilityRole::Group,
        interactive: false,
    },
    PrimitiveSpec {
        uid: "stack",
        label: "Stack",
        element: SandElement::Stack,
        role: AccessibilityRole::Group,
        interactive: false,
    },
];

pub fn primitive_gallery_package() -> SandPackage {
    let definitions = PRIMITIVES
        .iter()
        .map(|spec| {
            let definition = definition_for(*spec);
            (definition.uid.clone(), definition)
        })
        .collect();
    SandPackage {
        schema_version: SAND_SCHEMA_VERSION,
        uid: "lince-primitive-gallery".into(),
        graph: DefinitionGraph {
            schema_version: SAND_SCHEMA_VERSION,
            definitions,
        },
        assets: vec![
            SandAsset::first_party(
                INSTALLED_GALLERY_HTML_PATH,
                AssetKind::Html,
                "text/html",
                INSTALLED_GALLERY_HTML.as_bytes(),
            ),
            SandAsset::first_party(
                INSTALLED_GALLERY_CSS_PATH,
                AssetKind::Css,
                "text/css",
                INSTALLED_GALLERY_CSS.as_bytes(),
            ),
            SandAsset::first_party(
                INSTALLED_GALLERY_JS_PATH,
                AssetKind::JavaScriptModule,
                "text/javascript",
                INSTALLED_GALLERY_JS.as_bytes(),
            ),
            SandAsset::first_party(
                INSTALLED_GALLERY_ABI_JS_PATH,
                AssetKind::JavaScriptModule,
                "text/javascript",
                INSTALLED_GALLERY_ABI_JS.as_bytes(),
            ),
            SandAsset::first_party(
                INSTALLED_GALLERY_COMPOSITION_JS_PATH,
                AssetKind::JavaScriptModule,
                "text/javascript",
                INSTALLED_GALLERY_COMPOSITION_JS.as_bytes(),
            ),
            SandAsset::first_party(
                INSTALLED_GALLERY_PROBES_JS_PATH,
                AssetKind::JavaScriptModule,
                "text/javascript",
                INSTALLED_GALLERY_PROBES_JS.as_bytes(),
            ),
        ],
    }
}

pub fn installed_gallery_asset(path: &str) -> Option<(&'static [u8], &'static str)> {
    match path {
        "/index.html" | "/gallery/index.html" => {
            Some((INSTALLED_GALLERY_HTML.as_bytes(), "text/html"))
        }
        "/gallery.css" | "/gallery/gallery.css" => {
            Some((INSTALLED_GALLERY_CSS.as_bytes(), "text/css"))
        }
        "/gallery.js" | "/gallery/gallery.js" => {
            Some((INSTALLED_GALLERY_JS.as_bytes(), "text/javascript"))
        }
        "/sand-abi.js" | "/gallery/sand-abi.js" => {
            Some((INSTALLED_GALLERY_ABI_JS.as_bytes(), "text/javascript"))
        }
        "/composition.js" | "/gallery/composition.js" => Some((
            INSTALLED_GALLERY_COMPOSITION_JS.as_bytes(),
            "text/javascript",
        )),
        "/probes.js" | "/gallery/probes.js" => {
            Some((INSTALLED_GALLERY_PROBES_JS.as_bytes(), "text/javascript"))
        }
        _ => None,
    }
}

fn definition_for(spec: PrimitiveSpec) -> SandDefinition {
    let mut inputs = vec![InputPort {
        name: "value".into(),
        value_type: value_type(spec.element),
        required: false,
        default: Some(default_value(spec.element)),
    }];
    if spec.uid == "button" {
        inputs = vec![
            InputPort {
                name: "label".into(),
                value_type: ValueType::Text,
                required: true,
                default: None,
            },
            InputPort {
                name: "description".into(),
                value_type: ValueType::Text,
                required: true,
                default: None,
            },
            InputPort {
                name: "record".into(),
                value_type: ValueType::Record,
                required: true,
                default: None,
            },
        ];
    }
    let outputs = output_for(spec.element);
    let capabilities = if spec.uid == "button" {
        BTreeSet::from([
            SandCapability::ProteinRead,
            SandCapability::EmitEvent {
                event: "record-clicked".into(),
            },
            SandCapability::LocalStorage,
        ])
    } else {
        BTreeSet::new()
    };
    let behaviors = if spec.uid == "button" {
        vec![BehaviorBinding::Declarative {
            behavior: DeclarativeBehavior::EmitEvent {
                source_output: "pressed".into(),
                event: "record-clicked".into(),
            },
        }]
    } else {
        Vec::new()
    };
    SandDefinition {
        uid: spec.uid.into(),
        revision: 1,
        display_name: spec.label.into(),
        element: spec.element,
        inputs,
        outputs,
        children: Vec::new(),
        connections: Vec::new(),
        exports: Vec::new(),
        behaviors,
        configuration: Vec::new(),
        style: StyleLayer::default(),
        accessibility: AccessibilitySpec {
            role: spec.role,
            label: spec.label.into(),
            description: None,
            live: matches!(
                spec.role,
                AccessibilityRole::Status | AccessibilityRole::Alert
            ),
        },
        capabilities: capabilities.clone(),
        projections: vec![
            ProjectionManifest {
                key: "native-retained".into(),
                kind: ProjectionKind::NativeRetained,
                isolation: Isolation::Trusted,
                required: BTreeSet::from([
                    RendererCapability::RetainedControls,
                    RendererCapability::Accessibility,
                ]),
                capabilities: capabilities.clone(),
                assets: Vec::new(),
                projected_nodes: BTreeSet::from(["root".into()]),
            },
            ProjectionManifest {
                key: "installed-html".into(),
                kind: ProjectionKind::InstalledHtml,
                isolation: Isolation::InstalledHtml,
                required: BTreeSet::from([
                    RendererCapability::ExternalHtml,
                    RendererCapability::Accessibility,
                ]),
                capabilities,
                assets: vec![
                    INSTALLED_GALLERY_HTML_PATH.into(),
                    INSTALLED_GALLERY_CSS_PATH.into(),
                    INSTALLED_GALLERY_JS_PATH.into(),
                    INSTALLED_GALLERY_ABI_JS_PATH.into(),
                    INSTALLED_GALLERY_COMPOSITION_JS_PATH.into(),
                    INSTALLED_GALLERY_PROBES_JS_PATH.into(),
                ],
                projected_nodes: BTreeSet::from(["root".into()]),
            },
        ],
    }
}

fn value_type(element: SandElement) -> ValueType {
    match element {
        SandElement::Quantity => ValueType::Number,
        SandElement::Checkbox | SandElement::Disclosure => ValueType::Boolean,
        _ => ValueType::Text,
    }
}

fn default_value(element: SandElement) -> SandValue {
    match value_type(element) {
        ValueType::Number => SandValue::Number(-12.5),
        ValueType::Boolean => SandValue::Boolean(true),
        _ => SandValue::Text("Example".into()),
    }
}

fn output_for(element: SandElement) -> Vec<OutputPort> {
    match element {
        SandElement::Button | SandElement::Icon => vec![OutputPort {
            name: "pressed".into(),
            value_type: ValueType::Record,
        }],
        SandElement::Field | SandElement::Textarea | SandElement::Radio | SandElement::Select => {
            vec![OutputPort {
                name: "changed".into(),
                value_type: ValueType::Text,
            }]
        }
        SandElement::Checkbox | SandElement::Disclosure => vec![OutputPort {
            name: "changed".into(),
            value_type: ValueType::Boolean,
        }],
        _ => Vec::new(),
    }
}

pub fn gallery_mount_envelope() -> serde_json::Value {
    serde_json::json!({
        "abi_version": SAND_ABI_VERSION,
        "instance_uid": "installed-button-1",
        "sequence": 1,
        "event": {
            "event": "mount",
            "definition": { "uid": "button", "revision": 1 },
            "inputs": {
                "label": { "type": "text", "value": "Build the Box" },
                "description": { "type": "text", "value": "Protein.record.description arrived through the Sand ABI." },
                "record": { "type": "record", "value": "fixture-record" }
            },
            "configuration": {},
            "presented": true
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gallery_package_has_every_primitive_and_valid_assets() {
        let package = primitive_gallery_package();
        package.validate().unwrap();
        assert_eq!(package.graph.definitions.len(), PRIMITIVE_COUNT);
        for primitive in PRIMITIVES {
            assert!(
                INSTALLED_GALLERY_HTML
                    .contains(&format!("data-sand-definition=\"{}\"", primitive.uid))
            );
        }
    }

    #[test]
    fn retained_gallery_is_keyboard_usable_and_emits_typed_event() {
        let mut gallery = PrimitiveGalleryState::new();
        gallery.activate();
        assert_eq!(gallery.last_event(), "record-clicked");
        assert_eq!(gallery.emitted_events(), 1);
        gallery.focus_next(false);
        assert_eq!(gallery.focused().uid, "badge");
    }

    #[test]
    fn installed_gallery_has_external_assets_and_no_inline_handlers() {
        assert!(INSTALLED_GALLERY_HTML.contains("type=\"module\""));
        assert!(INSTALLED_GALLERY_JS.contains("decodeMount"));
        assert!(INSTALLED_GALLERY_ABI_JS.contains("unknown or missing field"));
        assert!(INSTALLED_GALLERY_HTML.contains("video-call-room:root"));
        assert!(INSTALLED_GALLERY_HTML.contains("data-composition-output"));
        assert!(INSTALLED_GALLERY_COMPOSITION_JS.contains("teardownComposition"));
        assert!(INSTALLED_GALLERY_JS.contains("compositionTeardowns+=1"));
        assert_eq!(
            installed_gallery_asset("/sand-abi.js"),
            Some((INSTALLED_GALLERY_ABI_JS.as_bytes(), "text/javascript"))
        );
        assert_eq!(
            installed_gallery_asset("/composition.js"),
            Some((
                INSTALLED_GALLERY_COMPOSITION_JS.as_bytes(),
                "text/javascript"
            ))
        );
        assert!(!INSTALLED_GALLERY_HTML.contains("<style"));
        assert!(!INSTALLED_GALLERY_HTML.contains("<script>"));
        assert!(!INSTALLED_GALLERY_HTML.contains("onclick="));
    }

    #[test]
    fn installed_gallery_refuses_a_missing_static_module_import() {
        let mut package = primitive_gallery_package();
        let index = package
            .assets
            .iter()
            .position(|asset| asset.path == INSTALLED_GALLERY_JS_PATH)
            .unwrap();
        package.assets[index] = SandAsset::first_party(
            INSTALLED_GALLERY_JS_PATH,
            AssetKind::JavaScriptModule,
            "text/javascript",
            b"import'./missing.js'".as_slice(),
        );
        assert!(package.validate().is_err());
    }
}
