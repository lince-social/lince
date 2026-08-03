use crate::{
    domain::lince_package::PackageManifest,
    sand::{HeadLink, SandWidgetSource, WidgetScript},
};
use maud::{Markup, PreEscaped, html};

pub(crate) const FEATURE_FLAG: &str = "shell";
const LINCE_LOGO_SVG: &str = include_str!("../../static/lince_logo_white.svg");

const STYLE: &str = r#"
:root{
  color-scheme:dark;
  --text-strong:#f6f8fb;
  --text-soft:#c8d0dc;
  --text-muted:#788190;
  --border:rgba(255,255,255,.08);
  --border-strong:rgba(255,255,255,.16);
  --primary-background:#121214;
  --secondary-background:#141416;
  --raised-background:var(--primary-background);
  --primary-ink:#f8fafc;
  --secondary-ink:#a7b4c2;
  --tooltip:#202126;
  --tooltip-ink:#f8fafc;
  --gray:#a7b4c2;
  --accent:#6366f1;
  --focus:#6366f1;
  --transition-fast:160ms ease;
  font-family:"IBM Plex Sans",Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;
  background:transparent;
  color:var(--text-strong);
}
*{box-sizing:border-box}
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent}
button,input{font:inherit}
button{cursor:pointer}
.brand{display:flex;align-items:center;gap:10px;width:100%;height:100%}
.brand-mark{display:grid;place-items:center;width:28px;height:28px;color:var(--text-strong);background:none}
.brand-logo,.brand-logo svg{width:100%;height:auto}.brand-logo svg{display:block}.brand-logo .s0{stroke-width:50!important}
.brand-name{color:var(--text-strong);font-size:1rem;font-weight:600;letter-spacing:.03em}
.operation-box{width:100%;height:100%}
.operation-box__input{width:100%;height:40px;padding:0 14px;border:1px solid var(--border);border-radius:14px;outline:none;background:rgba(18,20,24,.75);color:var(--text-strong);font-size:.92rem;box-shadow:inset 0 1px 0 rgba(255,255,255,.03);transition:border-color var(--transition-fast),background var(--transition-fast),color var(--transition-fast)}
.operation-box__input::placeholder{color:var(--text-muted)}
.operation-box__input:focus{border-color:var(--border-strong);background:rgba(24,27,32,.92)}
.workspace-indicator,.icon-button{border:1px solid var(--border);background:rgba(18,20,24,.75);box-shadow:inset 0 1px 0 rgba(255,255,255,.03);color:var(--text-soft)}
.workspace-indicator{position:absolute;top:0;right:0;display:inline-grid;place-items:center;width:40px;height:40px;padding:0;border-radius:2px;transition:border-color var(--transition-fast),background var(--transition-fast),color var(--transition-fast),transform var(--transition-fast)}
.workspace-indicator:hover,.icon-button:hover{border-color:var(--border-strong);background:rgba(30,32,38,.9);color:var(--text-strong)}
.workspace-indicator svg{width:19px;height:19px}
.workspace-popover{position:absolute;top:50px;right:0;z-index:24;display:none;gap:8px;width:260px;padding:10px;border:1px solid rgba(255,255,255,.08);border-radius:16px;background:rgba(13,15,19,.94);box-shadow:0 20px 46px rgba(0,0,0,.28),inset 0 1px 0 rgba(255,255,255,.04);backdrop-filter:blur(20px)}
.workspace-popover.isOpen{display:grid}.workspace-list{display:grid;grid-template-columns:1fr;gap:8px}.workspace-item{display:grid;grid-template-columns:minmax(0,1fr) 34px 34px;gap:6px;align-items:center}.workspace-item__switch,.workspace-item__edit,.workspace-item__delete{min-height:38px;border:1px solid var(--border);border-radius:2px;background:rgba(18,20,24,.75);color:var(--text-soft)}.workspace-item__switch{display:flex;align-items:center;justify-content:flex-start;min-width:0;padding:0 10px;font-size:.82rem;font-weight:700}.workspace-item__name{width:100%;min-width:0;padding:0;border:0;outline:0;background:transparent;color:var(--text-strong);font:inherit;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;cursor:pointer;pointer-events:none}.workspace-item.is-renaming .workspace-item__name{cursor:text;pointer-events:auto}.workspace-item.is-renaming .workspace-item__switch,.workspace-item__switch:focus-within{border-color:rgba(255,255,255,.18);background:rgba(43,47,56,.96);color:var(--text-strong)}.workspace-item__edit,.workspace-item__delete{display:grid;place-items:center;width:34px;padding:0}.workspace-item__edit svg,.workspace-item__delete svg{width:14px;height:14px}.workspace-item__switch:hover,.workspace-item__switch.is-active,.workspace-item__edit:hover,.workspace-item__delete:hover{border-color:var(--border-strong);background:rgba(30,32,38,.9);color:var(--text-strong)}.workspace-item__delete:disabled{opacity:.28;cursor:not-allowed}
.workspace-popover__footer{display:grid;gap:6px}.workspace-popover__action{display:flex;align-items:center;justify-content:space-between;gap:8px;min-height:34px;border:1px solid var(--border);border-radius:12px;background:rgba(18,20,24,.75);color:var(--text-soft);padding:0 10px;text-align:left;font-size:.78rem}.workspace-popover__action:hover{border-color:var(--border-strong);background:rgba(30,32,38,.9);color:var(--text-strong)}#notifications-count{display:inline-grid;place-items:center;min-width:17px;height:17px;padding:0 4px;border-radius:999px;background:#e8b45b;color:#191b1f;font-size:.68rem;font-weight:700}
.icon-button{display:inline-grid;place-items:center;width:40px;height:40px;padding:0;border-radius:2px;text-decoration:none;transition:transform var(--transition-fast),border-color var(--transition-fast),background var(--transition-fast),color var(--transition-fast)}
.icon-button svg{width:19px;height:19px}
.notification-button{position:relative}.notification-button__mark{font-size:.9rem;font-weight:700}.notification-button__count{position:absolute;right:-3px;bottom:-3px;display:inline-grid;place-items:center;min-width:17px;height:17px;padding:0 4px;border-radius:999px;background:#e8b45b;color:#191b1f;font-size:.68rem;font-weight:700}
.editShell{position:relative;width:100%;height:100%}.editShell>.icon-button{position:absolute;top:0;right:0}.editPopover{position:fixed;right:0;top:0;bottom:0;width:260px;display:none;gap:8px;padding:10px;border:1px solid rgba(255,255,255,.08);border-radius:2px;background:rgba(13,15,19,.94);box-shadow:0 20px 46px rgba(0,0,0,.28),inset 0 1px 0 rgba(255,255,255,.04);backdrop-filter:blur(20px);overflow:auto;overscroll-behavior:contain}
.editPopover.isSelfEditLocked::before{content:"";position:absolute;inset:0;z-index:2;border-radius:inherit;background:rgba(8,10,14,.48);backdrop-filter:saturate(.72);pointer-events:auto;cursor:not-allowed}.editPopover.isSelfEditLocked>*{pointer-events:none;filter:grayscale(.35);opacity:.64}
.editPopover.isOpen{display:grid}.add-card-popover__action{display:flex;align-items:center;gap:10px;width:100%;min-height:44px;padding:8px 10px;border:1px solid rgba(255,255,255,.08);border-radius:12px;background:rgba(18,20,24,.75);color:var(--text-soft);text-align:left}.add-card-popover__action:hover{border-color:var(--border-strong);color:var(--text-strong);background:rgba(30,32,38,.9)}
.add-card-popover__icon{display:grid;place-items:center;width:24px;height:24px;color:var(--text-strong)}.add-card-popover__copy{display:grid;gap:2px}.add-card-popover__copy strong{font-size:.82rem}.add-card-popover__copy small{color:var(--text-muted);font-size:.68rem}
.floating-tag{display:inline-flex;align-items:center;gap:10px;min-height:40px;padding:0 14px;border:1px solid rgba(255,255,255,.08);border-radius:2px;background:rgba(13,15,19,.86);box-shadow:0 20px 46px rgba(0,0,0,.28),inset 0 1px 0 rgba(255,255,255,.04);backdrop-filter:blur(20px);color:var(--text-soft)}
.floating-tag__label{font-size:.78rem}.density-slider{width:118px}.density-control__value{min-width:28px;color:var(--text-strong);font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:.79rem;text-align:right}
.board-zoom-controls{display:inline-flex;align-items:center;gap:2px;width:max-content;height:max-content;padding:2px}.board-zoom-button{flex:0 0 auto}
.editPopover{position:fixed;top:0;right:0;bottom:auto;width:fit-content;min-width:0;max-width:calc(100vw - var(--space-4,10px));height:fit-content;max-height:calc(100vh - var(--space-4,10px));gap:0;padding:var(--space-2,4px);overflow:visible;border:var(--hairline,.5px) solid var(--gray);background:var(--raised-background);box-shadow:var(--lynx-shadow-x,2px) var(--lynx-shadow-y,2px) var(--lynx-shadow-blur,6px) var(--lynx-shadow-color,rgb(0 0 0 / 16%));backdrop-filter:none}
.editPopover.isOpen{display:inline-flex;flex-direction:column;align-items:flex-start;align-content:flex-start}
.editPopover .add-card-popover__action,.editPopover .workspace-popover__action{justify-content:flex-start;width:fit-content;min-height:var(--control-height,25px);padding:var(--control-padding-y,3px) var(--control-padding-x,5px);border:var(--hairline,.5px) solid var(--gray);border-radius:var(--radius-control,2px);background:var(--primary-background);color:var(--primary-ink);font-size:inherit}
.editPopover .add-card-popover__action:hover,.editPopover .workspace-popover__action:hover{border-color:var(--gray);background:var(--secondary-background);color:var(--primary-ink)}
.editPopover .add-card-popover__icon{width:14px;height:14px;color:var(--secondary-ink)}
.editPopover .edit-popover__section{display:flex;flex-direction:column;gap:var(--space-1,2px)}.editPopover .edit-popover__section+.edit-popover__section{margin-top:var(--space-4,10px)}
.editPopover .edit-popover__label{color:var(--secondary-ink);font-size:.72rem;font-weight:600;letter-spacing:.1em;text-transform:uppercase}
.editPopover .edit-popover__row,.editPopover .workspace-popover__footer{display:flex;align-items:center;justify-content:flex-start;flex-wrap:nowrap;gap:var(--space-1,2px)}
.editPopover .edit-popover__row .lynx-button,.editPopover .workspace-popover__footer .lynx-button{width:auto;min-width:var(--control-height,25px);padding:0;justify-content:center}
.editPopover .workspace-list{width:fit-content;gap:var(--space-1,2px)}.editPopover .workspace-item{grid-template-columns:fit-content(12rem) var(--control-height,25px) var(--control-height,25px);gap:var(--space-1,2px)}
.editPopover .workspace-item__switch{min-height:var(--control-height,25px);padding:0;border-radius:var(--radius-control,2px);font-size:inherit;font-weight:400}.editPopover .workspace-item__name{width:fit-content;min-width:8ch;max-width:12rem;field-sizing:content;min-height:var(--control-height,25px);padding:var(--control-padding-y,3px) var(--control-padding-x,5px);border-color:var(--gray);border-radius:var(--radius-control,2px);background:var(--primary-background);color:var(--primary-ink)}
.editPopover .workspace-item__edit,.editPopover .workspace-item__delete{display:inline-flex;width:var(--control-height,25px);min-height:var(--control-height,25px);padding:0;border-radius:var(--radius-control,2px)}
.editPopover #notifications-toggle{position:relative}.editPopover #notifications-count{position:absolute;inset:0;place-items:center;padding:0;border-radius:inherit;background:#e8b45b;color:#191b1f}
.editPopover .floating-tag{min-height:var(--control-height,25px);padding:var(--control-padding-y,3px) var(--control-padding-x,5px);border:0;border-radius:0;background:var(--secondary-background);box-shadow:none;color:var(--primary-ink);backdrop-filter:none}
/* The long-form tutorial styles moved out with the sand itself: the Tutorial
   sand is now the standalone `sand/instinct` package, which owns its own CSS. */
.aiPanel{display:block;width:100%;height:100%;padding:18px;border:1px solid var(--border);border-radius:4px;background:#171a1f;color:var(--text-strong)}.aiPanel h1{font-size:18px;margin:0 0 8px}.aiPanel p{font-size:14px;line-height:1.45;margin:0;color:var(--text-soft)}.aiPanel button{margin-top:14px;height:36px;border-radius:2px;border:1px solid var(--border);background:rgba(18,20,24,.75);color:var(--text-soft);padding:0 14px}
"#;

fn manifest(title: &str, description: &str) -> PackageManifest {
    PackageManifest {
        icon: "◎".into(),
        title: title.into(),
        description: description.into(),
        details: description.into(),
        author: "Lince".into(),
        version: "1.0.0".into(),
        initial_width: 2,
        initial_height: 1,
        requires_server: false,
        permissions: vec!["bridge_state".into(), "shell_board".into()],
    }
}

fn source(
    filename: &'static str,
    title: &str,
    description: &str,
    body: Markup,
    script: &'static str,
) -> SandWidgetSource {
    let body_scripts = if script.trim().is_empty() {
        vec![]
    } else {
        vec![
            WidgetScript::src("/host/static/presentation/board/widget-frame-bootstrap.js"),
            WidgetScript::src("/host/static/presentation/board/LynxDS-components.js"),
            WidgetScript::src("/board/lynx-ui.js"),
            WidgetScript::inline(script),
        ]
    };

    SandWidgetSource {
        filename,
        lang: "en",
        manifest: manifest(title, description),
        head_links: vec![HeadLink {
            rel: "stylesheet",
            href: "/board/lynx-ui.css",
        }],
        inline_styles: vec![STYLE],
        body,
        body_scripts,
    }
}

#[allow(dead_code)]
pub(crate) fn logo_source() -> SandWidgetSource {
    source(
        "lince-shell-logo.html",
        "Lince Logo",
        "Pinned shell logo.",
        html! {
            div class="brand" {
                div class="brand-mark" aria-hidden="true" {
                    div class="brand-logo" { (PreEscaped(LINCE_LOGO_SVG)) }
                }
                span class="brand-name" { "Lince" }
            }
        },
        "",
    )
}

#[allow(dead_code)]
pub(crate) fn workspaces_source() -> SandWidgetSource {
    source(
        "lince-shell-workspaces.html",
        "Lince Workspaces",
        "Pinned workspace controller.",
        html! {
            div class="editShell" {
                button id="current" class="workspace-indicator" type="button" aria-label="Workspace control" {
                    svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
                        circle cx="12" cy="12" r="3" {}
                        path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.12 2.12-.06-.06a1.7 1.7 0 0 0-1.88-.34 1.7 1.7 0 0 0-1.03 1.56V20.3h-3v-.08A1.7 1.7 0 0 0 10.68 18.66a1.7 1.7 0 0 0-1.88.34l-.06.06-2.12-2.12.06-.06A1.7 1.7 0 0 0 7.02 15a1.7 1.7 0 0 0-1.56-1.03h-.08v-3h.08A1.7 1.7 0 0 0 7.02 9.94a1.7 1.7 0 0 0-.34-1.88l-.06-.06 2.12-2.12.06.06a1.7 1.7 0 0 0 1.88.34 1.7 1.7 0 0 0 1.03-1.56v-.08h3v.08a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.88-.34l.06-.06L19.8 8l-.06.06a1.7 1.7 0 0 0-.34 1.88 1.7 1.7 0 0 0 1.56 1.03h.08v3h-.08A1.7 1.7 0 0 0 19.4 15Z" {}
                    }
                }
                div id="workspace-popover" class="workspace-popover" {
                    button id="notifications-toggle" class="workspace-popover__action workspace-popover__action--subtle" type="button" {
                        span { "Notifications" }
                        span id="notifications-count" hidden { "0" }
                    }
                    div id="workspace-list" class="workspace-list" {}
                    div class="workspace-popover__footer" {
                        button class="workspace-popover__action" data-workspace-action="add" { "+ Nova area" }
                        button class="workspace-popover__action" data-workspace-action="import" { "Importar area" }
                        button class="workspace-popover__action" data-workspace-action="export" { "Exportar area" }
                    }
                }
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const current=document.getElementById("current");const popover=document.getElementById("workspace-popover");const list=document.getElementById("workspace-list");const notificationsToggle=document.getElementById("notifications-toggle");const notificationsCount=document.getElementById("notifications-count");let workspaceOpen=false;
function renderWorkspaceMenu(workspace){const ds=window.LynxDS;workspaceOpen=workspace?.open===true;const items=Array.isArray(workspace?.items)?workspace.items:[];list.replaceChildren(...items.map((item)=>{const row=document.createElement("div");row.className=`workspace-item${item.active?" is-active":""}`;const switchButton=document.createElement("div");switchButton.className=`workspace-item__switch${item.active?" is-active":""}`;switchButton.dataset.workspaceAction="switch";switchButton.dataset.workspaceId=String(item.id||"");switchButton.setAttribute("role","button");switchButton.tabIndex=0;const input=document.createElement("input");input.className="workspace-item__name";input.dataset.workspaceNameInput="";input.dataset.workspaceId=String(item.id||"");input.value=String(item.name||item.label||"");input.autocomplete="off";input.spellcheck=false;input.readOnly=true;input.tabIndex=-1;input.setAttribute("aria-label",`Nome de ${item.name||item.label||"area"}`);switchButton.append(input);const editButton=ds.iconButton({className:"workspace-item__edit",icon:"edit",label:`Renomear ${item.name||item.label||"area"}`,dataset:{workspaceAction:"rename",workspaceId:String(item.id||"")}});const deleteButton=ds.iconButton({className:"workspace-item__delete",icon:"delete",label:`Apagar ${item.name||item.label||"area"}`,disabled:item.canDelete===false,dataset:{workspaceAction:"delete",workspaceId:String(item.id||"")}});row.append(switchButton,editButton,deleteButton);return row;}));ds.setPopoverOpen(popover,workspaceOpen);}
withHost((host)=>host.subscribe(({meta})=>{const shell=meta.shell||{};const notificationCount=Number(shell.notifications?.count||0);notificationsCount.hidden=notificationCount<=0;notificationsCount.textContent=String(notificationCount);renderWorkspaceMenu(shell.workspace||{});}));
current?.addEventListener("click",()=>withHost((host)=>host.shell("workspace.toggle",{})));
notificationsToggle?.addEventListener("click",()=>withHost((host)=>host.shell("notifications.toggle",{})));
document.addEventListener("pointerdown",(event)=>{if(!workspaceOpen)return;if(current?.contains(event.target)||popover?.contains(event.target))return;withHost((host)=>host.shell("workspace.close",{}));});
function finishName(input,options={}){if(!input)return;if(options.cancel===true){input.value=input.dataset.originalValue||input.defaultValue||"";}else{const nextName=String(input.value||"").trim();const originalName=String(input.dataset.originalValue||input.defaultValue||"").trim();if(!nextName){input.value=originalName;}else if(nextName!==originalName){withHost((host)=>host.shell("workspace.rename",{workspaceId:input.dataset.workspaceId||"",name:nextName}));}}input.readOnly=true;input.tabIndex=-1;input.closest(".workspace-item")?.classList.remove("is-renaming");}
function focusName(workspaceId){for(const activeInput of list.querySelectorAll("[data-workspace-name-input]:not([readonly])")){if(activeInput.dataset.workspaceId!==workspaceId)finishName(activeInput);}const input=Array.from(list.querySelectorAll("[data-workspace-name-input]")).find((entry)=>entry.dataset.workspaceId===workspaceId);if(input){input.readOnly=false;input.tabIndex=0;input.dataset.originalValue=input.value;input.closest(".workspace-item")?.classList.add("is-renaming");input.focus();input.select();}}
list?.addEventListener("pointerdown",(event)=>{const button=event.target.closest("[data-workspace-action='rename']");if(button?.dataset.workspaceId){event.preventDefault();focusName(button.dataset.workspaceId||"");return;}if(!event.target.closest("[data-workspace-name-input]")){for(const activeInput of list.querySelectorAll("[data-workspace-name-input]:not([readonly])"))finishName(activeInput);}});
list?.addEventListener("click",(event)=>{const input=event.target.closest("[data-workspace-name-input]");if(input){event.stopPropagation();if(input.readOnly)input.blur();return;}const button=event.target.closest("[data-workspace-action][data-workspace-id]");if(!button)return;const action=button.dataset.workspaceAction;if(action==="rename"){focusName(button.dataset.workspaceId||"");return;}withHost((host)=>host.shell(`workspace.${action}`,{workspaceId:button.dataset.workspaceId||""}));});
list?.addEventListener("focusin",(event)=>{const input=event.target.closest("[data-workspace-name-input]");if(input&&!input.readOnly){input.dataset.originalValue=input.value;input.closest(".workspace-item")?.classList.add("is-renaming");}});
list?.addEventListener("keydown",(event)=>{const input=event.target.closest("[data-workspace-name-input]");if(!input)return;if(event.key==="Enter"){event.preventDefault();input.blur();return;}if(event.key==="Escape"){event.preventDefault();finishName(input,{cancel:true});input.blur();}});
list?.addEventListener("blur",(event)=>{const input=event.target.closest("[data-workspace-name-input]");if(!input?.dataset.workspaceId)return;finishName(input);},true);
popover?.addEventListener("click",(event)=>{const button=event.target.closest("[data-workspace-action]:not([data-workspace-id])");if(button)withHost((host)=>host.shell(`workspace.${button.dataset.workspaceAction}`,{}));});
"#,
    )
}

pub(crate) fn edit_source() -> SandWidgetSource {
    source(
        "lince-shell-edit.html",
        "Lince Edit",
        "Pinned edit controls.",
        html! {
            div class="editShell lynx-ui" {
                button id="edit" class="lynx-button lynx-icon-button icon-button" type="button" aria-label="Alternar modo de edicao" {
                    svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
                        circle cx="12" cy="12" r="3" {}
                        path d="M19.4 15a1.7 1.7 0 0 0 .34 1.88l.06.06-2.12 2.12-.06-.06a1.7 1.7 0 0 0-1.88-.34 1.7 1.7 0 0 0-1.03 1.56V20.3h-3v-.08A1.7 1.7 0 0 0 10.68 18.66a1.7 1.7 0 0 0-1.88.34l-.06.06-2.12-2.12.06-.06A1.7 1.7 0 0 0 7.02 15a1.7 1.7 0 0 0-1.56-1.03h-.08v-3h.08A1.7 1.7 0 0 0 7.02 9.94a1.7 1.7 0 0 0-.34-1.88l-.06-.06 2.12-2.12.06.06a1.7 1.7 0 0 0 1.88.34 1.7 1.7 0 0 0 1.03-1.56v-.08h3v.08a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.88-.34l.06-.06L19.8 8l-.06.06a1.7 1.7 0 0 0-.34 1.88 1.7 1.7 0 0 0 1.56 1.03h.08v3h-.08A1.7 1.7 0 0 0 19.4 15Z" {}
                    }
                }
                div id="popover" class="editPopover lynx-box" data-lynx-tooltip-boundary {
                    div class="edit-popover__section" {
                        div class="edit-popover__label" { "Edit sand" }
                        div class="edit-popover__row" {
                            button class="lynx-button lynx-icon-button add-card-popover__action" data-action="edit.self" aria-label="Edit this sand" data-lynx-tooltip="Edit this sand" {
                                svg class="lynx-icon" viewBox="0 0 16 16" aria-hidden="true" { path d="M9.8 3.1 12.9 6.2M11.3 1.9a1.45 1.45 0 0 1 2.1 2.1L5.7 11.7 3 12.4l.7-2.7 7.6-7.8Z" {} }
                            }
                            button class="lynx-button lynx-icon-button add-card-popover__action" data-action="card.add" aria-label="Add card" data-lynx-tooltip="Add card" {
                                svg class="lynx-icon" viewBox="0 0 16 16" aria-hidden="true" { path d="M8 3v10M3 8h10" {} }
                            }
                            button class="lynx-button lynx-icon-button add-card-popover__action" data-action="card.import" aria-label="Import card" data-lynx-tooltip="Import card" {
                                svg class="lynx-icon" viewBox="0 0 16 16" aria-hidden="true" { path d="M8 12V3M4.5 6.5 8 3l3.5 3.5M3 13h10" {} }
                            }
                        }
                    }
                    div class="edit-popover__section" {
                        div class="edit-popover__label" { "Workspace" }
                        div id="workspace-list" class="workspace-list" {}
                        div class="workspace-popover__footer" {
                            button id="notifications-toggle" class="lynx-button lynx-icon-button workspace-popover__action" type="button" aria-label="Notifications" data-lynx-tooltip="Notifications" {
                                svg class="lynx-icon" viewBox="0 0 16 16" aria-hidden="true" { path d="M12 6.5a4 4 0 0 0-8 0c0 4-1.5 4.5-1.5 4.5h11S12 10.5 12 6.5ZM6.5 13h3" {} }
                                span id="notifications-count" hidden { "0" }
                            }
                            button class="lynx-button lynx-icon-button workspace-popover__action" data-workspace-action="add" aria-label="New area" data-lynx-tooltip="New area" {
                                svg class="lynx-icon" viewBox="0 0 16 16" aria-hidden="true" { path d="M8 3v10M3 8h10" {} }
                            }
                            button class="lynx-button lynx-icon-button workspace-popover__action" data-workspace-action="import" aria-label="Import area" data-lynx-tooltip="Import area" {
                                svg class="lynx-icon" viewBox="0 0 16 16" aria-hidden="true" { path d="M8 12V3M4.5 6.5 8 3l3.5 3.5M3 13h10" {} }
                            }
                            button class="lynx-button lynx-icon-button workspace-popover__action" data-workspace-action="export" aria-label="Export area" data-lynx-tooltip="Export area" {
                                svg class="lynx-icon" viewBox="0 0 16 16" aria-hidden="true" { path d="M8 3v9M4.5 6.5 8 3l3.5 3.5M3 13h10" {} }
                            }
                        }
                    }
                }
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const edit=document.getElementById("edit");const popover=document.getElementById("popover");const density=document.getElementById("density");const densityValue=document.getElementById("density-value");let selfEditLocked=false;
const workspaceList=document.getElementById("workspace-list");const notificationsToggle=document.getElementById("notifications-toggle");const notificationsCount=document.getElementById("notifications-count");
function reportPopoverSize(){if(!popover?.classList.contains("isOpen"))return;const width=Math.ceil(popover.scrollWidth);const height=Math.ceil(popover.scrollHeight);window.parent.postMessage({type:"lince:widget-content-size",instanceId:window.frameElement?.dataset?.packageInstanceId||"",payload:{width,height}},"*");}new ResizeObserver(reportPopoverSize).observe(popover);
window.LynxDS?.tooltip(edit,"Edit mode");
withHost((host)=>host.subscribe(({meta})=>{const shell=meta.shell||{};const editing=shell.editMode===true;selfEditLocked=editing&&String(shell.selfEditCardId||"")===String(meta.instanceId||"");edit.classList.toggle("is-active",editing);window.LynxDS?.setPopoverOpen(popover,editing);popover.classList.toggle("isSelfEditLocked",selfEditLocked);popover.setAttribute("aria-disabled",String(selfEditLocked));requestAnimationFrame(reportPopoverSize);if(density&&shell.density)density.value=String(shell.density);if(densityValue&&shell.gridSnap)densityValue.textContent=String(shell.gridSnap);}));
function renderWorkspaceMenu(workspace){const ds=window.LynxDS;const items=Array.isArray(workspace?.items)?workspace.items:[];workspaceList.replaceChildren(...items.map((item)=>{const name=String(item.name||item.label||"area");const row=document.createElement("div");row.className=`workspace-item${item.active?" is-active":""}`;const switchButton=document.createElement("div");switchButton.className=`lynx-button workspace-item__switch${item.active?" is-active":""}`;switchButton.dataset.workspaceAction="switch";switchButton.dataset.workspaceId=String(item.id||"");switchButton.setAttribute("role","button");switchButton.tabIndex=0;const input=document.createElement("input");input.className="lynx-input workspace-item__name";input.dataset.workspaceNameInput="";input.dataset.workspaceId=String(item.id||"");input.value=name;input.autocomplete="off";input.spellcheck=false;input.readOnly=true;input.tabIndex=-1;input.setAttribute("aria-label",`Workspace name: ${name}`);switchButton.append(input);const editLabel=`Rename ${name}`;const editButton=ds.iconButton({className:"lynx-button lynx-icon-button workspace-item__edit",icon:"edit",label:editLabel,dataset:{workspaceAction:"rename",workspaceId:String(item.id||"")}});editButton.dataset.lynxTooltip=editLabel;const deleteLabel=`Delete ${name}`;const deleteButton=ds.iconButton({className:"lynx-button lynx-icon-button workspace-item__delete",icon:"delete",label:deleteLabel,disabled:item.canDelete===false,dataset:{workspaceAction:"delete",workspaceId:String(item.id||"")}});deleteButton.dataset.lynxTooltip=deleteLabel;row.append(switchButton,editButton,deleteButton);return row;}));requestAnimationFrame(reportPopoverSize);}
withHost((host)=>host.subscribe(({meta})=>{const shell=meta.shell||{};const notificationCount=Number(shell.notifications?.count||0);notificationsCount.hidden=notificationCount<=0;notificationsCount.textContent=String(notificationCount);renderWorkspaceMenu(shell.workspace||{});}));
function finishWorkspaceName(input,options={}){if(!input)return;if(options.cancel===true){input.value=input.dataset.originalValue||input.defaultValue||"";}else{const nextName=String(input.value||"").trim();const originalName=String(input.dataset.originalValue||input.defaultValue||"").trim();if(!nextName){input.value=originalName;}else if(nextName!==originalName){withHost((host)=>host.shell("workspace.rename",{workspaceId:input.dataset.workspaceId||"",name:nextName}));}}input.readOnly=true;input.tabIndex=-1;input.closest(".workspace-item")?.classList.remove("is-renaming");}
function focusWorkspaceName(workspaceId){for(const activeInput of workspaceList.querySelectorAll("[data-workspace-name-input]:not([readonly])")){if(activeInput.dataset.workspaceId!==workspaceId)finishWorkspaceName(activeInput);}const input=Array.from(workspaceList.querySelectorAll("[data-workspace-name-input]")).find((entry)=>entry.dataset.workspaceId===workspaceId);if(input){input.readOnly=false;input.tabIndex=0;input.dataset.originalValue=input.value;input.closest(".workspace-item")?.classList.add("is-renaming");input.focus();input.select();}}
edit?.addEventListener("click",()=>withHost((host)=>host.shell("edit.toggle",{})));
popover?.addEventListener("click",(event)=>{if(selfEditLocked){event.preventDefault();event.stopPropagation();return;}const button=event.target.closest("button[data-action]");if(button)withHost((host)=>host.shell(button.dataset.action,{}));});
density?.addEventListener("input",(event)=>{if(selfEditLocked){event.preventDefault();return;}withHost((host)=>host.shell("density.set",{value:Number(density.value)||4}));});
notificationsToggle?.addEventListener("click",()=>withHost((host)=>host.shell("notifications.toggle",{})));
workspaceList?.addEventListener("pointerdown",(event)=>{const button=event.target.closest("[data-workspace-action='rename']");if(button?.dataset.workspaceId){event.preventDefault();focusWorkspaceName(button.dataset.workspaceId||"");return;}if(!event.target.closest("[data-workspace-name-input]")){for(const activeInput of workspaceList.querySelectorAll("[data-workspace-name-input]:not([readonly])"))finishWorkspaceName(activeInput);}});
workspaceList?.addEventListener("click",(event)=>{const input=event.target.closest("[data-workspace-name-input]");if(input){event.stopPropagation();if(input.readOnly)input.blur();return;}const button=event.target.closest("[data-workspace-action][data-workspace-id]");if(!button)return;const action=button.dataset.workspaceAction;if(action==="rename"){focusWorkspaceName(button.dataset.workspaceId||"");return;}withHost((host)=>host.shell(`workspace.${action}`,{workspaceId:button.dataset.workspaceId||""}));});
workspaceList?.addEventListener("keydown",(event)=>{const input=event.target.closest("[data-workspace-name-input]");if(!input)return;if(event.key==="Enter"){event.preventDefault();input.blur();return;}if(event.key==="Escape"){event.preventDefault();finishWorkspaceName(input,{cancel:true});input.blur();}});
workspaceList?.addEventListener("blur",(event)=>{const input=event.target.closest("[data-workspace-name-input]");if(input?.dataset.workspaceId)finishWorkspaceName(input);},true);
popover?.addEventListener("click",(event)=>{const button=event.target.closest("[data-workspace-action]:not([data-workspace-id])");if(button)withHost((host)=>host.shell(`workspace.${button.dataset.workspaceAction}`,{}));});
"#,
    )
}

pub(crate) fn zoom_source() -> SandWidgetSource {
    source(
        "lince-shell-zoom.html",
        "Lince Zoom",
        "Pinned canvas zoom controls.",
        html! {
            div class="board-zoom-controls lynx-ui lynx-box" aria-label="Controles de zoom do canvas" {
                button class="lynx-button lynx-icon-button board-zoom-button" data-action="zoom.out" aria-label="Diminuir zoom" data-lynx-tooltip="Diminuir zoom" { svg class="lynx-icon" viewBox="-1 -1 18 18" aria-hidden="true" { path d="M3 8h10" {} } }
                button class="lynx-button lynx-icon-button board-zoom-button" data-action="zoom.reset" id="percent" aria-label="Voltar zoom para 100%" data-lynx-tooltip="Voltar zoom para 100%" { svg class="lynx-icon" viewBox="-1 -1 18 18" aria-hidden="true" { circle cx="8" cy="8" r="4" {} path d="M8 1v2M8 13v2M1 8h2M13 8h2" {} } }
                button class="lynx-button lynx-icon-button board-zoom-button" data-action="zoom.in" aria-label="Aumentar zoom" data-lynx-tooltip="Aumentar zoom" { svg class="lynx-icon" viewBox="-1 -1 18 18" aria-hidden="true" { path d="M8 2v12M2 8h12" {} } }
                button class="lynx-button lynx-icon-button board-zoom-button" data-action="zoom.center" aria-label="Recentralizar canvas" data-lynx-tooltip="Recentralizar canvas" { svg class="lynx-icon" viewBox="-1 -1 18 18" aria-hidden="true" { path d="M5 2H2v3M11 2h3v3M14 11v3h-3M5 14H2v-3M2 8h12M8 2v12" {} } }
                button class="lynx-button lynx-icon-button board-zoom-button" data-action="layout.circle" aria-label="Reorganizar componentes no centro" data-lynx-tooltip="Reorganizar componentes no centro" { svg class="lynx-icon" viewBox="-1 -1 18 18" aria-hidden="true" { circle cx="8" cy="8" r="5.5" {} circle cx="8" cy="2.5" r=".5" {} circle cx="13.5" cy="8" r=".5" {} circle cx="8" cy="13.5" r=".5" {} circle cx="2.5" cy="8" r=".5" {} } }
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const percent=document.getElementById("percent");
withHost((host)=>host.subscribe(({meta})=>{const zoom=Math.round(Number(meta.shell?.zoom?.percent||100));percent.setAttribute("aria-label",`Voltar zoom para ${zoom}%`);percent.dataset.lynxTooltip=`Voltar zoom para ${zoom}%`;}));
document.body.addEventListener("click",(event)=>{const button=event.target.closest("button[data-action]");if(button)withHost((host)=>host.shell(button.dataset.action,{}));});
"#,
    )
}

pub(crate) fn ai_source() -> SandWidgetSource {
    source(
        "lince-shell-ai.html",
        "AI",
        "Seed AI entry point.",
        html! { section class="aiPanel" { h1 { "AI" } p { "Open the local AI builder to draft and install new sand widgets." } button id="open-ai" { "Open AI" } } },
        r#"function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}document.getElementById("open-ai")?.addEventListener("click",()=>withHost((host)=>host.shell("navigation.ai",{})));"#,
    )
}
