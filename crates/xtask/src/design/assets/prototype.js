(() => {
  const theme = new URLSearchParams(location.search);
  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = `/tokens.css?${new URLSearchParams({scheme: theme.get('scheme') || 'Dark', kind: theme.get('kind') || 'Square'})}`;
  document.head.append(sheet);
  sheet.addEventListener('load', () => window.dispatchEvent(new Event('design:theme')));
  const storageKey = `lince-design:${theme.get('study') || location.host}:${location.pathname}`;
  let saved = {};
  try { saved = JSON.parse(sessionStorage.getItem(storageKey) || '{}'); } catch {}
  const save = () => { try { sessionStorage.setItem(storageKey, JSON.stringify(saved)); } catch {} };
  document.querySelectorAll('[role="tablist"]').forEach((list, index) => {
    const tabs = Array.from(list.querySelectorAll('[role="tab"]'));
    const activate = tab => {
      tabs.forEach(item => {
        const active = item === tab;
        item.setAttribute('aria-selected', String(active));
        item.tabIndex = active ? 0 : -1;
        const panel = document.getElementById(item.getAttribute('aria-controls'));
        if (panel) panel.hidden = !active;
      });
      saved[`tab:${index}`] = tab.id;
      save();
    };
    tabs.forEach((tab, position) => {
      tab.addEventListener('click', () => activate(tab));
      tab.addEventListener('keydown', event => {
        const vertical = list.getAttribute('aria-orientation') === 'vertical';
        const forward = vertical ? 'ArrowDown' : 'ArrowRight';
        const backward = vertical ? 'ArrowUp' : 'ArrowLeft';
        const target = event.key === forward ? (position + 1) % tabs.length : event.key === backward ? (position + tabs.length - 1) % tabs.length : event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : -1;
        if (target >= 0) { event.preventDefault(); activate(tabs[target]); tabs[target].focus(); }
      });
    });
    if (tabs.length) activate(tabs.find(tab => tab.id === saved[`tab:${index}`]) || tabs.find(tab => tab.getAttribute('aria-selected') === 'true') || tabs[0]);
  });
  document.querySelectorAll('input[name], textarea[name], select[name]').forEach(field => {
    if (field.type === 'password' || field.type === 'file' || field.dataset.persist === 'false') return;
    const key = `field:${field.form?.id || 'page'}:${field.name}${field.type === 'checkbox' ? `:${field.value}` : ''}`;
    if (key in saved) {
      if (field.type === 'radio') field.checked = saved[key] === field.value;
      else if (field.type === 'checkbox') field.checked = saved[key] === true;
      else { field.value = saved[key]; field.dataset.savedValue = saved[key]; }
    }
    field.addEventListener('input', () => {
      if (field.type === 'radio' && !field.checked) return;
      saved[key] = field.type === 'checkbox' ? field.checked : field.value;
      save();
    });
  });
  window.designReady = fetch('/fixtures.json', {cache: 'no-store'}).then(response => {
    if (!response.ok) throw new Error('Could not load study fixtures');
    return response.json();
  }).then(fixtures => { window.designFixtures = fixtures; return fixtures; });
})();
