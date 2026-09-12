import assert from 'node:assert/strict';
import http from 'node:http';
import {readFile, mkdtemp, rm} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import {tmpdir} from 'node:os';
import {join} from 'node:path';

const source = new URL('../src/', import.meta.url);
const profile = await mkdtemp(join(tmpdir(), 'facade-ui-'));
const server = http.createServer(async (request, response) => {
  if (request.url === '/session') {
    response.setHeader('Content-Type', 'application/json');
    response.end(JSON.stringify({ready:true, customtitle:'Equipe Azul', language:'pt-BR'}));
    return;
  }
  const file = {'/':'facade.html', '/facade.css':'facade.css', '/facade.js':'facade.js', '/i18n.js':'i18n.js', '/read-rules.js':'read-rules.js', '/datastar.js':'vendor/datastar.js'}[request.url];
  if (!file) { response.writeHead(404); response.end(); return; }
  response.setHeader('Content-Type', file.endsWith('.js') ? 'text/javascript' : file.endsWith('.css') ? 'text/css' : 'text/html');
  response.end(await readFile(new URL(file, source)));
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const chrome = spawn(process.env.CHROMIUM || '/run/current-system/sw/bin/chromium', ['--headless','--no-sandbox','--disable-gpu','--disable-dev-shm-usage','--remote-debugging-port=0',`--user-data-dir=${profile}`,'about:blank'], {stdio:'ignore'});
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
let socket;
try {
  let port;
  for (let attempt = 0; attempt < 100; attempt++) {
    try { port = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n')[0]; break; } catch {}
    await delay(100);
  }
  assert.ok(port, 'Chromium starts');
  const tabs = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  socket = new WebSocket(tabs.find(tab => tab.type === 'page').webSocketDebuggerUrl);
  await new Promise(resolve => socket.addEventListener('open', resolve, {once:true}));
  let sequence = 0;
  const pending = new Map(), errors = [];
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    if (message.id) { pending.get(message.id)?.(message); pending.delete(message.id); }
    if (message.method === 'Runtime.exceptionThrown') errors.push(message.params.exceptionDetails);
  });
  const cdp = (method, params = {}) => new Promise(resolve => { const id = ++sequence; pending.set(id, resolve); socket.send(JSON.stringify({id, method, params})); });
  const evaluate = async expression => {
    const result = await cdp('Runtime.evaluate', {expression, awaitPromise:true, returnByValue:true});
    assert.equal(result.result.exceptionDetails, undefined, JSON.stringify(result.result.exceptionDetails));
    return result.result.result.value;
  };
  const patch = fields => evaluate(`import('/datastar.js').then(({mergePatch}) => mergePatch(${JSON.stringify(fields)}))`);
  const click = async selector => {
    const point = await evaluate(`(() => { const r = document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2}; })()`);
    await cdp('Input.dispatchMouseEvent', {type:'mouseMoved', ...point});
    await delay(200);
    await cdp('Input.dispatchMouseEvent', {type:'mousePressed', button:'left', clickCount:1, ...point});
    await cdp('Input.dispatchMouseEvent', {type:'mouseReleased', button:'left', clickCount:1, ...point});
    await delay(100);
  };
  await cdp('Runtime.enable');
  await cdp('Page.enable');
  await cdp('Emulation.setDeviceMetricsOverride', {width:1440, height:1000, deviceScaleFactor:1, mobile:false});
  await cdp('Page.addScriptToEvaluateOnNewDocument', {source:`
    window.actions = [];
    window.WebSocket = class {
      static OPEN = 1;
      readyState = 1;
      constructor() { window.mockSocket=this; setTimeout(() => { this.onopen(); this.onmessage({data:JSON.stringify({type:'session_challenge',signing_required:false})}); }, 50); }
      send(data) {
        const message = JSON.parse(data);
        if (message.type === 'act') {
          window.actions.push(message.action);
          setTimeout(() => this.onmessage({data:JSON.stringify({type:'result',id:message.id,created:'new-record'})}), 0);
        }
      }
      close() {}
    };
  `});
  await cdp('Page.navigate', {url:`http://127.0.0.1:${server.address().port}`});
  for (let attempt = 0; attempt < 100; attempt++) {
    if (await evaluate(`document.querySelector('.brand')?.textContent === 'Equipe Azul ·'`)) break;
    await delay(100);
  }
  assert.equal(await evaluate('document.documentElement.lang'), 'pt-BR');
  assert.equal(await evaluate('document.title'), 'Equipe Azul ·');
  await patch({ready:true, signedin:true, connected:true, cancreate:true, canmove:true, canadmin:true, canconfigure:true, facadeuid:'facade-record', columns:[{title:'Pendências',quantity:0},{title:'Próximos',quantity:-1}], records:[{uid:'record',head:'Teste <img src=x>',quantity:0}]});
  assert.equal(await evaluate(`document.querySelectorAll('.column-add').length`), 2);
  assert.equal(await evaluate(`document.querySelectorAll('header,.connection,.board-heading').length`), 0);
  assert.ok(await evaluate(`(() => {const bar=document.querySelector('.column-heading'), title=bar.querySelector('h2').getBoundingClientRect(), button=bar.querySelector('button').getBoundingClientRect(); return button.left >= title.right && Math.abs((button.top+button.height/2)-(title.top+title.height/2)) < 2;})()`));
  await evaluate(`window.prompt = () => { throw new Error('Creation must not use a prompt'); }; window.facade.createCard(-1)`);
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('#create-record')).display`), 'grid');
  assert.equal(await evaluate(`document.querySelector('#create-column').value`), '-1');
  await patch({head:'New task',body:'Description draft',dirty:true});
  await evaluate(`window.mockSocket.onmessage({data:JSON.stringify({type:'signals',signals:{record:{},selected:''}})})`);
  assert.equal(await evaluate(`document.querySelector('#create-title').value`), 'New task');
  await evaluate(`document.querySelector('#create-record').requestSubmit()`);
  await delay(100);
  assert.deepEqual(await evaluate('window.actions.at(-1)'), {action:'create-record-with-tags',head:'New task',body:'Description draft',quantity:-1,tags:[]});
  assert.equal(await evaluate(`location.pathname`), '/records/new-record');
  await evaluate(`window.facade.select('')`);
  await evaluate(`(() => { const move=document.querySelector('.card-move'); move.value='-1'; move.dispatchEvent(new Event('change')); })()`);
  await delay(100);
  assert.deepEqual(await evaluate('window.actions.at(-1)'), {action:'set-quantity',target:'record',value:-1});
  assert.ok(await evaluate(`(() => { const theme=document.querySelector('#theme').getBoundingClientRect(), login=document.querySelector('.logout').getBoundingClientRect(); return theme.bottom <= login.top && theme.top > innerHeight * .8; })()`));
  await click('#theme');
  assert.ok(await evaluate(`document.documentElement.classList.contains('light')`));
  assert.equal(await evaluate(`document.querySelector('#theme').getAttribute('aria-label')`), 'Mudar para o modo escuro');
  await cdp('Input.dispatchMouseEvent', {type:'mouseMoved', x:400, y:200});
  await delay(250);
  assert.equal(await evaluate(`document.querySelector('.sidebar').getBoundingClientRect().width`), 60);
  await click('[data-i18n-title="Settings"]');
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('.general-settings')).display`), 'block');
  assert.equal(await evaluate(`document.querySelector('#general-form').elements.title.value`), 'Equipe Azul');
  await evaluate(`(() => {const form=document.querySelector('#general-form'); form.elements.title.value='Time <Azul>'; form.elements.language.value='en'; form.requestSubmit();})()`);
  await delay(200);
  assert.deepEqual(await evaluate('window.actions.at(-1)'), {action:'set-extension', target:'facade-record', namespace:'lince.facade', fds:{title:'Time <Azul>', language:'en'}});
  assert.equal(await evaluate('document.documentElement.lang'), 'en');
  assert.equal(await evaluate('document.title'), 'Time <Azul> ·');
  assert.equal(await evaluate(`document.querySelector('.brand').textContent`), 'Time <Azul> ·');
  assert.equal(await evaluate(`document.querySelector('.general-settings h1').textContent`), 'General settings');
  assert.equal(await evaluate(`document.querySelector('#theme').getAttribute('aria-label')`), 'Switch to dark mode');
  assert.equal(await evaluate(`document.querySelectorAll('.brand img, .card img').length`), 0);
  await evaluate(`(() => {const form=document.querySelector('#general-form'); form.elements.language.value='pt-BR'; form.requestSubmit();})()`);
  await delay(200);
  assert.equal(await evaluate(`document.querySelector('.general-settings h1').textContent`), 'Configurações gerais');
  await patch({canadmin:false});
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('.general-settings')).display`), 'none');
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('[data-i18n-title="Settings"]')).display`), 'none');
  await patch({page:'users',canusers:true,canrules:true,filterconcepts:[{uid:'project',name:'project-a'},{uid:'done',name:'done'}],roles:[{name:'editor',permissions:[],revision:3,rules:{allow:{all:[]},block:{any:[]}}}]});
  await evaluate(`(() => {const form=document.querySelector('.role-rules'); form.closest('details').open=true; const groups=form.querySelectorAll('.read-rules > .filter-group'); for(const group of groups) group.querySelector('button').click(); const allow=groups[0].querySelectorAll('.filter-row select'), block=groups[1].querySelectorAll('.filter-row select'); allow[1].value='project'; block[0].value='not'; block[1].value='done'; form.requestSubmit();})()`);
  await delay(100);
  assert.deepEqual(await evaluate('window.actions.at(-1)'), {action:'set-role-read-rules',role:'editor',expected_revision:3,rules:{allow:{all:[{concept_in:'project'}]},block:{any:[{not:{concept_in:'done'}}]}}});
  await patch({canrules:false});
  assert.equal(await evaluate(`document.querySelectorAll('.role-rules').length`),0);
  await patch({ready:false, signedin:false});
  assert.ok(await evaluate(`document.querySelector('#login').innerText.includes('Nome de usuário')`));
  assert.ok(await evaluate(`(() => { const theme=document.querySelector('#theme').getBoundingClientRect(), login=document.querySelector('[data-i18n-title="Log in"]').getBoundingClientRect(); return theme.bottom <= login.top && theme.top > innerHeight * .8; })()`));
  await cdp('Emulation.setDeviceMetricsOverride', {width:390, height:844, deviceScaleFactor:1, mobile:true});
  assert.ok(await evaluate('document.documentElement.scrollWidth <= innerWidth'));
  assert.deepEqual(errors, []);
  console.log('Facade browser checks passed: Portuguese/English, admin settings, escaped title, column creation, moves, role filters, layout, theme and sidebar collapse.');
} finally {
  socket?.close();
  chrome.kill();
  await new Promise(resolve => chrome.once('exit', resolve));
  server.close();
  await rm(profile, {recursive:true, force:true});
}
