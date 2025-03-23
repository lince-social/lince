use axum::response::{Html, IntoResponse};

pub async fn page() -> impl IntoResponse {
    Html(
        br#"
    <!doctype html>
    <html lang="en">
        <head>
            <meta charset="UTF-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1.0" />
            <meta http-equiv="X-UA-Compatible" content="ie=edge" />
            <title>Lince</title>
            <!--<script src="https://unpkg.com/htmx.org@2.0.4"></script> -->
            <script>
(()=>{if(document.__fixi_mo)return;document.__fixi_mo=new MutationObserver((e=>e.forEach((e=>"childList"===e.type&&e.addedNodes.forEach((e=>r(e)))))));let e=(e,t,i,n)=>e.dispatchEvent(new CustomEvent("fx:"+t,{detail:i,cancelable:!0,bubbles:!1!==n,composed:!0})),t=(e,t,i)=>e.getAttribute(t)||i,i=e=>null!=e.closest("[fx-ignore]"),n=n=>{let r={};n.__fixi||i(n)||!e(n,"init",{options:r})||(n.__fixi=async i=>{let r=n.__fixi.requests||=new Set,a=n.form||n.closest("form"),o=new FormData(a??void 0,i.submitter);!a&&n.name&&o.append(n.name,n.value);let s=new AbortController,c={trigger:i,action:t(n,"fx-action"),method:t(n,"fx-method","GET").toUpperCase(),target:document.querySelector(t(n,"fx-target"))??n,swap:t(n,"fx-swap","outerHTML"),body:o,drop:r.size,headers:{"FX-Request":"true"},abort:s.abort.bind(s),signal:s.signal,preventTrigger:!0,transition:document.startViewTransition?.bind(document),fetch:fetch.bind(window)},f=e(n,"config",{cfg:c,requests:r});if(c.preventTrigger&&i.preventDefault(),!f||c.drop)return;if(/GET|DELETE/.test(c.method)){let e=new URLSearchParams(c.body);e.size&&(c.action+=(/\?/.test(c.action)?"&":"?")+e),c.body=null}r.add(c);try{if(c.confirm){if(!await c.confirm())return}if(!e(n,"before",{cfg:c,requests:r}))return;if(c.response=await c.fetch(c.action,c),c.text=await c.response.text(),!e(n,"after",{cfg:c}))return}catch(t){return void e(n,"error",{cfg:c,error:t})}finally{r.delete(c),e(n,"finally",{cfg:c})}let d=()=>{if(c.swap instanceof Function)return c.swap(c);if(/(before|after)(begin|end)/.test(c.swap))c.target.insertAdjacentHTML(c.swap,c.text);else{if(!(c.swap in c.target))throw c.swap;c.target[c.swap]=c.text}};c.transition?await c.transition(d).finished:await d(),e(n,"swapped",{cfg:c}),document.contains(n)||e(document,"swapped",{cfg:c})},n.__fixi.evt=t(n,"fx-trigger",n.matches("form")?"submit":n.matches("input:not([type=button]),select,textarea")?"change":"click"),n.addEventListener(n.__fixi.evt,n.__fixi,r),e(n,"inited",{},!1))},r=e=>{if(e.matches){if(i(e))return;e.matches("[fx-action]")&&n(e)}e.querySelectorAll&&e.querySelectorAll("[fx-action]").forEach(n)};document.addEventListener("fx:process",(e=>r(e.target))),document.addEventListener("DOMContentLoaded",(()=>{document.__fixi_mo.observe(document.documentElement,{childList:!0,subtree:!0}),r(document.body)}))})();
            </script>
            <style>
                body {
                    background-color: #000000;
                    color: #ffffff;
                }
            </style>
        </head>
        <body>
        <button
            id="abda"
            fx-action="/section/body"
            fx-trigger="click"
            fx-method="get"
            fx-target="\#abda"
            fx-swap="outerHTML"
        >
        odisodi
        </button>
        </body>
    </html>
    "#,
    )
}
