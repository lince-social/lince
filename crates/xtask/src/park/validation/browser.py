import argparse,hashlib,json,os,shutil,signal,socket,subprocess,sys,time,urllib.request,urllib.error
from pathlib import Path
ROOT=Path(__file__).resolve().parent

def write(path,value):
    path.write_text(json.dumps(value,indent=2)+'\n')

def host_environment():
    environment=os.environ.copy()
    for key in ['LD_LIBRARY_PATH','GBM_BACKENDS_PATH','__EGL_VENDOR_LIBRARY_DIRS']:
        saved=environment.pop('LINCE_HOST_'+key,None)
        if saved=='0':environment.pop(key,None)
        elif saved is not None and saved.startswith('1'):environment[key]=saved[1:]
    return environment

def port():
    with socket.socket() as listener:
        listener.bind(('127.0.0.1',0));return listener.getsockname()[1]

def resources(pid,elapsed):
    process=Path('/proc')/str(pid)
    fields={line.split(':',1)[0]:line.split(':',1)[1].strip() for line in (process/'status').read_text().splitlines() if ':' in line}
    stat=(process/'stat').read_text().rsplit(')',1)[1].split()
    drm={}
    for entry in (process/'fdinfo').iterdir():
        try:data={line.split(':',1)[0]:line.split(':',1)[1].strip() for line in entry.read_text().splitlines() if ':' in line}
        except (FileNotFoundError,PermissionError):continue
        if 'drm-client-id' not in data:continue
        identity=data.get('drm-pdev','unknown')+':'+data['drm-client-id']
        drm[identity]={key:value for key,value in data.items() if key.startswith(('drm-total-','drm-resident-','drm-memory-')) or key=='drm-driver'}
    return {'elapsed':elapsed,'rss':fields.get('VmRSS'),'swap':fields.get('VmSwap'),'virtual':fields.get('VmSize'),'threads':fields.get('Threads'),'descriptors':len(list((process/'fd').iterdir())),'cpu_ticks':int(stat[11])+int(stat[12]),'drm_clients':drm or None}

def wait(check,seconds=40):
    deadline=time.monotonic()+seconds
    error=None
    while time.monotonic()<deadline:
        try:
            value=check()
            if value:return value
        except Exception as failure:error=failure
        time.sleep(.1)
    raise RuntimeError(f'Timed out waiting: {error}')

if len(sys.argv)>1 and sys.argv[1]=='ready':
    run=Path(sys.argv[2])
    keys=['WAYLAND_DISPLAY','XDG_RUNTIME_DIR','DBUS_SESSION_BUS_ADDRESS','GDK_BACKEND','QT_QPA_PLATFORM','XDG_CURRENT_DESKTOP','XDG_SESSION_DESKTOP','XDG_SESSION_TYPE']
    subprocess.run(['dbus-update-activation-environment','WAYLAND_DISPLAY','XDG_CURRENT_DESKTOP','XDG_SESSION_TYPE','XDG_SESSION_DESKTOP'],check=True,timeout=10)
    write(run/'ready.json',{'pid':os.getppid(),'helper_pid':os.getpid(),'environment':{key:os.environ.get(key) for key in keys}})
    if (run/'provider.json').exists():
        provider=json.loads((run/'provider.json').read_text())
        program=provider['target']['program']
        environment=os.environ.copy();environment.update(XDG_CONFIG_HOME=str(run/'config'),XDG_CACHE_HOME=str(run/'cache'),XDG_STATE_HOME=str(run/'state'))
        os.execvpe(program,[program,*provider['target'].get('arguments',[])],environment)
    wait(lambda:(run/'capture.request').exists(),300)
    probe=sys.argv[3]
    os.execv(probe,[probe,str(run/'capture.json'),'--image',str(run/'scene.ppm'),'--application','firefox'])

if len(sys.argv)>1 and sys.argv[1]=='client':
    run=Path(sys.argv[2]);program=sys.argv[3]
    keys=['WAYLAND_DISPLAY','DISPLAY','XDG_RUNTIME_DIR','GDK_BACKEND','LD_LIBRARY_PATH','GBM_BACKENDS_PATH','__EGL_VENDOR_LIBRARY_DIRS','NIRI_SOCKET','SWAYSOCK','HYPRLAND_INSTANCE_SIGNATURE']
    write(run/'client-environment.json',{key:os.environ.get(key) for key in keys})
    wait(lambda:(run/'ready.json').exists(),60)
    os.execvp(program,[program,*sys.argv[4:]])

if len(sys.argv)>1 and sys.argv[1]=='sandbox':
    run=Path(sys.argv[2]);program=sys.argv[3]
    write(run/'sandbox.json',{'pid':os.getpid()})
    os.execv(program,[program,*sys.argv[4:]])

parser=argparse.ArgumentParser()
parser.add_argument('executable',type=Path)
parser.add_argument('--geckodriver',required=True,type=Path)
parser.add_argument('--capture-probe',required=True,type=Path)
parser.add_argument('--provider',type=Path)
parser.add_argument('--plain',action='store_true')
parser.add_argument('--host-close',type=Path)
parser.add_argument('--host-prepare',type=Path)
parser.add_argument('--firefox',required=True)
parser.add_argument('--terminal',required=True)
parser.add_argument('--fixtures',required=True,type=Path)
parser.add_argument('--fullscreen',action='store_true')
parser.add_argument('--keep-size',action='store_true')
parser.add_argument('--ownership',action='store_true')
parser.add_argument('--save-failure',action='store_true')
parser.add_argument('--reopen-sandbox',action='store_true')
parser.add_argument('--outputs',type=int,default=1)
parser.add_argument('--seconds',type=int,default=75)
parser.add_argument('--lifecycle-rounds',type=int,choices=range(21),default=0)
arguments=parser.parse_args()
assert 15<=arguments.seconds<=3600
assert 1<=arguments.outputs<=8
assert not (arguments.save_failure or arguments.reopen_sandbox) or arguments.host_close,'These checks require --host-close'
run=ROOT/'runs'/f'installed-{int(time.time())}-{os.getpid()}'
run.mkdir(parents=True,mode=0o700)
for name in ['cell','profile','config','cache','state']: (run/name).mkdir(mode=0o700)
runtime=Path.cwd()/'target/pvr'/str(os.getpid());runtime.mkdir(parents=True,mode=0o700)
page=(arguments.fixtures/'page.html').read_text()
prefs=(arguments.fixtures/'user.js').read_text()
marionette,webdriver=port(),port()
(run/'profile/user.js').write_text(prefs+f'\nuser_pref("marionette.port", {marionette});\nuser_pref("media.autoplay.default", 0);\n')
(run/'page.html').write_text(page)
shutil.copyfile(arguments.fixtures/'video.webm',run/'video.webm')
provider=json.loads(arguments.provider.read_text()) if arguments.provider else None
if provider:
    write(run/'provider.json',provider)
    for entry in provider.get('configuration',[]):
        relative=Path(entry['destination'])
        assert not relative.is_absolute() and '..' not in relative.parts
        destination=run/'config'/relative
        destination.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(entry['source'],destination)
write(run/'cell/park.json',{'terminal':{'program':arguments.terminal,'arguments':[]},'ready':{'program':sys.executable,'arguments':[str(Path(__file__).resolve()),'ready',str(run),str(arguments.capture_probe.resolve())],'capabilities':(provider.get('capabilities',[]) if provider else ['capture','window-list'])}})
environment=host_environment()
outer=Path(environment.get('WAYLAND_DISPLAY','wayland-1'))
if not outer.is_absolute():outer=Path(environment['XDG_RUNTIME_DIR'])/outer
environment.update(XDG_RUNTIME_DIR=str(runtime),WAYLAND_DISPLAY=str(outer),LINCE_PARK_TRACE_FRAMES='1')
for key in ['WAYLAND_SOCKET','LINCE_PARK_BOOTSTRAP_PID']:environment.pop(key,None)
def interrupt(signum,frame):raise RuntimeError(f'Interrupted by signal {signum}')
signal.signal(signal.SIGTERM,interrupt)
signal.signal(signal.SIGINT,interrupt)
children=[];result={'directory':str(run),'functional_automation':'running','physical_acceptance':'not run','cpu_ticks_per_second':os.sysconf('SC_CLK_TCK')}

def spawn(command,log,env=None):
    output=open(run/log,'w')
    process=subprocess.Popen(command,stdout=output,stderr=subprocess.STDOUT,env=env or environment,cwd=run,start_new_session=True)
    children.append(process);return process

base=f'http://127.0.0.1:{webdriver}'
session=None

def manager_environment():
    try:data=subprocess.check_output(['systemctl','--user','show-environment'],timeout=5,stderr=subprocess.DEVNULL).decode()
    except (FileNotFoundError,subprocess.SubprocessError):return None
    return {key:value for key,value in (line.split('=',1) for line in data.splitlines() if '=' in line) if key in ['WAYLAND_DISPLAY','XDG_CURRENT_DESKTOP','XDG_SESSION_TYPE','NIRI_SOCKET']}

result['user_manager_before']=manager_environment()

def request(method,path,payload=None):
    data=None if payload is None else json.dumps(payload).encode()
    query=urllib.request.Request(base+path,data=data,method=method,headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(query,timeout=20) as response:value=json.load(response)
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read().decode()) from error
    value=value.get('value',value)
    if isinstance(value,dict) and 'error' in value:raise RuntimeError(value)
    return value

def command(method,path,payload=None):return request(method,'/session/'+session+path,payload)
def javascript(script):return command('POST','/execute/sync',{'script':script,'args':[]})

try:
    information=json.loads(subprocess.check_output([str(arguments.executable.resolve()),'--build-info'],env=environment,timeout=10))
    assert information['optimized'] and information['features']['park'] and information['features']['ui']
    result['build_info']=information
    executable=arguments.executable.resolve()
    payload=executable.with_name('.lince-wrapped')
    if not payload.exists():payload=executable
    with payload.open('rb') as binary:result['executable_sha256']=hashlib.file_digest(binary,'sha256').hexdigest()
    result['executable']=str(executable)
    result['payload']=str(payload)
    commandline=['dbus-run-session','--',str(arguments.executable.resolve()),'--park-nested','--directory',str(run/'cell'),'--listen-addr','127.0.0.1:0','--peer-port','0','--park-outputs',str(arguments.outputs)]
    if not arguments.plain:commandline.append('--showoff')
    commandline+=['--',sys.executable,str(Path(__file__).resolve()),'client',str(run),arguments.firefox,'--no-remote','--marionette','--profile',str(run/'profile'),(run/'page.html').as_uri()]
    compositor=spawn(commandline,'session.log')
    ready=wait(lambda:json.loads((run/'ready.json').read_text()))
    if arguments.host_prepare:
        subprocess.run([str(arguments.host_prepare.resolve()),str(ready['pid'])],env=host_environment(),timeout=10,check=True,capture_output=True)
    inner=environment.copy();inner.update({key:value for key,value in ready['environment'].items() if value is not None});inner.pop('WAYLAND_SOCKET',None)
    for key in ['NIRI_SOCKET','SWAYSOCK','I3SOCK','HYPRLAND_INSTANCE_SIGNATURE','WAYFIRE_SOCKET']:inner.pop(key,None)
    spawn([str(arguments.geckodriver.resolve()),'--connect-existing','--marionette-port',str(marionette),'--port',str(webdriver)],'webdriver.log')
    wait(lambda:request('GET','/status'))
    def marionette_ready():
        with socket.create_connection(('127.0.0.1',marionette),timeout=.2):return True
    wait(marionette_ready)
    response=wait(lambda:request('POST','/session',{'capabilities':{'alwaysMatch':{'browserName':'firefox'}}}))
    session=response['sessionId']
    wait(lambda:javascript('return typeof window.parkMetrics === "function" && window.parkMetrics().videoWidth > 0'))
    result['browser']=response.get('capabilities')
    element=command('POST','/element',{'using':'css selector','value':'input'})
    element_id=element['element-6066-11e4-a52e-4f735466cecf']
    command('POST','/element/'+element_id+'/click',{})
    command('POST','/element/'+element_id+'/value',{'text':'Park typed 67'})
    assert javascript('return document.querySelector("input").value')=='Park typed 67'
    command('POST','/actions',{'actions':[{'type':'pointer','id':'mouse','parameters':{'pointerType':'mouse'},'actions':[{'type':'pointerMove','duration':0,'origin':element,'x':0,'y':0},{'type':'pointerDown','button':2},{'type':'pointerUp','button':2}]}]})
    time.sleep(1)
    if not provider:
        (run/'capture.request').touch()
        capture=wait(lambda:json.loads((run/'capture.json').read_text()))
        assert capture['status']=='passed',capture
        result['capture']=capture
    command('POST','/element/'+element_id+'/value',{'text':'\ue00c'})
    if arguments.lifecycle_rounds:
        original=command('GET','/window')
        baseline=command('GET','/window/handles')
        result['client_lifecycle']={'baseline':resources(ready['pid'],0),'rounds':[],'scope':'Real application window creation, geometry transitions and closure; WebDriver does not establish physical keyboard focus or scanout.'}
        lifecycle_start=time.monotonic()
        for index in range(arguments.lifecycle_rounds):
            started=time.monotonic()
            created=command('POST','/window/new',{'type':'window'})
            command('POST','/window',{'handle':created['handle']})
            command('POST','/url',{'url':(run/'page.html').as_uri()})
            wait(lambda:javascript('return typeof window.parkMetrics === "function" && window.parkMetrics().videoWidth > 0'))
            maximized=command('POST','/window/maximize',{})
            fullscreen=command('POST','/window/fullscreen',{})
            requested={'width':720+index%2*80,'height':480+index%2*80}
            restored=command('POST','/window/rect',requested)
            for rectangle in [maximized,fullscreen,restored]:
                assert rectangle['width']>0 and rectangle['height']>0,rectangle
            assert fullscreen['width']>=maximized['width'] and fullscreen['height']>=maximized['height'],(maximized,fullscreen)
            assert all(restored[axis]==requested[axis] for axis in ['width','height']),(requested,restored)
            assert javascript('return !window.fullScreen'),'Browser did not leave fullscreen'
            command('DELETE','/window')
            command('POST','/window',{'handle':original})
            assert set(command('GET','/window/handles'))==set(baseline),'A closed application window was retained'
            assert javascript('return document.querySelector("input").value')=='Park typed 67'
            assert compositor.poll() is None,'Park exited during repeated client lifecycle'
            time.sleep(1)
            result['client_lifecycle']['rounds'].append({'round':index+1,'seconds':time.monotonic()-started,'maximized':maximized,'fullscreen':fullscreen,'requested_restore':requested,'actual_restore':restored,'resources':resources(ready['pid'],time.monotonic()-lifecycle_start)})
        result['client_lifecycle']['status']='passed'
    if provider:
        inner.update(XDG_CONFIG_HOME=str(run/'config'),XDG_CACHE_HOME=str(run/'cache'),XDG_STATE_HOME=str(run/'state'))
        command('POST','/window/maximize',{})
        def control(name):
            target=provider[name]
            return subprocess.run([target['program'],*target.get('arguments',[])],env=inner,cwd=run,capture_output=True,timeout=10,check=True).stdout.decode()
        wait(lambda:control('show') or True)
        time.sleep(2)
        shown=command('GET','/window/rect')
        control('hide');time.sleep(2)
        hidden=command('GET','/window/rect')
        control('show');time.sleep(2)
        restored=command('GET','/window/rect')
        result['bar_rectangles']={'shown':shown,'hidden':hidden,'restored':restored}
        assert hidden['height']>shown['height'] and restored['height']==shown['height'],result['bar_rectangles']
        if 'open' in provider:control('open');time.sleep(1)
        if 'close' in provider:control('close');time.sleep(1)
        result['bar_placement']='passed'
        result['provider_permissions']=provider.get('capabilities',[])
    if not arguments.keep_size:command('POST','/window/maximize',{});time.sleep(2)
    if arguments.fullscreen:
        command('POST','/window/fullscreen',{});time.sleep(2)
    javascript("document.querySelector('video').scrollIntoView({block:'center'});return true")
    result['video_visibility']=javascript("const r=document.querySelector('video').getBoundingClientRect();return {top:r.top,bottom:r.bottom,height:r.height,viewport:innerHeight,visible:r.top>=0&&r.bottom<=innerHeight}")
    assert result['video_visibility']['visible'],result['video_visibility']
    trace_offset=(run/'session.log').stat().st_size
    result['video_start']=javascript('return window.parkMetrics()')
    start=time.monotonic()
    result['resources']=[]
    while time.monotonic()-start<arguments.seconds:
        assert compositor.poll() is None,'compositor exited during video measurement'
        result['resources'].append(resources(ready['pid'],time.monotonic()-start))
        time.sleep(1)
    result['video_end']=javascript('return window.parkMetrics()')
    with (run/'session.log').open('rb') as trace:
        trace.seek(trace_offset);(run/'measured.log').write_bytes(trace.read())
    if arguments.ownership:
        for mode in ['--park-nested','--park']:
            contender=subprocess.run([str(executable),mode,'--directory',str(run/'cell'),'--listen-addr','127.0.0.1:0','--peer-port','0'],env=environment,cwd=run,capture_output=True,text=True,timeout=10)
            assert contender.returncode!=0 and 'Another Lince instance owns' in contender.stderr,contender.stderr
        activated=subprocess.run([str(executable),'--directory',str(run/'cell')],env=environment,cwd=run,capture_output=True,text=True,timeout=10)
        assert activated.returncode==0,activated.stderr
        assert compositor.poll() is None
        result['ownership']='session contenders rejected; ordinary invocation activated the same owner'
    if arguments.save_failure:
        blocked=run/'cell/park-windows.json'
        backup=blocked.with_suffix('.before-failure')
        if blocked.exists():blocked.rename(backup)
        blocked.mkdir()
        try:
            command('POST','/window/new',{'type':'window'})
            time.sleep(3)
            subprocess.run([str(arguments.host_close.resolve()),str(ready['pid'])],env=host_environment(),timeout=10,check=True,capture_output=True)
            time.sleep(7)
            assert compositor.poll() is None,'Park exited despite the forced customization write failure'
            assert 'window customizations could not be saved' in (run/'session.log').read_text()
            result['failed_save_keeps_desktop']='passed; retry after restoring storage follows during cleanup'
        finally:
            blocked.rmdir()
            if backup.exists():backup.rename(blocked)
    result['functional_automation']='passed'
except Exception as error:
    result['functional_automation']='failed';result['error']=repr(error)
finally:
    write(run/'results.json',result)
    if 'compositor' in locals() and compositor.poll() is None:
        try:
            if not arguments.host_close:raise RuntimeError('Save-aware logout unrun: supply a host-close program accepting the compositor PID')
            subprocess.run([str(arguments.host_close.resolve()),str(ready['pid'])],env=host_environment(),timeout=10,check=True,capture_output=True)
            compositor.wait(timeout=20)
            result['compositor_exit']=compositor.returncode
            assert compositor.returncode==0,'compositor failed during logout'
            result['save_aware_logout']='passed'
            if arguments.reopen_sandbox:
                sandbox=spawn(['dbus-run-session','--',sys.executable,str(Path(__file__).resolve()),'sandbox',str(run),str(executable),'--directory',str(run/'cell'),'--listen-addr','127.0.0.1:0','--peer-port','0'],'sandbox.log',host_environment())
                sandbox_pid=wait(lambda:json.loads((run/'sandbox.json').read_text()))['pid']
                time.sleep(5)
                assert sandbox.poll() is None,'Sandbox exited before reopening the saved Cell'
                if arguments.host_prepare:subprocess.run([str(arguments.host_prepare.resolve()),str(sandbox_pid)],env=host_environment(),timeout=10,check=True,capture_output=True)
                time.sleep(5)
                subprocess.run([str(arguments.host_close.resolve()),str(sandbox_pid)],env=host_environment(),timeout=10,check=True,capture_output=True)
                sandbox.wait(timeout=20)
                assert sandbox.returncode==0,'Sandbox did not save and close normally'
                snapshots=sorted((run/'cell/interface.snapshots').glob('*.json'))
                assert snapshots,'No saved workspace snapshot'
                saved=json.loads(snapshots[-1].read_text())
                if not arguments.plain:assert any(space.get('name')=='showoff' for space in saved['workspaces']),'Showoff was not preserved after Sandbox reopened'
                result['sandbox_reopen']='passed; same Cell, no --showoff argument, orderly save and close'
        except Exception as error:
            result['cleanup_error']=repr(error)
            result['save_aware_logout']='unrun'
            if arguments.host_close:result['functional_automation']='failed'
    result['cleanup_observations']=[{'pid':child.pid,'exit_before_cleanup':child.poll()} for child in children]
    for child in reversed(children):
        try:os.killpg(child.pid,signal.SIGTERM)
        except ProcessLookupError:pass
        if child.poll() is None:
            try:child.wait(timeout=5)
            except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()
    result['user_manager_after']=manager_environment()
    result['user_manager_unchanged']=result['user_manager_after']==result['user_manager_before'] if result['user_manager_before'] is not None and result['user_manager_after'] is not None else None
    if result['user_manager_unchanged'] is False:result['functional_automation']='failed'
    write(run/'results.json',result)
    print(json.dumps({key:value for key,value in result.items() if key not in ['resources','browser']},indent=2),flush=True)
if result['functional_automation']!='passed':raise SystemExit(1)
