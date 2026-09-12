import {t} from '/i18n.js';

const node = (tag, text) => {
  const value = document.createElement(tag);
  if (text !== undefined) value.textContent = text;
  return value;
};
const choice = (entries, value) => {
  const select = node('select');
  select.replaceChildren(...entries.map(([value,label]) => new Option(label,value)));
  select.value = value;
  return select;
};
const control = (parent, title, input) => {
  const label = node('label',t(title)); label.append(input); parent.append(label); return input;
};
const button = (parent, title, action) => {
  const value = node('button',t(title)); value.type = 'button'; value.onclick = action; parent.append(value); return value;
};

export function readRulesEditor(rules, concepts) {
  const group = (predicate, depth = 0) => {
    const host = node('fieldset'); host.className = 'filter-group';
    const isGroup = Object.hasOwn(predicate,'all') || Object.hasOwn(predicate,'any');
    if (!isGroup) predicate = {all:[predicate]};
    const mode = Object.hasOwn(predicate,'all') ? 'all' : 'any';
    const match = control(host,'Match',choice([['all',t('All conditions')],['any',t('Any condition')]],mode));
    const children = node('div'); host.append(children);
    const add = predicate => {
      const row = node('div'); row.className = 'filter-row';
      let read;
      if (Object.hasOwn(predicate,'all') || Object.hasOwn(predicate,'any')) {
        if (depth >= 7) throw new Error(t('Too many filter levels.'));
        const nested = group(predicate,depth + 1); row.append(nested.host); read = nested.read;
      } else {
        const negative = Object.hasOwn(predicate,'not'), leaf = negative ? predicate.not : predicate;
        if (!Object.hasOwn(leaf,'concept_in')) throw new Error(t('This policy uses conditions that this editor cannot change yet.'));
        const operation = control(row,'Condition',choice([['is',t('Has tag')],['not',t('Does not have tag')]],negative ? 'not' : 'is'));
        const tag = control(row,'Tag',choice([['',t('Choose a tag')],...concepts.map(concept => [concept.uid,`#${concept.name}`])],leaf.concept_in));
        tag.required = true;
        if (leaf.concept_in && !concepts.some(concept => concept.uid === leaf.concept_in)) throw new Error(t('A tag in this policy is unavailable.'));
        read = () => operation.value === 'not' ? {not:{concept_in:tag.value}} : {concept_in:tag.value};
      }
      row.read = read;
      button(row,'Remove',() => row.remove()); children.append(row);
    };
    for (const child of predicate[mode]) add(child);
    button(host,'Add condition',() => add({concept_in:''}));
    if (depth < 7) button(host,'Add group',() => add({all:[]}));
    return {host, read:() => ({[match.value]:Array.from(children.children,child => child.read())})};
  };
  const host = node('div'); host.className = 'read-rules';
  const allow = group(rules.allow), block = group(rules.block);
  allow.host.prepend(node('legend',t('Allow records matching')));
  block.host.prepend(node('legend',t('Block records matching')));
  host.append(node('p',t('Allow rules share matching local records with this role. Explicit visibility restrictions still apply. Blocks win. Empty “all” matches every record; empty “any” matches none.')),allow.host,block.host);
  return {host, read:() => ({allow:allow.read(),block:block.read()})};
}
