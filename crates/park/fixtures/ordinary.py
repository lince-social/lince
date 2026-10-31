import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
import urllib.request

directory = Path(sys.argv[1])
listener = socket.socket()
listener.bind(('127.0.0.1', 0))
port = listener.getsockname()[1]
listener.close()
log = (directory / 'webdriver.log').open('w')
driver = subprocess.Popen([os.environ['LINCE_PARK_TEST_GECKODRIVER'], '--port', str(port)], stdout=log, stderr=log)
base = f'http://127.0.0.1:{port}'
session = ''

def request(method, path, value=None):
    data = None if value is None else json.dumps(value).encode()
    query = urllib.request.Request(base + path, data=data, method=method, headers={'Content-Type': 'application/json'})
    with urllib.request.urlopen(query, timeout=35) as response:
        result = json.load(response)['value']
    if isinstance(result, dict) and result.get('error'):
        raise RuntimeError(result)
    return result

try:
    for line in sys.stdin:
        try:
            command = json.loads(line)
            if command['operation'] == 'start':
                deadline = time.monotonic() + 10
                while True:
                    try:
                        request('GET', '/status')
                        break
                    except Exception:
                        if time.monotonic() >= deadline:
                            raise
                        time.sleep(0.1)
                profile = directory / 'profile'
                profile.mkdir(exist_ok=True)
                (profile / 'user.js').write_text('user_pref("browser.shell.checkDefaultBrowser", false);\nuser_pref("browser.startup.homepage_override.mstone", "ignore");\nuser_pref("browser.tabs.warnOnClose", false);\nuser_pref("browser.aboutwelcome.enabled", false);\nuser_pref("media.autoplay.default", 0);\nuser_pref("browser.sessionstore.resume_from_crash", false);\n')
                result = request('POST', '/session', {'capabilities': {'alwaysMatch': {'browserName': 'firefox', 'moz:firefoxOptions': {'binary': os.environ['LINCE_PARK_TEST_FIREFOX'], 'args': ['--no-remote', '--profile', str(profile)]}}}})
                session = result['sessionId']
                request('POST', f'/session/{session}/url', {'url': (directory / 'page.html').as_uri()})
            elif command['operation'] == 'js':
                result = request('POST', f'/session/{session}/execute/sync', {'script': command['script'], 'args': []})
            else:
                result = request(command.get('method', 'POST'), f'/session/{session}' + command['path'], command.get('value'))
            print(json.dumps({'ok': True, 'value': result}), flush=True)
        except Exception as error:
            print(json.dumps({'ok': False, 'error': str(error)}), flush=True)
finally:
    if session:
        try:
            request('DELETE', f'/session/{session}')
        except Exception:
            pass
    driver.terminate()
    try:
        driver.wait(timeout=5)
    except subprocess.TimeoutExpired:
        driver.kill()
        driver.wait()
