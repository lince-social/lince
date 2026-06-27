use crate::{
    domain::lince_package::PackageManifest,
    sand::{SandWidgetSource, WidgetScript},
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
  --transition-fast:160ms ease;
  font-family:"IBM Plex Sans",Inter,ui-sans-serif,system-ui,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;
  background:transparent;
  color:var(--text-strong);
}
*{box-sizing:border-box}
html,body{margin:0;width:100%;height:100%;overflow:hidden;background:transparent}
button,input{font:inherit}
button{cursor:pointer;border:0;color:inherit;background:transparent}
.brand{display:flex;align-items:center;gap:10px;width:100%;height:100%}
.brand-mark{display:grid;place-items:center;width:28px;height:28px;color:var(--text-strong);background:none}
.brand-logo,.brand-logo svg{width:100%;height:auto}.brand-logo svg{display:block}.brand-logo .s0{stroke-width:50!important}
.brand-name{color:var(--text-strong);font-size:1rem;font-weight:600;letter-spacing:.03em}
.operation-box{width:100%;height:100%}
.operation-box__input{width:100%;height:40px;padding:0 14px;border:1px solid var(--border);border-radius:14px;outline:none;background:rgba(18,20,24,.75);color:var(--text-strong);font-size:.92rem;box-shadow:inset 0 1px 0 rgba(255,255,255,.03);transition:border-color var(--transition-fast),background var(--transition-fast),color var(--transition-fast)}
.operation-box__input::placeholder{color:var(--text-muted)}
.operation-box__input:focus{border-color:var(--border-strong);background:rgba(24,27,32,.92)}
.workspace-indicator,.icon-button{border:1px solid var(--border);background:rgba(18,20,24,.75);box-shadow:inset 0 1px 0 rgba(255,255,255,.03);color:var(--text-soft)}
.workspace-indicator{position:absolute;top:0;right:0;display:inline-flex;align-items:center;justify-content:center;gap:10px;min-width:72px;height:40px;padding:0 12px;border-radius:2px;transition:border-color var(--transition-fast),background var(--transition-fast),color var(--transition-fast),transform var(--transition-fast)}
.workspace-indicator:hover,.icon-button:hover{border-color:var(--border-strong);background:rgba(30,32,38,.9);color:var(--text-strong)}
.workspace-indicator__value{color:var(--text-strong);font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:.82rem;letter-spacing:.08em}
.workspace-indicator__chevron{display:inline-grid;place-items:center}.workspace-indicator__chevron svg{width:13px;height:13px}
.workspace-popover{position:absolute;top:50px;right:0;z-index:24;display:none;gap:8px;width:260px;padding:10px;border:1px solid rgba(255,255,255,.08);border-radius:16px;background:rgba(13,15,19,.94);box-shadow:0 20px 46px rgba(0,0,0,.28),inset 0 1px 0 rgba(255,255,255,.04);backdrop-filter:blur(20px)}
.workspace-popover.isOpen{display:grid}.workspace-list{display:grid;grid-template-columns:1fr;gap:8px}.workspace-item{display:grid;grid-template-columns:minmax(0,1fr) 34px 34px;gap:6px;align-items:center}.workspace-item__switch,.workspace-item__edit,.workspace-item__delete{min-height:38px;border:1px solid var(--border);border-radius:2px;background:rgba(18,20,24,.75);color:var(--text-soft)}.workspace-item__switch{display:flex;align-items:center;justify-content:flex-start;min-width:0;padding:0 10px;font-size:.82rem;font-weight:700}.workspace-item__name{width:100%;min-width:0;padding:0;border:0;outline:0;background:transparent;color:var(--text-strong);font:inherit;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;cursor:pointer;pointer-events:none}.workspace-item.is-renaming .workspace-item__name{cursor:text;pointer-events:auto}.workspace-item.is-renaming .workspace-item__switch,.workspace-item__switch:focus-within{border-color:rgba(255,255,255,.18);background:rgba(43,47,56,.96);color:var(--text-strong)}.workspace-item__edit,.workspace-item__delete{display:grid;place-items:center;width:34px;padding:0}.workspace-item__edit svg,.workspace-item__delete svg{width:14px;height:14px}.workspace-item__switch:hover,.workspace-item__switch.is-active,.workspace-item__edit:hover,.workspace-item__delete:hover{border-color:var(--border-strong);background:rgba(30,32,38,.9);color:var(--text-strong)}.workspace-item__delete:disabled{opacity:.28;cursor:not-allowed}
.workspace-popover__footer{display:grid;gap:6px}.workspace-popover__action{min-height:34px;border:1px solid var(--border);border-radius:12px;background:rgba(18,20,24,.75);color:var(--text-soft);padding:0 10px;text-align:left;font-size:.78rem}.workspace-popover__action:hover{border-color:var(--border-strong);background:rgba(30,32,38,.9);color:var(--text-strong)}
.icon-button{display:inline-grid;place-items:center;width:40px;height:40px;padding:0;border-radius:2px;text-decoration:none;transition:transform var(--transition-fast),border-color var(--transition-fast),background var(--transition-fast),color var(--transition-fast)}
.icon-button svg{width:19px;height:19px}
.notification-button{position:relative}.notification-button__mark{font-size:.9rem;font-weight:700}.notification-button__count{position:absolute;right:-3px;bottom:-3px;display:inline-grid;place-items:center;min-width:17px;height:17px;padding:0 4px;border-radius:999px;background:#e8b45b;color:#191b1f;font-size:.68rem;font-weight:700}
.editShell{position:relative;width:100%;height:100%}.editShell>.icon-button{position:absolute;top:0;right:0}.editPopover{position:fixed;right:0;top:46px;bottom:0;width:260px;display:none;gap:8px;padding:10px;border:1px solid rgba(255,255,255,.08);border-radius:16px 16px 0 0;background:rgba(13,15,19,.94);box-shadow:0 20px 46px rgba(0,0,0,.28),inset 0 1px 0 rgba(255,255,255,.04);backdrop-filter:blur(20px);overflow:auto;overscroll-behavior:contain}
.editPopover.isSelfEditLocked::before{content:"";position:absolute;inset:0;z-index:2;border-radius:inherit;background:rgba(8,10,14,.48);backdrop-filter:saturate(.72);pointer-events:auto;cursor:not-allowed}.editPopover.isSelfEditLocked>*{pointer-events:none;filter:grayscale(.35);opacity:.64}
.editPopover.isOpen{display:grid}.add-card-popover__action{display:flex;align-items:center;gap:10px;width:100%;min-height:44px;padding:8px 10px;border:1px solid rgba(255,255,255,.08);border-radius:12px;background:rgba(18,20,24,.75);color:var(--text-soft);text-align:left}.add-card-popover__action:hover{border-color:var(--border-strong);color:var(--text-strong);background:rgba(30,32,38,.9)}
.add-card-popover__icon{display:grid;place-items:center;width:24px;height:24px;color:var(--text-strong)}.add-card-popover__copy{display:grid;gap:2px}.add-card-popover__copy strong{font-size:.82rem}.add-card-popover__copy small{color:var(--text-muted);font-size:.68rem}
.floating-tag{display:inline-flex;align-items:center;gap:10px;min-height:40px;padding:0 14px;border:1px solid rgba(255,255,255,.08);border-radius:2px;background:rgba(13,15,19,.86);box-shadow:0 20px 46px rgba(0,0,0,.28),inset 0 1px 0 rgba(255,255,255,.04);backdrop-filter:blur(20px);color:var(--text-soft)}
.floating-tag__label{font-size:.78rem}.density-slider{width:118px}.density-control__value{min-width:28px;color:var(--text-strong);font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:.79rem;text-align:right}
.board-zoom-controls{display:inline-flex;align-items:center;gap:6px;padding:6px;border:1px solid rgba(255,255,255,.08);border-radius:14px;background:rgba(13,15,19,.88);box-shadow:0 20px 46px rgba(0,0,0,.28),inset 0 1px 0 rgba(255,255,255,.04);backdrop-filter:blur(20px)}
.board-zoom-button,.board-zoom-indicator{display:inline-grid;place-items:center;height:32px;border:0;border-radius:2px;background:transparent;color:var(--text-soft);cursor:pointer;font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:.86rem;transition:background var(--transition-fast),color var(--transition-fast)}
.board-zoom-button{width:32px}.board-zoom-button--wide{width:38px}.board-zoom-indicator{min-width:64px;padding:0 9px;background:rgba(18,20,24,.72);color:var(--text-strong);font-weight:700}
.board-zoom-button:hover,.board-zoom-indicator:hover{background:rgba(255,255,255,.08);color:var(--text-strong)}
.tutorial,.aiPanel{display:block;width:100%;height:100%;padding:18px;border:1px solid var(--border);border-radius:4px;background:#171a1f;color:var(--text-strong)}.tutorial{overflow:auto;padding:24px}.tutorial__eyebrow{margin:0 0 8px;color:var(--text-muted);font-size:11px;font-weight:700;letter-spacing:.16em;text-transform:uppercase}.tutorial h1,.aiPanel h1{font-size:18px;margin:0 0 8px}.tutorial h1{font-size:24px;line-height:1.12;margin-bottom:10px}.tutorial h2{margin:24px 0 8px;color:var(--text-strong);font-size:15px}.tutorial p,.aiPanel p{font-size:14px;line-height:1.45;margin:0;color:var(--text-soft)}.tutorial p{max-width:68ch;margin-bottom:10px}.tutorial ul{display:grid;gap:8px;margin:10px 0 0;padding:0;list-style:none}.tutorial li{position:relative;padding-left:18px;color:var(--text-soft);font-size:13px;line-height:1.42}.tutorial li::before{content:"";position:absolute;left:0;top:.62em;width:6px;height:6px;border-radius:2px;background:var(--text-muted)}.tutorial strong{color:var(--text-strong);font-weight:700}.tutorial__grid{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:18px;margin-top:12px}.tutorial__note{margin-top:18px;padding:14px;border:1px solid rgba(255,255,255,.08);border-radius:4px;background:rgba(255,255,255,.03)}.tutorial__note p{margin:0}.aiPanel button{margin-top:14px;height:36px;border-radius:2px;border:1px solid var(--border);background:rgba(18,20,24,.75);color:var(--text-soft);padding:0 14px}
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
            WidgetScript::inline(script),
        ]
    };

    SandWidgetSource {
        filename,
        lang: "en",
        manifest: manifest(title, description),
        head_links: vec![],
        inline_styles: vec![STYLE],
        body,
        body_scripts,
    }
}

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

pub(crate) fn operation_source() -> SandWidgetSource {
    source(
        "lince-shell-operation.html",
        "Lince Operation",
        "Pinned operation launcher.",
        html! {
            form class="operation-box" id="operation-form" {
                input id="operation-input" class="operation-box__input" placeholder="Operation" autocomplete="off" aria-label="Executar operacao";
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const form=document.getElementById("operation-form");const input=document.getElementById("operation-input");
form?.addEventListener("submit",(event)=>{event.preventDefault();withHost((host)=>host.shell("operation.submit",{value:input?.value||""}));if(input)input.value="";});
"#,
    )
}

pub(crate) fn workspaces_source() -> SandWidgetSource {
    source(
        "lince-shell-workspaces.html",
        "Lince Workspaces",
        "Pinned workspace controller.",
        html! {
            div class="editShell" {
                button id="current" class="workspace-indicator" type="button" aria-label="Abrir seletor de areas" {
                    span id="workspace-value" class="workspace-indicator__value" { "01" }
                    span class="workspace-indicator__chevron" aria-hidden="true" {
                        svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" { path d="m6 9 6 6 6-6" {} }
                    }
                }
                div id="workspace-popover" class="workspace-popover" {
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
const current=document.getElementById("current");const value=document.getElementById("workspace-value");const popover=document.getElementById("workspace-popover");const list=document.getElementById("workspace-list");let workspaceOpen=false;
function iconSvg(kind){return kind==="edit"?'<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9.8 3.1 12.9 6.2"></path><path d="M11.3 1.9a1.45 1.45 0 0 1 2.1 2.1L5.7 11.7 3 12.4l.7-2.7 7.6-7.8Z"></path></svg>':'<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3.5 4.5h9"></path><path d="M6.5 2.75h3"></path><path d="M5 4.5v7"></path><path d="M8 4.5v7"></path><path d="M11 4.5v7"></path><path d="M4.5 4.5 5 13h6l.5-8.5"></path></svg>';}
function renderWorkspaceMenu(workspace){workspaceOpen=workspace?.open===true;const items=Array.isArray(workspace?.items)?workspace.items:[];list.replaceChildren(...items.map((item)=>{const row=document.createElement("div");row.className=`workspace-item${item.active?" is-active":""}`;const switchButton=document.createElement("div");switchButton.className=`workspace-item__switch${item.active?" is-active":""}`;switchButton.dataset.workspaceAction="switch";switchButton.dataset.workspaceId=String(item.id||"");switchButton.setAttribute("role","button");switchButton.tabIndex=0;const input=document.createElement("input");input.className="workspace-item__name";input.dataset.workspaceNameInput="";input.dataset.workspaceId=String(item.id||"");input.value=String(item.name||item.label||"");input.autocomplete="off";input.spellcheck=false;input.readOnly=true;input.tabIndex=-1;input.setAttribute("aria-label",`Nome de ${item.name||item.label||"area"}`);switchButton.append(input);const editButton=document.createElement("button");editButton.type="button";editButton.className="workspace-item__edit";editButton.dataset.workspaceAction="rename";editButton.dataset.workspaceId=String(item.id||"");editButton.setAttribute("aria-label",`Renomear ${item.name||item.label||"area"}`);editButton.innerHTML=iconSvg("edit");const deleteButton=document.createElement("button");deleteButton.type="button";deleteButton.className="workspace-item__delete";deleteButton.dataset.workspaceAction="delete";deleteButton.dataset.workspaceId=String(item.id||"");deleteButton.disabled=item.canDelete===false;deleteButton.setAttribute("aria-label",`Apagar ${item.name||item.label||"area"}`);deleteButton.innerHTML=iconSvg("delete");row.append(switchButton,editButton,deleteButton);return row;}));popover.classList.toggle("isOpen",workspaceOpen);}
withHost((host)=>host.subscribe(({meta})=>{const shell=meta.shell||{};value.textContent=shell.workspace?.label||"01";renderWorkspaceMenu(shell.workspace||{});}));
current?.addEventListener("click",()=>withHost((host)=>host.shell("workspace.toggle",{})));
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

pub(crate) fn notifications_source() -> SandWidgetSource {
    source(
        "lince-shell-notifications.html",
        "Lince Notifications",
        "Pinned notification control.",
        html! {
            button id="toggle" class="icon-button notification-button" type="button" aria-label="Abrir notificacoes" {
                span class="notification-button__mark" aria-hidden="true" { "!" }
                span id="count" class="notification-button__count" hidden { "0" }
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const count=document.getElementById("count");
withHost((host)=>host.subscribe(({meta})=>{const value=Number(meta.shell?.notifications?.count||0);count.hidden=value<=0;count.textContent=String(value);}));
document.getElementById("toggle")?.addEventListener("click",()=>withHost((host)=>host.shell("notifications.toggle",{})));
"#,
    )
}

pub(crate) fn edit_source() -> SandWidgetSource {
    source(
        "lince-shell-edit.html",
        "Lince Edit",
        "Pinned edit controls.",
        html! {
            div class="editShell" {
                button id="edit" class="icon-button" type="button" aria-label="Alternar modo de edicao" {
                    svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
                        path d="M12 20h9" {}
                        path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" {}
                    }
                }
                div id="popover" class="editPopover" {
                    button class="add-card-popover__action" data-action="edit.self" {
                        span class="add-card-popover__icon" { "✎" }
                        span class="add-card-popover__copy" { strong { "Edit self" } small { "Mover e configurar este sand" } }
                    }
                    button class="add-card-popover__action" data-action="card.add" {
                        span class="add-card-popover__icon" { "+" }
                        span class="add-card-popover__copy" { strong { "Add card" } small { "Escolher no catalogo de sand" } }
                    }
                    button class="add-card-popover__action" data-action="card.import" {
                        span class="add-card-popover__icon" { "↥" }
                        span class="add-card-popover__copy" { strong { "Importar" } small { "widget .html, .sand ou .lince do disco" } }
                    }
                    label class="floating-tag" {
                        span class="floating-tag__label" { "Grid" }
                        input id="density" class="density-slider" type="range" min="1" max="7" step="1";
                        span id="density-value" class="density-control__value" { "40" }
                    }
                }
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const edit=document.getElementById("edit");const popover=document.getElementById("popover");const density=document.getElementById("density");const densityValue=document.getElementById("density-value");let selfEditLocked=false;
withHost((host)=>host.subscribe(({meta})=>{const shell=meta.shell||{};const editing=shell.editMode===true;selfEditLocked=editing&&String(shell.selfEditCardId||"")===String(meta.instanceId||"");edit.classList.toggle("is-active",editing);popover.classList.toggle("isOpen",editing);popover.classList.toggle("isSelfEditLocked",selfEditLocked);popover.setAttribute("aria-disabled",String(selfEditLocked));if(density&&shell.density)density.value=String(shell.density);if(densityValue&&shell.gridSnap)densityValue.textContent=String(shell.gridSnap);}));
edit?.addEventListener("click",()=>withHost((host)=>host.shell("edit.toggle",{})));
popover?.addEventListener("click",(event)=>{if(selfEditLocked){event.preventDefault();event.stopPropagation();return;}const button=event.target.closest("button[data-action]");if(button)withHost((host)=>host.shell(button.dataset.action,{}));});
density?.addEventListener("input",(event)=>{if(selfEditLocked){event.preventDefault();return;}withHost((host)=>host.shell("density.set",{value:Number(density.value)||4}));});
"#,
    )
}

pub(crate) fn zoom_source() -> SandWidgetSource {
    source(
        "lince-shell-zoom.html",
        "Lince Zoom",
        "Pinned canvas zoom controls.",
        html! {
            div class="board-zoom-controls" aria-label="Controles de zoom do canvas" {
                button class="board-zoom-button" data-action="zoom.out" aria-label="Diminuir zoom" { "−" }
                button class="board-zoom-indicator" data-action="zoom.reset" id="percent" aria-label="Voltar zoom para 100%" { "100%" }
                button class="board-zoom-button" data-action="zoom.in" aria-label="Aumentar zoom" { "+" }
                button class="board-zoom-button board-zoom-button--wide" data-action="zoom.center" aria-label="Recentralizar canvas" { "⌖" }
                button class="board-zoom-button board-zoom-button--wide" data-action="layout.circle" aria-label="Reorganizar componentes no centro" { "◎" }
            }
        },
        r#"
function withHost(fn){let tries=0;const tick=()=>{if(window.LinceWidgetHost){fn(window.LinceWidgetHost);return;}if(++tries<80)setTimeout(tick,25);};tick();}
const percent=document.getElementById("percent");
withHost((host)=>host.subscribe(({meta})=>{const zoom=Number(meta.shell?.zoom?.percent||100);percent.textContent=`${Math.round(zoom)}%`;}));
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

pub(crate) fn tutorial_source() -> SandWidgetSource {
    source(
        "lince-shell-tutorial.html",
        "Tutorial",
        "Seed workspace tutorial.",
        html! {
            section class="tutorial" {
                p class="tutorial__eyebrow" { "Welcome to the frontend" }
                h1 { "This screen is a workspace made of sand." }
                p {
                    "The frontend is the part of Lince you can touch: a large canvas, a few pinned controls, and many small tools called "
                    strong { "sands" }
                    ". A sand can be a note, table, calendar, terminal, document viewer, transfer panel, or anything else that fits in a widget."
                }
                p {
                    "You can move sands around, resize them, pin the important ones, remove what is noise, and open the catalog to add more. Pinned sand stays near the screen, while regular sand lives in the zoomable world."
                }
                div class="tutorial__grid" {
                    div {
                        h2 { "How the sand system feels" }
                        ul {
                            li { strong { "Add" } " opens the sand catalog so you can choose the tool you need." }
                            li { strong { "Edit mode" } " lets you move, resize, configure, pin, or remove sands." }
                            li { strong { "Configure" } " connects a sand to views, servers, preview behavior, or host features when that sand supports it." }
                            li { strong { "Pinned sands" } " become your control surface: useful for navigation, status, and tools you always want nearby." }
                        }
                    }
                    div {
                        h2 { "What the frontend is for" }
                        ul {
                            li { "It turns backend records, rules, and transfers into things you can see and act on." }
                            li { "It keeps different workflows side by side instead of forcing everything into one page." }
                            li { "It lets each sand stay focused, while the workspace holds the whole situation." }
                            li { "It is meant to feel practical: arrange the work the way your mind already maps it." }
                        }
                    }
                }
                h2 { "The Lince idea" }
                p {
                    "Lince models the world with "
                    strong { "Records" }
                    ". A Record has a head and body for human meaning, and a "
                    strong { "quantity" }
                    " for state. A lot of Lince work is about changing quantities in a clear, inspectable way."
                }
                p {
                    "That sounds simple on purpose. A number can mean stock, debt, need, budget, progress, votes, time, responsibility, or any other measurable state. Instead of hiding behavior behind fixed app buttons, Lince pushes you toward rules made from data."
                }
                h2 { "Karma is automation over records" }
                p {
                    strong { "Karma" }
                    " combines record quantities, commands, frequencies, and Rust-side condition logic into equations. When a condition crosses its threshold, a consequence happens."
                }
                ul {
                    li { strong { "Condition" } " is the math or logic being evaluated." }
                    li { strong { "Frequency" } " works like a built-in cron signal, often returning zero until the scheduled moment arrives." }
                    li { strong { "Threshold" } " decides whether the value is meaningful enough to pass through." }
                    li { strong { "Consequence" } " can change another record quantity, call shell or SQL commands, activate transfers, or synchronize record trees between organs." }
                }
                p {
                    "The docs give a monthly example: a frequency returns 1 only at the scheduled time, then multiplies by one record's quantity and copies that amount into another record. Most of the month the equation is zero, so nothing happens. On the right day, the state moves."
                }
                h2 { "Organs and Transfers" }
                p {
                    "A Lince node is an "
                    strong { "Organ" }
                    ". You might have a personal organ, a family organ, a work organ, or a project organ. A person can have users in many organs and access them through login."
                }
                p {
                    strong { "Transfer" }
                    " is how organs coordinate needs and contributions. It can represent a one-way donation, or a trade where one need is met by an item or service and another need is met by money. The same idea can cover a marketplace purchase, a shared project, or planning a party."
                }
                div class="tutorial__note" {
                    p {
                        "In short: the frontend gives you a humane workspace of sands. Lince underneath gives those sands a theory of records, quantities, automation, organs, and transfers. The canvas is where those ideas become usable."
                    }
                }
            }
        },
        "",
    )
}
