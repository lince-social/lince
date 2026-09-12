import {t, localize} from '/i18n.js';
import {readRulesEditor} from '/read-rules.js';

let mergePatch, root, beginBatch, endBatch;
let socket, challenge, signingKey, sequence = 1, draftUid = '', revision = '', reconnect;
let signingReady = false, stopped = false, view = location.pathname.startsWith('/records/') ? decodeURIComponent(location.pathname.slice(9)) : '';
const pending = new Map();
const encoder = new TextEncoder();
let lanes = [], activeThread = '';
const base64 = bytes => btoa(Array.from(new Uint8Array(bytes), byte => String.fromCharCode(byte)).join(''));
const element = (tag, text, className) => {
  const node = document.createElement(tag);
  if (text != null) node.textContent = text;
  if (className) node.className = className;
  return node;
};

function reset() {
  beginBatch();
  mergePatch({record: null});
  mergePatch({ready:false, connected:false, signedin:false,name:'',page:'kanban',settings:false,creating:false,record:{}, records:[], threads:[], messages:[], assertions:[], users:[], roles:[], permissions:[], templates:[], canusers:false, canconfigure:false, canadmin:false, canrules:false, filterconcepts:[], canedit:false, cancomment:false, head:'', body:'', slug:'', quantity:'0', unit:'', identity:'', predicate:'', object:'', comment:'', thread:'', dirty:false, remotechanged:false});
  endBatch();
  draftUid = '';
  signingReady = false;
}

async function request(path, body) {
  const response = await fetch(path, {method:body === undefined ? 'GET' : 'POST', headers:{'Content-Type':'application/json'}, body:body === undefined ? undefined : JSON.stringify(body)});
  if (!response.ok) throw new Error(t(await response.text()));
  return response.json();
}

async function work(callback) {
  if (root.busy) return;
  mergePatch({busy:true, error:'', notice:''});
  try { await callback(); }
  catch (error) { mergePatch({error:error.message || t("Could not complete this request.")}); }
  finally { mergePatch({busy:false}); }
}

function transmit(message) {
  if (socket?.readyState !== WebSocket.OPEN) throw new Error(t("Connection lost. Wait for the connection before trying again."));
  socket.send(JSON.stringify(message));
}

function reply(message) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(message.id); reject(new Error(t("No reply received. Check the record before trying again."))); }, 20000);
    pending.set(message.id, {resolve, reject, timer, signed:message.type === 'signed_act'});
    try { transmit(message); }
    catch (error) { clearTimeout(timer); pending.delete(message.id); reject(error); }
  });
}

async function sign(text) {
  return base64(await crypto.subtle.sign('Ed25519', signingKey.privateKey, encoder.encode(text)));
}

async function authenticate(message) {
  challenge = message;
  sequence = 1;
  if (!message.signing_required) { signingReady = true; return; }
  try {
    if (!crypto.subtle) throw new Error(t("Editing needs HTTPS or localhost in this browser."));
    signingKey ||= await crypto.subtle.generateKey('Ed25519', true, ['sign', 'verify']);
    const publicKey = base64(await crypto.subtle.exportKey('raw', signingKey.publicKey));
    const keyId = `facade-${base64(await crypto.subtle.digest('SHA-256', encoder.encode(publicKey))).replaceAll('/', '_').replaceAll('+', '-').replaceAll('=', '')}`;
    const proof = ['lince.action-intent-session.v1', message.session_id, message.challenge, message.person, keyId, publicKey].join('\n');
    await reply({type:'session_authenticate', id:crypto.randomUUID(), session_id:message.session_id, session_challenge:message.challenge, person_uid:message.person, key_id:keyId, public_key_base64:publicKey, signature:await sign(proof)});
    signingReady = true;
  } catch (error) { mergePatch({error:error.message}); }
}

async function act(action) {
  if (!signingReady) throw new Error(t("Editing is not ready. Use HTTPS or localhost and wait for the connection."));
  const id = crypto.randomUUID?.() || `facade-${Date.now()}-${sequence++}`;
  if (!challenge.signing_required) return reply({type:'act', id, action});
  const actionBase64 = base64(encoder.encode(JSON.stringify(action)));
  const number = sequence++;
  const text = ['lince.action-intent.v1', challenge.session_id, challenge.challenge, number, id, actionBase64].join('\n');
  return reply({type:'signed_act', id, session_id:challenge.session_id, session_challenge:challenge.challenge, sequence:number, action_base64:actionBase64, signature:await sign(text)});
}

function connect() {
  clearTimeout(reconnect);
  signingReady = false;
  socket = new WebSocket(`${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/live`);
  socket.onopen = () => {
    mergePatch({connected:true, error:''});
    transmit({type:'select', uid:view});
    if (root.search) transmit({type:'search', text:root.search});
  };
  socket.onmessage = event => {
    const message = JSON.parse(event.data);
    if (message.type === 'session_challenge') { void authenticate(message); return; }
    if (message.type === 'signals') {
      const record = message.signals.record;
      beginBatch();
      mergePatch({record:null});
      mergePatch(message.signals);
      if (!root.creating && record.uid && (!root.dirty || draftUid !== record.uid)) loadDraft(record);
      else if (!root.creating && record.uid && revision !== record.updated_at) mergePatch({remotechanged:true});
      if (!root.creating && !record.uid) { mergePatch({head:'', body:'', slug:'', quantity:'0', unit:'', identity:'', predicate:'', object:'', comment:'', dirty:false}); draftUid = ''; }
      endBatch();
      return;
    }
    if (message.type === 'logout') { stopped = true; reset(); socket.close(); return; }
    const waiting = pending.get(message.id);
    if (waiting) {
      clearTimeout(waiting.timer);
      pending.delete(message.id);
      if (message.type === 'error') {
        if (waiting.signed && message.code === 'facade_request_rejected') sequence--;
        waiting.reject(new Error(t(message.message)));
      }
      else waiting.resolve(message);
    } else if (message.type === 'error') mergePatch({error:t(message.message)});
  };
  socket.onclose = () => {
    mergePatch({connected:false});
    signingReady = false;
    for (const item of pending.values()) { clearTimeout(item.timer); item.reject(new Error(t("Connection lost. Check the record before trying again."))); }
    pending.clear();
    if (!stopped) reconnect = setTimeout(async () => {
      try {
        const session = await request('/session');
        if (session.ready) connect(); else reset();
      } catch { connect(); }
    }, 2000);
  };
}

function loadDraft(record) {
  draftUid = record.uid;
  revision = record.updated_at;
  mergePatch({head:record.head || '', body:record.body || '', slug:record.slug || '', quantity:String(record.quantity ?? 0), unit:record.unit || '', identity:record.concept_name || '', dirty:false, remotechanged:false});
}

function select(uid, push = true) {
  if (root.dirty && !confirm(t("Discard your unsaved edits?"))) return false;
  view = uid;
  activeThread = '';
  draftUid = '';
  mergePatch({dirty:false, comment:'', thread:'', error:'', notice:'', selected:uid,page:'kanban',creating:false});
  if (push) history.pushState(null, '', uid ? `/records/${encodeURIComponent(uid)}` : '/');
  transmit({type:'select', uid});
  window.scrollTo(0, 0);
  return true;
}

const renderCache = new Map();
function changed(key, data) {
  const value = JSON.stringify(data);
  if (renderCache.get(key) === value) return false;
  renderCache.set(key, value);
  return true;
}

function field(form, title, name, value = '', type = 'text') {
  const label = element('label', title), input = element('input');
  input.name = name; input.type = type; input.value = value; input.maxLength = type === 'password' ? 1024 : 500;
  label.append(input); form.append(label); return input;
}

function button(parent, title, callback) {
  const node = element('button', title); node.type = 'button'; node.onclick = callback; parent.append(node); return node;
}

function columnRow(title = '', quantity = '') {
  const row = element('div', null, 'column-row');
  const name = field(row, t("Column"), 'column', title); name.required = true; name.maxLength = 100;
  const amount = field(row, t("Value"), 'value', quantity, 'number'); amount.required = true; amount.step = 'any';
  button(row, t("Remove"), () => row.remove());
  document.getElementById('column-editor').append(row);
}

function renderUsers(users, roles, permissions) {
  const host = document.getElementById('user-management'); host.replaceChildren();
  const roleSelect = (form, selected) => {
    const label = element('label', t("Role")), select = element('select'); select.name = 'role';
    select.replaceChildren(...roles.map(role => new Option(role.name, role.name)));
    select.value = selected || roles.find(role => role.name !== 'admin')?.name || ''; label.append(select); form.append(label); return select;
  };
  if (root.canusercreate && root.canassign) {
    const form = element('form'); form.append(element('h2', t("New user")));
    field(form, t("Username"), 'username').required = true; field(form, t("Name"), 'name'); field(form, t("Password"), 'password', '', 'password').required = true;
    roleSelect(form); const submit = element('button', t("Create user")); form.append(submit);
    form.onsubmit = event => { event.preventDefault(); work(async () => { await act({action:'create-user', ...Object.fromEntries(new FormData(form))}); form.reset(); refresh(); }); }; host.append(form);
  }
  for (const user of users) {
    const section = element('details', null, 'user'); section.append(element('summary', `${user.name || user.username} · ${user.username} · ${user.role}`));
    if (root.canuserupdate) {
      const form = element('form'); field(form,t("Username"),'username',user.username).required = true; field(form,t("Name"),'name',user.name); field(form,t("New password (blank keeps current)"),'password','','password');
      form.append(element('button',t("Save user"))); form.onsubmit = event => { event.preventDefault(); work(async () => { await request('/users',{uid:user.uid,...Object.fromEntries(new FormData(form))}); form.reset(); refresh(); }); }; section.append(form);

    }
    if (root.canassign) { const form = element('form'); const select = roleSelect(form,user.role); form.append(element('button',t("Assign role"))); form.onsubmit = event => { event.preventDefault(); work(async () => { await act({action:'assign-role',user:user.uid,role:select.value}); refresh(); }); }; section.append(form); }
    if (root.canuserdelete) button(section,t("Delete login"),() => { if (confirm(t('Delete the login for {name}? Their authored content will be kept.', {name:user.username}))) work(async () => { await request('/users',{uid:user.uid,delete:true}); refresh(); }); });
    host.append(section);
  }
  if (root.canrolecreate) {
    const form = element('form'); form.append(element('h2',t("New role"))); field(form,t("Role name"),'name').required = true; form.append(element('button',t("Create role")));
    form.onsubmit = event => { event.preventDefault(); work(async () => { await act({action:'create-role',name:new FormData(form).get('name')}); form.reset(); refresh(); }); }; host.append(form);
  }
  for (const role of roles) {
    const section = element('details'); section.append(element('summary', t('{name} permissions', {name:role.name})));
    if (role.name === 'admin') section.append(element('p',t("Admins have all permissions.")));
    else {
      section.append(element('p',t("Changes apply to every user with this role. Create a separate role for individual access.")));
      const list = element('div',null,'permission-list');
      for (const key of permissions) { const label = element('label',key), input = element('input'); input.type = 'checkbox'; input.checked = role.permissions.includes(key); input.disabled = !root.canpermissions;
        input.onchange = () => { const checked = input.checked; work(async () => { try { await act({action:checked ? 'grant-permission' : 'revoke-permission',role:role.name,permission:key}); refresh(); } catch(error) { input.checked = !checked; throw error; } }); }; label.prepend(input); list.append(label); }
      section.append(list);
    }
    if (root.canrules && role.name !== 'admin') {
      const form = element('form'); form.className = 'role-rules';
      try {
        const editor = readRulesEditor(role.rules || {allow:{all:[]},block:{any:[]}},root.filterconcepts || []);
        form.append(editor.host,element('button',t('Save access rules')));
        form.onsubmit = event => { event.preventDefault(); work(async () => { await act({action:'set-role-read-rules',role:role.name,rules:editor.read(),expected_revision:role.revision || 0}); refresh(); mergePatch({notice:t('Access rules saved.')}); }); };
      } catch (error) { form.append(element('p',error.message)); }
      section.append(form);
    }
    host.append(section);
  }
}

function refresh() { transmit({type:'select',uid:view}); }

function applyTheme(light) {
  document.documentElement.classList.toggle('light', light);
  const toggle = document.getElementById('theme');
  toggle.children[0].textContent = light ? '☾' : '☀';
  toggle.children[1].textContent = t(light ? 'Dark' : 'Light');
  toggle.setAttribute('aria-label', t(light ? 'Switch to dark mode' : 'Switch to light mode'));
}

window.facade = {
  select,
  t: (text, language) => t(text, {}, language),
  navigate(page) { if (select('')) mergePatch({page}); },
  theme() { const light = !document.documentElement.classList.contains('light'); applyTheme(light); try { localStorage.setItem('theme',light ? 'light' : 'dark'); } catch {} },
  createCard(quantity) {
    if (!root.cancreate || !select('')) return;
    mergePatch({creating:true,head:'',body:'',quantity:String(quantity),dirty:false});
    document.getElementById('create-tags').selectedIndex = -1;
    document.getElementById('create-title').focus();
  },
  submitRecord() { return work(async () => {
    const tags = Array.from(document.getElementById('create-tags').selectedOptions,option => option.value);
    const result = await act({action:'create-record-with-tags',head:root.head.trim(),body:root.body,quantity:Number(root.quantity),tags});
    mergePatch({dirty:false,creating:false});
    select(result.created);
  }); },
  moveCard(uid, quantity) { return work(async () => {
    await act({action:'set-quantity',target:uid,value:Number(quantity)});
    refresh();
  }); },
  deleteCard(uid = view) { if (confirm(t("Delete this card?"))) return work(async () => { await act({action:'delete-record',target:uid}); if (view === uid) { mergePatch({dirty:false}); select(''); } }); },
  customize(form) { return work(async () => {
    const data = new FormData(form), fds = {title:data.get('title').trim(),language:data.get('language')};
    await act({action:'set-extension',target:root.facadeuid,namespace:'lince.facade',fds});
    mergePatch({customtitle:fds.title,language:fds.language});
    mergePatch({notice:t('Settings saved.', {}, fds.language)});
  }); },
  addColumn() { columnRow(); },
  chooseTemplate(uid) { const template = root.templates.find(item => item.uid === uid); if (!template) return; document.getElementById('column-editor').replaceChildren(); for (const column of template.columns) columnRow(column.title,column.quantity); document.getElementById('column-form').elements.name.value = template.name; },
  saveColumns(form, template = false) { return work(async () => {
    const data = new FormData(form), values = data.getAll('value');
    const columns = data.getAll('column').map((title,index) => ({title,quantity:Number(values[index])}));
    if (!columns.length || columns.length > 40 || new Set(columns.map(c => c.quantity)).size !== columns.length || columns.some(c => !c.title.trim() || !Number.isFinite(c.quantity))) throw new Error(t("Use 1–40 named columns with unique numeric values."));
    const fds = {name:data.get('name'),columns}; let target = root.facadeuid;
    if (template) { const created = await act({action:'create-record',kind:'plain',head:fds.name}); target = created.created; select(target); }
    await act({action:'set-extension',target,namespace:'lince.kanban.columns',fds}); mergePatch({notice:template ? t("Template saved.") : t("Columns saved.")});
  }); },
  filterThread(uid) { activeThread = uid; renderCache.delete('messages'); window.facade.render(root.records,root.record,root.messages,root.threads,root.assertions,root.columns,root.templates,root.users,root.roles,root.permissions); },
  createThread() { const head = prompt(t("Thread title")); if (head?.trim()) return work(async () => { const result = await act({action:'create-thread',target:view,head:head.trim()}); mergePatch({thread:result.created}); }); },
  search(text) { if (socket?.readyState === WebSocket.OPEN) transmit({type:'search', text}); },
  login(form) { return work(async () => {
    const data = new FormData(form);
    await request('/login', {username:data.get('username'), password:data.get('password')});
    form.reset(); signingKey = undefined; stopped = false; mergePatch({ready:true}); connect();
  }); },
  logout() { return work(async () => {
    await request('/logout', {}); stopped = true; socket?.close(); signingKey = undefined; reset();
  }); },
  reloadDraft() { loadDraft(root.record); },
  copyLink() { return work(async () => { await navigator.clipboard.writeText(location.href); mergePatch({notice:t("Link copied.")}); }); },
  saveText() { return work(async () => {
    await act({action:'edit-record-text', target:view, head:root.head, body:root.body});
    mergePatch({dirty:false, remotechanged:false, notice:t("Text saved.")});
  }); },
  property(action, fields) { return work(async () => {
    await act({action, target:view, ...fields}); mergePatch({notice:t("Property saved.")});
  }); },
  identity(predicate) { return work(async () => {
    await act({action:'set-identity', subject:view, predicate:predicate || null}); mergePatch({notice:t("Identity saved.")});
  }); },
  assertion(remove) { return work(async () => {
    await act({action:remove ? 'retract-record' : 'assert-record', subject:view, predicate:root.predicate, object:root.object || null});
    mergePatch({predicate:'', object:'', notice:t("Assertion saved.")});
  }); },
  comment() { return work(async () => {
    const body = root.comment.trim();
    if (!body) return;
    let thread = root.thread;
    if (!thread) {
      const created = await act({action:'create-thread', target:view, head:t("General")});
      thread = created.created;
      mergePatch({thread});
    }
    await act({action:'create-message', thread, body});
    mergePatch({comment:'', notice:t("Comment posted.")});
  }); },
  render(records, record, messages, threads, assertions, columns = [], templates = [], users = [], roles = [], permissions = [], language = root?.language || 'pt-BR', title = root?.customtitle || 'Facade', canadmin = root?.canadmin || false) {
    if (changed('language',language)) {
      renderCache.clear();
      renderCache.set('language',JSON.stringify(language));
      localize(language);
      applyTheme(document.documentElement.classList.contains('light'));
    }
    if (changed('general',[title,language,canadmin])) {
      const form = document.getElementById('general-form');
      form.elements.title.value = title;
      form.elements.language.value = language;
    }
    lanes = columns.map(column => [column.title,column.quantity]);
    if (changed('concepts',root?.filterconcepts || [])) {
      const select = document.getElementById('create-tags');
      const chosen = new Set(Array.from(select.selectedOptions,option => option.value));
      select.replaceChildren(...(root?.filterconcepts || []).map(concept => new Option(`#${concept.name}`,concept.uid,false,chosen.has(concept.uid))));
    }
    if (changed('columns',columns)) {
      document.getElementById('create-column').replaceChildren(...lanes.map(([title,value]) => new Option(title,value)));
      document.getElementById('create-column').value = root?.quantity || '0';
      document.getElementById('move-column').replaceChildren(new Option(t("Choose a column"),''),...lanes.map(([title,value]) => new Option(title,value)));
      if (!document.getElementById('column-editor').contains(document.activeElement)) { document.getElementById('column-editor').replaceChildren(); for (const [title,value] of lanes) columnRow(title,value); }
    }
    if (changed('templates',templates)) document.getElementById('templates').replaceChildren(new Option(t("Choose a template"),''),...templates.map(template => new Option(template.name,template.uid)));
    if (root && changed('users',[users,roles,permissions,root.canusercreate,root.canuserupdate,root.canuserdelete,root.canassign,root.canrolecreate,root.canpermissions,root.canrules,root.filterconcepts])) renderUsers(users,roles,permissions);
    if (changed('board', [records,columns,root?.cancarddelete,root?.cancreate,root?.canmove,root?.connected,root?.busy])) {
      const columns = lanes.map(([title, quantity]) => {
        const section = element('section', null, 'column');
        const matching = records.filter(row => row.quantity === quantity || (quantity === lanes[0]?.[1] && !lanes.some(lane => lane[1] === row.quantity)));
        const heading = element('h2', title); heading.append(element('span', matching.length));
        const bar = element('div',null,'column-heading'); bar.append(heading);
        if (root?.cancreate) { const add = button(bar,'+',() => window.facade.createCard(quantity)); add.className = 'column-add'; add.setAttribute('aria-label',t('Create in {name}', {name:title})); }
        section.append(bar);
        section.ondragover = event => { if (root?.canmove && event.dataTransfer.types.includes('text/x-lince-record')) { event.preventDefault(); section.classList.add('drag-over'); } };
        section.ondragleave = event => { if (!section.contains(event.relatedTarget)) section.classList.remove('drag-over'); };
        section.ondrop = event => { event.preventDefault(); section.classList.remove('drag-over'); const uid = event.dataTransfer.getData('text/x-lince-record'); if (root?.canmove && records.some(row => row.uid === uid)) window.facade.moveCard(uid,quantity); };
        for (const row of matching) {
          const card = element('a', row.head || t("Untitled"), 'card');
          card.href = `/records/${encodeURIComponent(row.uid)}`;
          card.draggable = !!root?.canmove;
          card.ondragstart = event => { event.dataTransfer.setData('text/x-lince-record',row.uid); event.dataTransfer.effectAllowed = 'move'; };
          card.ondragend = () => document.querySelectorAll('.drag-over').forEach(node => node.classList.remove('drag-over'));
          card.onclick = event => { if (!event.ctrlKey && !event.metaKey && !event.shiftKey) { event.preventDefault(); select(row.uid); } };
          if (row.slug) card.append(element('span', `@${row.slug}`));
          const item = element('div',null,'card-item'); item.append(card);
          if (root?.cancarddelete) { const remove = button(item,'×',() => window.facade.deleteCard(row.uid)); remove.className = 'card-delete'; remove.title = t("Delete card"); remove.setAttribute('aria-label',t('Delete {name}', {name:row.head || t('Untitled')})); }
          if (root?.canmove) {
            const move = element('select',null,'card-move');
            move.setAttribute('aria-label',t('Move {name}',{name:row.head || t('Untitled')}));
            move.replaceChildren(...lanes.map(([title,value]) => new Option(title,value)));
            move.value = String(quantity); move.disabled = !root.connected || root.busy;
            move.onchange = () => window.facade.moveCard(row.uid,move.value);
            item.append(move);
          }
          section.append(item);
        }
        return section;
      });
      document.getElementById('board').replaceChildren(...columns);
    }
    if (changed('messages', messages)) {
      document.getElementById('messages').replaceChildren(...messages.filter(row => !activeThread || row.thread_uid === activeThread).map(row => {
        const article = element('article', null, 'message');
        article.append(element('time', row.created_at), element('p', row.body));
        return article;
      }));
    }
    if (changed('threads', threads)) {
      const select = document.getElementById('threads');
      const selected = root?.thread || select.value;
      select.replaceChildren(new Option(t("New conversation"), ''), ...threads.map(row => new Option(row.head, row.uid)));
      document.getElementById('thread-view').replaceChildren(new Option(t("All threads"),''),...threads.map(row => new Option(row.head,row.uid)));
      if (!threads.some(row => row.uid === activeThread)) activeThread = '';
      document.getElementById('thread-view').value = activeThread;
      if (threads.some(row => row.uid === selected)) select.value = selected;
    }
    if (changed('assertions', assertions)) document.getElementById('assertions').replaceChildren(...assertions.map(row => element('li', `${row.predicate}${row.object ? ` → ${row.object}` : ''}`)));
    document.title = title ? `${title} ·` : 'Lince';
  }
};

({mergePatch, root, beginBatch, endBatch} = await import('/datastar.js'));
try { applyTheme(localStorage.getItem('theme') === 'light'); } catch { applyTheme(false); }
window.addEventListener('popstate', () => {
  const uid = location.pathname.startsWith('/records/') ? decodeURIComponent(location.pathname.slice(9)) : '';
  if (!select(uid, false)) history.pushState(null, '', view ? `/records/${encodeURIComponent(view)}` : '/');
});
try {
  const session = await request('/session');
  mergePatch({customtitle:session.customtitle || 'Facade',language:session.language || 'pt-BR'});
  if (session.ready) { mergePatch({ready:true}); connect(); }
} catch (error) { mergePatch({error:error.message}); }
