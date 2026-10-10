(async () => {
  const data = await window.designReady;
  const app = document.querySelector('#app');
  const scenario = document.querySelector('#scenario');
  const alternative = data.alternatives.find(item => item.id === document.body.dataset.alternative);
  const storageKey = `organ-structure:${alternative.id}`;
  let saved;
  try { saved = JSON.parse(sessionStorage.getItem(storageKey) || '{}'); } catch { saved = {}; }
  let route = location.hash.slice(1) || 'home';
  let space = Object.keys(alternative.groups)[0];
  let chosen = saved.chosen || 0;
  let state = saved.state || 'populated';
  let draft = saved.draft || {};
  let contactNames = saved.contactNames || data.contacts.map(item => item.name);
  let extraContact = saved.extraContact || false;
  let operation = 0;
  let confirmAction;
  scenario.value = state;
  const esc = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const button = (label, destination, cls = '') => `<button class="${cls}" data-route="${destination}">${esc(label)}</button>`;
  const persist = () => sessionStorage.setItem(storageKey, JSON.stringify({chosen,state,draft,contactNames,extraContact}));
  const contacts = () => state === 'empty' ? [] : state === 'large' ? Array.from({length:28}, (_,i) => ({...data.contacts[i%2],name:i===0?'Ana with a very long descriptive Organ name that must remain readable':`Contact ${i+1}`})) : data.contacts.map((c,i) => ({...c,name:contactNames[i]})).concat(extraContact ? [{name:'New contact',uid:'organ-demo-new',status:'Verification pending',fingerprint:'DEMO · 0F12'}] : []);
  const current = () => contacts()[chosen] || data.contacts[0];
  const directory = (groups = alternative.groups) => `<div class="directory">${Object.entries(groups).map(([label, ids]) => `<section><h2>${esc(label)}</h2>${ids.filter(id => !['contact','contact-policy','contact-access','contact-files'].includes(id)).map(id => button(data.pages[id].title,id)).join('')}</section>`).join('')}</div>`;
  const contactList = () => contacts().length ? `<ul class="contacts">${contacts().map((c,i) => `<li><button data-contact="${i}"><span>${esc(c.name)}</span><span class="quiet">${esc(state==='error'?'Offline':c.status)}</span></button></li>`).join('')}</ul>` : `<p class="quiet">No contacts yet. Add a pairing code or find someone on this Wi-Fi.</p>`;
  const stateNotice = () => state==='pending' ? '<p class="notice" role="status">Connecting to the local Organ… saved tools remain available.</p>' : state==='error' ? '<div class="notice"><p>Connection unavailable. Your saved information is still here.</p><button data-retry>Reconnect</button></div>' : state==='restricted' ? '<p class="notice">Viewing only. This device or login lacks editing authority.</p>' : state==='expired' ? '<p class="notice">Device roster expired. Owner authorization is needed before it can authorize devices.</p>' : '';
  const mySummary = () => `<div class="identity"><h2>${esc(data.organ.name)}</h2><p class="quiet">${state==='empty'?'This device has no published Organ identity yet.':`${data.devices.length} devices · ${contacts().length} contacts`}</p></div>`;
  const hub = () => `<section class="overview">${mySummary()}<div class="actions">${button('Add contact','add','primary')}${button('Share my Organ','pairing')}${button('Manage','tools')}</div><section><h2>Contacts</h2>${contactList()}</section></section>`;
  const sectionHome = () => {
    if (space==='Contacts' || space==='People') return `<section class="overview"><div class="actions">${button('Add contact','add','primary')}${button('Nearby','nearby')}</div>${contactList()}${alternative.id==='two-spaces'?'':`<details><summary>Contact tools</summary>${directory({[space]:alternative.groups[space]})}</details>`}</section>`;
    if (space==='Connect') return `<section class="overview"><h2>Make a connection</h2><div class="directory"><section>${button('Add a pairing code','add')}${button('Find nearby','nearby')}${button('Browse public discovery','discover')}</section></div><details><summary>Search settings & visibility</summary>${directory({'Public discovery':['searches','moderation']})}</details></section>`;
    return `<section class="overview">${mySummary()}<div class="actions">${button('Contacts','contacts','primary')}${button('Share my Organ','pairing')}${button('My devices','devices')}</div>${alternative.id==='two-spaces'?'':`<details><summary>Identity, delivery & settings</summary>${directory({[space]:alternative.groups[space]})}</details>`}</section>`;
  };
  const ownerOf = id => Object.entries(alternative.groups).find(([,ids])=>ids.includes(id))?.[0] || Object.keys(alternative.groups)[0];
  const nav = () => alternative.id==='task-hub' ? '' : `<nav aria-label="Organ spaces">${Object.keys(alternative.groups).map(label => `<button data-space="${esc(label)}" ${space===label?'aria-current="page"':''}>${esc(label)}</button>`).join('')}</nav>${alternative.id==='two-spaces'?`<label class="picker">Page in ${esc(space)}<select id="page-picker"><option value="home">Overview</option>${alternative.groups[space].filter(id=>!['contact','contact-policy','contact-access','contact-files'].includes(id)).map(id=>`<option value="${id}" ${route===id?'selected':''}>${esc(data.pages[id].title)}</option>`).join('')}</select></label>`:''}`;
  const info = id => {
    if(id==='pairing') return `<div class="metadata"><div class="qr-placeholder" role="img" aria-label="QR placement illustration, not a scannable code">QR location<br>Illustration only</div><p class="code">${esc(data.organ.pairing)}</p><p>Identity key for comparison: ${esc(data.organ.fingerprint)}</p><p class="quiet">Contact pairing does not enrol a device or grant login access.</p></div>`;
    if(id.startsWith('contact') && id!=='contacts' && id!=='contact-search') return `<div class="metadata"><h2>${esc(current().name)}</h2><p>${esc(current().status)} · fingerprint ${esc(current().fingerprint)}</p><p class="quiet">${esc(current().uid)}</p></div>${id==='contact'?`<div class="actions"><button class="primary" data-mock="Open conversation Thread">Open conversation</button><button data-mock="Open live Organ">Open live Organ</button></div><div class="actions">${button('Sharing & trust','contact-policy')}${button('Login & workspaces','contact-access')}${button('File sync','contact-files')}</div>`:''}${id==='contact-policy'?'<p>Nothing hidden. No refused changes. Hiding cannot recall received data; unhiding sends the whole Record and earlier changes.</p><p class="quiet">Unreadable saved limits are ignored and must be fixed before relying on them. Incoming policy skips do not appear as refused changes.</p>':''}`;
    if(id==='devices')return `<div class="metadata"><p>Roster version 4 · ${state==='expired'?'expired':'valid until 2026-12-01'}</p>${state==='empty'?'<p>Create an Organ here, or join an existing one from a fresh device profile.</p>':data.devices.map((d,i)=>`<button data-device="${i}" aria-pressed="${Number(draft.device||0)===i}">${esc(d.name)} · ${esc(d.status)}</button>`).join('')}<p class="quiet">Selected device: ${esc(data.devices[Number(draft.device||0)].name)}. Fingerprint DEMO · 82A1. Mail key DEMO · valid until 2026-12-01. A relay cannot write for your Organ.</p></div><div class="actions">${button('Add or join a device','enrol')}${button('Keys & backup','recovery')}</div>`;
    if(id==='enrol')return '<p class="notice">One-use device code: expires in ten minutes. Keep it private. Joining requires a fresh Cell; compare identity with the device showing the code.</p><div id="enrolment-output" role="status"></div>';
    if(id==='nearby') return state==='empty'?'<p>No nearby Cells found. LAN presence may be disabled.</p>'+button('Network & presence','network'):`<div class="metadata"><h2>Studio tablet</h2><p>Unverified name · fingerprint DEMO · C3A1 · 4B02</p><p class="quiet">Compare fingerprints before trusting this Organ.</p><div class="actions"><button data-mock="Nearby chat">Chat</button><button data-nearby-add>Add known contact</button>${button('Open saved contact','contact')}</div></div>${button('Network & presence','network','text-button')}`;
    if(id==='mail')return '<div class="metadata"><h2>One message waiting</h2><p>Saved locally · 2 queued operations · 1 sealed envelope</p><p>Carrier storage: 1 of 2 servers accepted.</p><p>Recipient receipt: not confirmed.</p><p class="quiet">Mailbox carries sealed Record changes and conversations, up to 1 MiB. Files and calls need a direct connection. Automatic route tries direct first, then mailbox after ten minutes offline; conversations fall back immediately.</p></div>'+button('Carriers & pickup points','carriers');
    if(id==='network')return '<p class="quiet">Local-only reach uses no connection relay. Changing reach or relay selection restarts live connections. Direct internet connectivity can reveal this machine’s address.</p><p>Peer addresses: local demo · port 6175</p>';
    if(id==='storage')return '<p>Disk usage: 240 MiB · budget unlimited</p><div class="actions">'+button('File copies','copies')+button('Moves & offers','moves')+button('Shared workspaces','workspaces')+button('Sync activity','sync-tools')+'</div>';
    if(id==='moves')return '<p class="quiet">Moves need a direct connection. Preview includes linked Records, Assertions and Karma dependencies: at most 128 Records / 8 MiB and 2,048 dependencies. Source remains until explicit acceptance and durable receipt. Karma arrives paused. Cancellation closes when accepted delivery starts.</p><p>Demo offer: Studio notes · waiting for explicit acceptance</p>';
    if(id==='contact-files')return '<p class="quiet">An unreadable filter is ignored: everything syncs until fixed. Saving this filter replaces any saved Protein selection. A blank filter selects all Records.</p>';
    if(id==='requests')return `<div class="metadata"><h2>Introduction about bicycle repair</h2><p>${state==='expired'?'Message delivery expired; retained text remains.':'Accepted private conversation · saved on this device'}</p><p>“Hello, I can help this weekend.”</p><p class="quiet">Carrier storage is separate from recipient confirmation. Resume keeps the old deadline; confirmed resend renews eligible expired text for thirty days.</p></div>`;
    if(id==='posts')return '<p>One saved draft · Bicycle repair · not published</p><p class="quiet">Publishing, updating, pausing, fulfilment and withdrawal each require an exact public preview. Earlier public copies may remain.</p>';
    if(id==='profile')return '<p class="quiet">Preparing an image does not publish it. Authorized edits sync to your devices; owner authorization may be pending. Resolve concurrent fields before saving.</p>';
    if(id==='discover')return '<div class="metadata"><h2>Bicycle repair</h2><p>Contribution · nearby region · signed public claim</p><p class="quiet">A posting signature does not verify a person’s name or claims. Chosen directories can see deliberate queries.</p></div><div class="actions">'+button('My announcements','posts')+button('Private requests','requests')+button('Saved searches','searches')+'</div>';
    if(id==='recovery')return '<p class="quiet">Export before detaching a root key. Detaching checks the saved copy. Future membership changes require bringing the root key back.</p>';
    if(id==='credits')return '<p>Native QR: nokhwa, qrcode-rust, rqrr. Native media: image, rfd, base64, vodozemac. This HTML study embeds no third-party QR or media library; it uses Lince’s bundled Lato font and tokens.</p><a href="/assets/Lato-OFL.txt">Lato font license</a>';
    return '';
  };
  const fieldHTML = (f,form,i) => {
    const context = ['contact','contact-policy','contact-access','contact-files'].includes(route) ? current().uid : route==='devices' ? data.devices[Number(draft.device||0)].uid : 'local';
    const name = `${form.id}:${context}:${i}`;
    const value = draft[name] ?? (f.label==='Local name'?current().name:f.label==='Name'?data.organ.name:f.label==='Device name'?data.devices[Number(draft.device||0)].name:'');
    const common = `name="${esc(name)}" ${f.type==='password'?'data-persist="false" autocomplete="new-password"':''}`;
    if(f.type==='select')return `<label>${esc(f.label)}<select ${common}>${(f.options?.length?f.options:['Direct','Mailbox','Automatic']).map(o=>`<option ${value===o?'selected':''}>${esc(o)}</option>`).join('')}</select></label>`;
    if(f.type==='checkbox')return `<label><input type="checkbox" ${common} ${value===true?'checked':''}>${esc(f.label)}</label>`;
    return `<label>${esc(f.label)}<input type="${f.type||'text'}" ${common} value="${f.type==='password'?'':esc(value)}" ${f.type==='number'?'min="0"':''}></label>`;
  };
  const readOnly = f => /^(Refresh|Reload|Check|Inspect|Review|View|Next|Continue|Open|Search local)/.test(f.title) || ['copy-pairing','credits'].includes(f.id);
  const forms = id => `<div class="tools">${data.pages[id].forms.filter(f => {
    if(id==='devices' && f.id==='roster-create-organ')return state==='empty';
    if(id==='devices' && state==='empty')return ['roster-create-organ','roster-status'].includes(f.id);
    if(f.id==='resend-expired-private')return state==='expired';
    if(f.id==='resume-private')return state!=='expired';
    return true;
  }).map((f,i) => {
    const visible = (id==='add' && f.id==='add-known-organ') || (id==='identity' && i===0);
    return `<details data-capability="${esc(f.id)}" ${visible?'open':''}><summary>${esc(f.title)}</summary><form id="${esc(f.id)}" data-operation="${esc(f.id)}">${f.note?`<p class="quiet">${esc(f.note)}</p>`:''}${f.fields.map((field,j)=>fieldHTML(field,f,j)).join('')}${f.id.includes('scan-')?'<p class="quiet">Simulates decoding a demo code; no image or camera is accessed.</p>':''}<button class="primary" ${state==='restricted'&&!readOnly(f)?'disabled':''}>${esc(f.title)}</button><p class="status" role="status"></p></form></details>`;
  }).join('')}</div>`;
  function render(focus=false) {
    if(data.pages[route])space=ownerOf(route);
    const title = route==='home'?'Organ':route==='tools'?'All tools':data.pages[route]?.title || 'Organ';
    app.innerHTML = `<header><div><p class="eyebrow">Castle</p><h1 tabindex="-1">${esc(title)}</h1></div>${route==='home'?'':button('Home','home','text-button')}</header>${nav()}${stateNotice()}${route==='home'?(alternative.id==='task-hub'?hub():sectionHome()):route==='tools'?directory():data.pages[route]?`<div class="breadcrumb">${button(alternative.id==='task-hub'?'All tools':space,alternative.id==='task-hub'?'tools':'home','text-button')}<span aria-hidden="true">/</span><span>${esc(title)}</span></div><section class="page" data-page="${route}"><p class="quiet">${esc(data.pages[route].description)}</p>${route==='contacts'?`<div class="actions">${button('Add contact','add')}${button('Share my Organ','pairing')}</div>${contactList()}`:info(route)}${forms(route)}</section>`:hub()}`;
    if(focus)app.querySelector('h1').focus();
    persist();
  }
  function go(id, updateHistory=true) { route=id; if(updateHistory && location.hash!==`#${id}`)history.pushState(null,'',`#${id}`); render(true); }
  const confirm = (text,action) => { confirmAction=action; document.querySelector('#confirm-copy').textContent=text; document.querySelector('#confirmation').showModal(); };
  document.querySelector('#cancel-confirm').onclick=()=>{confirmAction=null;document.querySelector('#confirmation').close();};
  document.querySelector('#accept-confirm').onclick=()=>{document.querySelector('#confirmation').close();const action=confirmAction;confirmAction=null;action?.();};
  app.addEventListener('click',event=>{
    const target=event.target.closest('button');
    if(!target)return;
    if(target.dataset.route)go(target.dataset.route);
    else if(target.dataset.space){space=target.dataset.space;go('home');}
    else if(target.hasAttribute('data-contact')){chosen=Number(target.dataset.contact);go('contact');}
    else if(target.hasAttribute('data-device')){draft.device=Number(target.dataset.device);render();}
    else if(target.hasAttribute('data-retry')){state='populated';scenario.value=state;render();}
    else if(target.hasAttribute('data-nearby-add'))confirm('Compare the nearby fingerprint DEMO · C3A1 · 4B02 before saving. Save this synthetic contact?',()=>{extraContact=true;go('contacts');});
    else if(target.dataset.mock)confirm(`${target.dataset.mock}. This is a prototype entry point; no native action will be dispatched.`,()=>{
      if(target.dataset.mock==='Publish this exact preview')target.closest('form').querySelector('[role="status"]').textContent='Exact demo preview queued for the selected host. Publication is simulated.';
    });
  });
  app.addEventListener('change',event=>{
    if(event.target.id==='page-picker'){go(event.target.value);return;}
    if(event.target.name && event.target.type!=='password'){draft[event.target.name]=event.target.type==='checkbox'?event.target.checked:event.target.value;persist();}
  });
  app.addEventListener('submit',event=>{
    event.preventDefault();
    const form=event.target;
    const f=data.pages[route].forms.find(item=>item.id===form.dataset.operation);
    if(!f || (state==='restricted'&&!readOnly(f)))return;
    const perform=async()=>{
      const output=form.querySelector('[role="status"]');
      const submit=form.querySelector('button');
      const token=++operation;
      submit.disabled=true;output.textContent='Waiting for the simulated request…';
      await new Promise(resolve=>setTimeout(resolve,220));
      if(token!==operation || !form.isConnected)return;
      submit.disabled=false;
      if(state==='error' && !readOnly(f)) {output.classList.add('error');output.innerHTML='Connection unavailable. Nothing sent. <button type="button" data-retry>Reconnect and try again</button>';return;}
      if(f.id.includes('scan-file')||f.id.includes('scan-camera')){
        const destination=route==='enrol'?'roster-join-organ':'add-known-organ';
        const input=app.querySelector(`#${destination} input`);
        const code=route==='enrol'?data.organ.enrolment:data.organ.pairing;
        if(input){input.value=code;draft[input.name]=code;input.closest('details').open=true;input.focus();}
        output.textContent='Demo code filled. Review identity and submit separately; nothing added.';
      } else if(f.id==='add-known-organ'){
        const value=form.querySelector('input')?.value||'';
        if(!value.startsWith('lince1|')){output.classList.add('error');output.textContent='Enter or simulate scanning a whole lince1| pairing code.';return;}
        extraContact=true;output.textContent='Synthetic contact saved. Find it in Contacts.';
      } else if(f.id==='roster-enrol-token'){
        document.querySelector('#enrolment-output').innerHTML=`<p class="code">${esc(data.organ.enrolment)}</p><p class="quiet">Demo one-use code · ten-minute lifetime · keep private. QR placement uses this code.</p>`;
        output.textContent='Synthetic device code issued.';
      } else if(f.id==='rename-organ-contact'){
        const value=form.querySelector('input')?.value.trim();
        if(value)contactNames[chosen]=value;
        output.textContent='Local demo name saved. It does not change their public profile.';
      } else if(f.id==='copy-pairing')output.textContent='Demo pairing code selected for copying. Clipboard is not accessed.';
      else if(f.id.startsWith('stop-camera'))output.textContent='Simulated scanner stopped. No contact or device was added.';
      else if(f.id.startsWith('preview-') || f.id==='preview')output.innerHTML='<p>Exact public preview: Bicycle repair · Contribution · Anonymous · demo selected host. Public copies may remain.</p><button type="button" data-mock="Publish this exact preview">Publish this exact preview</button>';
      else if(f.id==='owner-backup'){
        const values=[...form.querySelectorAll('input')];
        if(!values[0].value || !values[1].value || values[1].value!==values[2].value){output.textContent='Choose a destination and matching demo passphrases.';return;}
        values.filter(v=>v.type==='password').forEach(v=>v.value='');output.textContent='Backup sequence simulated. Native Lince would close, capture and reopen. No file was written.';
      } else output.textContent=`${f.title}: simulated. No native action was dispatched.`;
      output.classList.remove('error');persist();
    };
    if(f.confirm)confirm(f.confirm,perform);else perform();
  });
  scenario.addEventListener('change',()=>{state=scenario.value;operation++;render();});
  addEventListener('hashchange',()=>{const next=location.hash.slice(1)||'home';if(next!==route){route=next;render(true);}});
  addEventListener('popstate',()=>{route=location.hash.slice(1)||'home';render(true);});
  window.organStudy={go,render,data,alternative,setState:value=>{state=value;scenario.value=value;render();}};
  render();
})();
