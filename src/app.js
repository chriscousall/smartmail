import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { visibleMessages, replyAccount, groupAccountCount } from './model.js';

const byId = id => document.getElementById(id);
const appWindow = getCurrentWindow();
let htmlResizeObserver = null;
const state = {
  groups: [],
  accounts: [],
  messages: [],
  scope: 'all',
  search: '',
  selectedId: null,
  newestFirst: true,
  editingGroupId: null,
  movingAccountId: null,
  contextAccountId: null,
};

function element(tag, className, content) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (content !== undefined) node.textContent = content;
  return node;
}

function accountFor(message) { return state.accounts.find(account => account.id === message.accountId); }
function groupFor(account) { return state.groups.find(group => group.id === account?.groupId); }
function messagesInView() { return visibleMessages(state.messages, state.accounts, state.scope, state.search, state.newestFirst); }
async function refreshSnapshot() {
  const snapshot = await invoke('get_snapshot');
  state.groups = snapshot.groups;
  state.accounts = snapshot.accounts;
  state.messages = snapshot.messages;
  render();
  updateAccountControls();
}
function updateAccountControls() {
  byId('syncButton').hidden = state.accounts.length === 0;
  byId('composeButton').disabled = state.accounts.length === 0;
}
async function runNative(task, success) {
  try { await task(); if (success) showToast(success); }
  catch (error) { showToast(String(error)); }
}
async function updateFlags(message, values) {
  await runNative(async () => {
    await invoke('set_message_flags', { accountId: message.accountId, uidvalidity: message.uidvalidity, uid: message.uid, unread: values.unread, starred: values.starred });
    message.unread = values.unread; message.starred = values.starred; render();
  });
}
async function selectMessage(message) {
  state.selectedId = message.id;
  render();
  if (!message.bodyLoaded) {
    try {
      const content = await invoke('load_message_body', { accountId: message.accountId, uidvalidity: message.uidvalidity, uid: message.uid });
      message.body = content.body;
      message.html = content.html;
      message.attachments = content.attachments;
      message.bodyLoaded = true;
      message.snippet = message.body.slice(0, 120);
      render();
    } catch (error) { showToast(String(error)); }
  }
  if (message.unread) await updateFlags(message, { unread: false, starred: message.starred });
}

function viewDetails() {
  if (state.scope === 'all') return 'All inboxes';
  if (state.scope === 'starred') return 'Starred';
  if (state.scope === 'unread') return 'Unread';
  if (state.scope.startsWith('group:')) {
    const group = state.groups.find(item => item.id === state.scope.slice(6));
    return group?.name ?? 'Group';
  }
  const account = state.accounts.find(item => item.id === state.scope.slice(8));
  return account?.name ?? 'Account';
}

function chooseScope(scope) {
  closeAccountContextMenu();
  state.scope = scope;
  state.selectedId = null;
  render();
}

function renderNavigation() {
  byId('allCount').textContent = String(state.messages.filter(message => message.unread).length);
  for (const button of document.querySelectorAll('[data-scope]')) {
    button.classList.toggle('active', button.dataset.scope === state.scope);
  }
  const container = byId('groupsNav');
  container.replaceChildren();
  for (const group of [...state.groups].sort((a, b) => a.order - b.order)) {
    const wrap = element('div', 'group-wrap');
    const row = element('div', `group-row${state.scope === `group:${group.id}` ? ' active' : ''}`);
    const groupButton = element('button', `nav-item group-button${state.scope === `group:${group.id}` ? ' active' : ''}`);
    groupButton.type = 'button';
    groupButton.append(element('span', 'group-chevron', '▾'), element('span', '', group.name));
    groupButton.addEventListener('click', () => chooseScope(`group:${group.id}`));
    const menu = element('button', 'icon-button group-menu', '···');
    menu.type = 'button';
    menu.title = `Edit ${group.name}`;
    menu.setAttribute('aria-label', `Edit ${group.name}`);
    menu.addEventListener('click', () => openGroupDialog(group.id));
    row.append(groupButton, menu);
    const accountsList = element('div', 'account-list');
    for (const account of state.accounts.filter(item => item.groupId === group.id)) {
      const accountRow = element('div', 'account-row');
      const button = element('button', `account-item${state.scope === `account:${account.id}` ? ' active' : ''}`);
      button.type = 'button';
      button.title = account.email;
      const dot = element('span', 'account-dot');
      dot.style.backgroundColor = account.color;
      button.append(dot, element('span', '', account.name));
      button.addEventListener('click', () => chooseScope(`account:${account.id}`));
      button.addEventListener('keydown', event => {
        if (event.key !== 'ContextMenu' && !(event.shiftKey && event.key === 'F10')) return;
        event.preventDefault();
        const bounds = accountRow.getBoundingClientRect();
        openAccountContextMenu(account.id, bounds.left + 12, bounds.bottom);
      });
      accountRow.addEventListener('contextmenu', event => {
        event.preventDefault();
        openAccountContextMenu(account.id, event.clientX, event.clientY);
      });
      accountRow.append(button);
      accountsList.append(accountRow);
    }
    wrap.append(row, accountsList);
    container.append(wrap);
  }
}

function formatTime(iso) {
  const date = new Date(iso);
  const today = new Date();
  if (date.toDateString() === today.toDateString()) return new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit' }).format(date);
  return new Intl.DateTimeFormat('en-GB', { day: 'numeric', month: 'short' }).format(date);
}

function renderList() {
  const items = messagesInView();
  const title = viewDetails();
  byId('breadcrumbCurrent').textContent = title;
  byId('viewTitle').textContent = title;
  byId('messageCount').textContent = `${items.length} ${items.length === 1 ? 'message' : 'messages'}`;
  byId('sortButton').firstChild.textContent = state.newestFirst ? 'Newest first ' : 'Oldest first ';
  if (!items.some(message => message.id === state.selectedId)) state.selectedId = null;
  const list = byId('messageList');
  list.replaceChildren();
  if (!items.length) {
    list.append(element('p', 'empty-list', state.search ? 'No messages match that search. Try a different word.' : !state.accounts.length ? 'Connect an account to see your mail here.' : 'Nothing here yet. Choose another account or group.'));
    return;
  }
  for (const message of items) {
    const account = accountFor(message);
    const card = element('div', `message-card${message.unread ? ' unread' : ''}${message.id === state.selectedId ? ' selected' : ''}`);
    const open = element('button', 'message-open');
    open.type = 'button';
    open.setAttribute('aria-label', `Read ${message.subject} from ${message.sender} in ${account?.email ?? 'this account'}`);
    const top = element('span', 'message-top');
    const meta = element('span', 'message-meta');
    meta.append(element('span', 'time', formatTime(message.time)));
    if (state.scope === 'all' && account?.email) {
      const accountAddress = element('span', 'account-address', account.email);
      accountAddress.title = account.email;
      meta.append(element('span', 'meta-separator', '·'), accountAddress);
    }
    top.append(element('span', 'sender', message.sender), meta);
    const subject = element('span', 'subject', message.subject);
    const snippet = element('span', 'snippet', message.snippet);
    const star = element('button', `star${message.starred ? ' on' : ''}`, message.starred ? '★' : '☆');
    star.type = 'button';
    star.setAttribute('aria-label', message.starred ? 'Remove star' : 'Star message');
    star.addEventListener('click', () => updateFlags(message, { unread: message.unread, starred: !message.starred }));
    open.append(top, subject, snippet);
    open.addEventListener('click', () => selectMessage(message));
    card.append(open, star);
    list.append(card);
  }
}

function renderReading() {
  htmlResizeObserver?.disconnect();
  htmlResizeObserver = null;
  const panel = byId('readingPanel');
  panel.replaceChildren();
  const message = state.messages.find(item => item.id === state.selectedId);
  if (!message) {
    const empty = element('div', 'reading-empty');
    empty.append(element('div', 'empty-illustration', '✉'), element('h2', '', !state.accounts.length ? 'Your inbox starts here' : 'A little breathing room'), element('p', '', !state.accounts.length ? 'Connect an account to start reading.' : 'Choose a message to read it here.'));
    if (!state.accounts.length) {
      const connect = element('button', 'action-button empty-action', 'Connect an account');
      connect.type = 'button';
      connect.addEventListener('click', openConnectDialog);
      empty.append(connect);
    }
    panel.append(empty);
    return;
  }
  const account = accountFor(message);
  const inner = element('div', 'reading-inner');
  const toolbar = element('div', 'reading-toolbar');
  toolbar.append(element('span', '', 'MESSAGE  /  ' + (groupFor(account)?.name.toUpperCase() ?? '')));
  const actions = element('div', 'reading-actions');
  const unread = element('button', 'tool-button', '◌');
  unread.type = 'button'; unread.title = message.unread ? 'Mark as read' : 'Mark as unread'; unread.setAttribute('aria-label', unread.title);
  unread.addEventListener('click', () => updateFlags(message, { unread: !message.unread, starred: message.starred }));
  const star = element('button', 'tool-button', message.starred ? '★' : '☆');
  star.type = 'button'; star.title = message.starred ? 'Remove star' : 'Star message'; star.setAttribute('aria-label', star.title);
  star.addEventListener('click', () => updateFlags(message, { unread: message.unread, starred: !message.starred }));
  actions.append(unread, star); toolbar.append(actions);
  inner.append(toolbar, element('h2', 'reading-title', message.subject), element('span', 'reading-account', `To ${account?.name ?? 'Unknown account'} · ${account?.email ?? ''}`));
  const author = element('div', 'message-author');
  const initials = message.sender.split(' ').map(part => part[0]).slice(0, 2).join('').toUpperCase();
  const info = element('div', 'author-info');
  info.append(element('strong', '', message.sender), element('span', '', `From ${message.from} · To ${message.to}`));
  author.append(element('div', 'author-avatar', initials), info, element('span', 'message-date', new Intl.DateTimeFormat('en-GB', { day: 'numeric', month: 'long', year: 'numeric' }).format(new Date(message.time))));
  inner.append(author);
  if (message.bodyLoaded && message.html && !message.showText) {
    const controls = element('div', 'html-controls');
    if (!message.loadImages) {
      controls.append(element('span', '', 'Remote images are blocked'));
      const load = element('button', 'reply-button', 'Load images');
      load.type = 'button';
      load.addEventListener('click', () => { message.loadImages = true; renderReading(); });
      controls.append(load);
    }
    const textButton = element('button', 'reply-button', 'Text view');
    textButton.type = 'button';
    textButton.addEventListener('click', () => { message.showText = true; renderReading(); });
    controls.append(textButton);
    const frame = element('iframe', 'mail-html');
    frame.title = 'Email content';
    // Parent sizing needs DOM access; the email still cannot run scripts or navigate the app.
    frame.setAttribute('sandbox', 'allow-same-origin');
    frame.referrerPolicy = 'no-referrer';
    frame.scrolling = 'no';
    frame.addEventListener('load', () => {
      const document = frame.contentDocument;
      if (!document?.body || !frame.isConnected) return;
      const resize = () => {
        if (!frame.isConnected) return;
        const height = Math.min(50000, Math.max(120, Math.ceil(document.body.scrollHeight)));
        if (Math.abs(frame.getBoundingClientRect().height - height) > 1) frame.style.height = `${height}px`;
      };
      htmlResizeObserver?.disconnect();
      htmlResizeObserver = new ResizeObserver(resize);
      htmlResizeObserver.observe(document.body);
      document.querySelectorAll('img').forEach(image => image.addEventListener('load', resize));
      resize();
    });
    frame.srcdoc = htmlDocument(message.html, message.loadImages);
    inner.append(controls, frame);
  } else {
    if (message.bodyLoaded && message.html) {
      const htmlButton = element('button', 'reply-button view-html', 'HTML view');
      htmlButton.type = 'button';
      htmlButton.addEventListener('click', () => { message.showText = false; renderReading(); });
      inner.append(htmlButton);
    }
    const readableBody = message.body.replace(/\r\n?/g, '\n').replace(/\n(?:[ \t]*\n){2,}/g, '\n\n');
    inner.append(element('div', 'mail-body', !message.bodyLoaded ? 'Loading message…' : readableBody));
  }
  if (message.attachments?.length) {
    const section = element('section', 'attachments');
    section.append(element('h3', '', `Attachments · ${message.attachments.length}`));
    for (const attachment of message.attachments) {
      const row = element('div', 'attachment-row');
      const details = element('div', 'attachment-details');
      details.append(element('strong', '', attachment.name), element('span', '', formatFileSize(attachment.size)));
      const save = element('button', 'reply-button', 'Save to Downloads');
      save.type = 'button';
      save.addEventListener('click', async () => {
        save.disabled = true;
        try {
          const filename = await invoke('save_attachment', { accountId: message.accountId, uidvalidity: message.uidvalidity, uid: message.uid, partId: attachment.partId });
          showToast(`Saved ${filename} to Downloads`);
        } catch (error) { showToast(String(error)); }
        finally { save.disabled = false; }
      });
      row.append(details, save);
      section.append(row);
    }
    inner.append(section);
  }
  const reply = element('button', 'reply-button', '↩  Reply');
  reply.type = 'button'; reply.addEventListener('click', () => openCompose(message));
  inner.append(reply); panel.append(inner);
}

function render() { renderNavigation(); renderList(); renderReading(); }

function htmlDocument(html, loadImages) {
  const imageSources = loadImages ? 'https: data:' : 'data:';
  const policy = `default-src 'none'; img-src ${imageSources}; style-src 'unsafe-inline'; script-src 'none'; font-src 'none'; connect-src 'none'; media-src 'none'; frame-src 'none'; object-src 'none'; form-action 'none'; base-uri 'none'; navigate-to 'none'`;
  return `<!doctype html><html><head><meta http-equiv="Content-Security-Policy" content="${policy}"><meta name="referrer" content="no-referrer"><style>html,body{margin:0;padding:0;max-width:100%;overflow:hidden;overflow-wrap:anywhere}body{padding:16px;background:#fff;color:#242424;font:13px/1.6 Arial,sans-serif}img,table{max-width:100%}img{height:auto}a{pointer-events:none}</style></head><body>${html}</body></html>`;
}

function formatFileSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function openCompose(replyTo = null) {
  if (!state.accounts.length) { showToast('Connect an account before composing.'); return; }
  const select = byId('composeFrom');
  select.replaceChildren();
  for (const account of state.accounts) {
    const option = element('option', '', `${account.name} <${account.email}>`);
    option.value = account.id; select.append(option);
  }
  const account = replyTo ? replyAccount(replyTo, state.accounts) : state.accounts.find(item => state.scope === `account:${item.id}`) ?? state.accounts[0];
  select.value = account?.id ?? '';
  byId('composeTitle').textContent = replyTo ? `Reply to ${replyTo.sender}` : 'New message';
  byId('composeTo').value = replyTo?.from ?? '';
  byId('composeSubject').value = replyTo ? `Re: ${replyTo.subject.replace(/^Re:\s*/i, '')}` : '';
  byId('composeBody').value = replyTo ? `\n\nOn ${new Intl.DateTimeFormat('en-GB', { dateStyle: 'medium' }).format(new Date(replyTo.time))}, ${replyTo.sender} wrote:\n> ${replyTo.body.replaceAll('\n', '\n> ')}` : '';
  byId('composeDialog').showModal();
  byId('composeTo').focus();
}

function openGroupDialog(groupId = null) {
  state.editingGroupId = groupId;
  const group = state.groups.find(item => item.id === groupId);
  byId('groupDialogTitle').textContent = group ? 'Edit group' : 'Create a group';
  byId('groupName').value = group?.name ?? '';
  byId('saveGroupButton').textContent = group ? 'Save changes' : 'Create group';
  byId('deleteGroupButton').hidden = !group;
  byId('groupDialog').showModal();
  byId('groupName').focus();
}

function openMoveDialog(accountId) {
  closeAccountContextMenu();
  state.movingAccountId = accountId;
  const account = state.accounts.find(item => item.id === accountId);
  byId('moveAccountName').textContent = `${account?.name ?? 'Account'} · ${account?.email ?? ''}`;
  const select = byId('moveGroup'); select.replaceChildren();
  for (const group of state.groups) {
    const option = element('option', '', group.name); option.value = group.id; select.append(option);
  }
  select.value = account?.groupId ?? '';
  byId('moveDialog').showModal();
}

function closeAccountContextMenu() {
  byId('accountContextMenu').hidden = true;
  state.contextAccountId = null;
}

function openAccountContextMenu(accountId, x, y) {
  closeMenus();
  state.contextAccountId = accountId;
  const menu = byId('accountContextMenu');
  menu.hidden = false;
  menu.style.left = `${Math.max(8, Math.min(x, window.innerWidth - menu.offsetWidth - 8))}px`;
  menu.style.top = `${Math.max(8, Math.min(y, window.innerHeight - menu.offsetHeight - 8))}px`;
  byId('moveAccountMenuItem').focus();
}

function openConnectDialog() {
  const select = byId('accountGroup'); select.replaceChildren();
  for (const group of state.groups) { const option = element('option', '', group.name); option.value = group.id; select.append(option); }
  byId('connectDialog').showModal();
}

let toastTimer;
function showToast(message) {
  const toast = byId('toast'); toast.textContent = message; toast.classList.add('show');
  clearTimeout(toastTimer); toastTimer = setTimeout(() => toast.classList.remove('show'), 3500);
}

const savedTheme = localStorage.getItem('smartmail-theme');
let themePreference = savedTheme === 'light' || savedTheme === 'dark'
  ? savedTheme
  : window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
localStorage.setItem('smartmail-theme', themePreference);
function applyTheme() {
  document.documentElement.dataset.theme = themePreference;
  document.querySelectorAll('[data-theme-choice]').forEach(button => {
    button.classList.toggle('selected', button.dataset.themeChoice === themePreference);
  });
}
function setTheme(value) {
  themePreference = value;
  localStorage.setItem('smartmail-theme', value);
  applyTheme();
}
applyTheme();

const defaultUnreadColor = '#4cae78';
const savedUnreadColor = localStorage.getItem('smartmail-unread-color');
const unreadColor = /^#[0-9a-f]{6}$/i.test(savedUnreadColor ?? '') ? savedUnreadColor : defaultUnreadColor;
document.documentElement.style.setProperty('--unread-accent', unreadColor);
byId('unreadColor').value = unreadColor;
byId('unreadColor').addEventListener('input', event => {
  const color = event.target.value;
  if (!/^#[0-9a-f]{6}$/i.test(color)) return;
  document.documentElement.style.setProperty('--unread-accent', color);
  localStorage.setItem('smartmail-unread-color', color);
});

function isoWeek(date) {
  const day = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()));
  day.setUTCDate(day.getUTCDate() + 4 - (day.getUTCDay() || 7));
  const yearStart = new Date(Date.UTC(day.getUTCFullYear(), 0, 1));
  return Math.ceil((((day - yearStart) / 86400000) + 1) / 7);
}
function updateDate() {
  const now = new Date();
  byId('todayLabel').textContent = new Intl.DateTimeFormat('en-GB', { weekday: 'long', day: 'numeric', month: 'long' }).format(now);
  byId('weekLabel').textContent = `Week ${isoWeek(now)}`;
}
updateDate();
setInterval(updateDate, 60_000);

function closeMenus() {
  document.querySelectorAll('.menu-trigger').forEach(button => button.setAttribute('aria-expanded', 'false'));
  document.querySelectorAll('.menu-popover').forEach(panel => { panel.hidden = true; });
}
document.querySelectorAll('.menu-trigger').forEach(button => button.addEventListener('click', () => {
  const opening = button.getAttribute('aria-expanded') !== 'true';
  closeMenus();
  if (opening) { button.setAttribute('aria-expanded', 'true'); byId(`menu-${button.dataset.menu}`).hidden = false; }
}));
document.addEventListener('click', event => { if (!event.target.closest('.window-menus')) closeMenus(); });
document.addEventListener('pointerdown', event => {
  if (!byId('accountContextMenu').contains(event.target)) closeAccountContextMenu();
});
document.addEventListener('scroll', closeAccountContextMenu, true);
window.addEventListener('resize', closeAccountContextMenu);
byId('moveAccountMenuItem').addEventListener('click', () => {
  const accountId = state.contextAccountId;
  if (accountId && state.accounts.some(account => account.id === accountId)) openMoveDialog(accountId);
});
document.querySelectorAll('[data-theme-choice]').forEach(button => button.addEventListener('click', () => {
  setTheme(button.dataset.themeChoice); closeMenus();
}));
document.querySelectorAll('[data-menu-action]').forEach(button => button.addEventListener('click', () => {
  const action = button.dataset.menuAction;
  closeMenus();
  if (action === 'compose') openCompose();
  else if (action === 'connect') openConnectDialog();
  else if (action === 'settings') byId('settingsDialog').showModal();
  else if (action === 'sync') { if (state.accounts.length) byId('syncButton').click(); else showToast('Connect an account before syncing.'); }
  else if (action === 'group') openGroupDialog();
  else if (action === 'search') byId('searchInput').focus();
  else if (action === 'all' || action === 'unread' || action === 'starred') chooseScope(action);
  else if (action === 'close') appWindow.close().catch(error => showToast(String(error)));
}));
byId('minimizeButton').addEventListener('click', () => appWindow.minimize().catch(error => showToast(String(error))));
byId('maximizeButton').addEventListener('click', () => appWindow.toggleMaximize().catch(error => showToast(String(error))));
byId('closeButton').addEventListener('click', () => appWindow.close().catch(error => showToast(String(error))));
document.querySelector('.window-drag').addEventListener('dblclick', () => appWindow.toggleMaximize().catch(error => showToast(String(error))));

document.querySelectorAll('[data-scope]').forEach(button => button.addEventListener('click', () => chooseScope(button.dataset.scope)));
document.querySelectorAll('[data-close]').forEach(button => button.addEventListener('click', () => byId(button.dataset.close).close()));
byId('composeButton').addEventListener('click', () => openCompose());
byId('addGroupButton').addEventListener('click', () => openGroupDialog());
byId('connectButton').addEventListener('click', openConnectDialog);
byId('searchInput').addEventListener('input', event => { state.search = event.target.value; renderList(); renderReading(); });
byId('sortButton').addEventListener('click', () => { state.newestFirst = !state.newestFirst; renderList(); });
byId('sendButton').addEventListener('click', () => {
  const recipient = byId('composeTo').value.trim();
  if (!byId('composeTo').checkValidity() || !recipient) { showToast('Enter one valid recipient address.'); return; }
  const button = byId('sendButton'); button.disabled = true; button.textContent = 'Sending…';
  runNative(async () => {
    await invoke('send_plain_text', { accountId: byId('composeFrom').value, recipient, subject: byId('composeSubject').value, body: byId('composeBody').value });
    byId('composeDialog').close();
  }, 'Message submitted to the SMTP server.').finally(() => { button.disabled = false; button.textContent = 'Send message ↗'; });
});
byId('groupForm').addEventListener('submit', async event => {
  event.preventDefault();
  const name = byId('groupName').value.trim();
  if (!name) return;
  if (state.groups.some(group => group.name.toLocaleLowerCase() === name.toLocaleLowerCase() && group.id !== state.editingGroupId)) {
    showToast('A group with that name already exists.'); return;
  }
  try {
    if (state.editingGroupId) await invoke('rename_group', { groupId: state.editingGroupId, name });
    else await invoke('create_group', { name });
    await refreshSnapshot();
  } catch (error) { showToast(String(error)); return; }
  byId('groupDialog').close(); render(); showToast(state.editingGroupId ? 'Group updated.' : 'Group created.');
});
byId('deleteGroupButton').addEventListener('click', async () => {
  const id = state.editingGroupId;
  if (groupAccountCount(state.accounts, id)) { showToast('Move the accounts out of this group first.'); return; }
  try { await invoke('delete_group', { groupId: id }); await refreshSnapshot(); }
  catch (error) { showToast(String(error)); return; }
  if (state.scope === `group:${id}`) state.scope = 'all';
  byId('groupDialog').close(); render(); showToast('Group deleted.');
});
byId('moveForm').addEventListener('submit', async event => {
  event.preventDefault();
  const account = state.accounts.find(item => item.id === state.movingAccountId);
  if (!account) return;
  try { await invoke('move_account', { accountId: account.id, groupId: byId('moveGroup').value }); await refreshSnapshot(); }
  catch (error) { showToast(String(error)); return; }
  byId('moveDialog').close(); render(); showToast(`${account.name} moved to ${groupFor(state.accounts.find(item => item.id === account.id))?.name}.`);
});
byId('manualAccountForm').addEventListener('submit', async event => {
  event.preventDefault();
  const value = id => byId(id).value.trim();
  const input = {
    name: value('accountName'), email: value('accountEmail'), groupId: value('accountGroup'),
    imapHost: value('imapHost'), imapPort: Number(value('imapPort')),
    imapUsername: value('imapUsername') || value('accountEmail'), imapPassword: byId('imapPassword').value,
    smtpHost: value('smtpHost'), smtpPort: Number(value('smtpPort')),
    smtpSecurity: value('smtpSecurity'), smtpUsername: value('smtpUsername') || value('accountEmail'),
    smtpPassword: byId('smtpPassword').value,
  };
  const button = byId('connectSubmit'); button.disabled = true; button.textContent = 'Testing connection…';
  try {
    const account = await invoke('connect_manual_account', { input });
    byId('manualAccountForm').reset(); byId('connectDialog').close();
    await refreshSnapshot();
    showToast('Account connected. Syncing inbox…');
    await invoke('sync_account', { accountId: account.id });
    await refreshSnapshot();
    showToast('Inbox synced.');
  } catch (error) { showToast(String(error)); }
  finally { button.disabled = false; button.textContent = 'Connect account'; }
});
byId('syncButton').addEventListener('click', async () => {
  const button = byId('syncButton'); button.disabled = true; button.textContent = 'Syncing…';
  try {
    const results = await Promise.allSettled(state.accounts.map(account => invoke('sync_account', { accountId: account.id })));
    await refreshSnapshot();
    const failed = results.filter(result => result.status === 'rejected');
    showToast(failed.length ? `${failed.length} account sync failed: ${String(failed[0].reason)}` : 'Inbox synced.');
  } finally { button.disabled = false; button.textContent = '↻ Sync mail'; }
});
document.addEventListener('keydown', event => {
  if (event.key === 'Escape') { closeMenus(); closeAccountContextMenu(); }
  const target = event.target;
  const typing = target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement || target?.isContentEditable;
  if (typing || document.querySelector('dialog[open]')) return;
  if (event.key === '/') { event.preventDefault(); byId('searchInput').focus(); }
  if (event.key.toLowerCase() === 'c' && !event.ctrlKey && !event.metaKey) { event.preventDefault(); openCompose(); }
});

async function loadSavedMail() {
  let lastError;
  let loaded = false;
  for (const delay of [0, 500, 1500, 3000]) {
    if (delay) await new Promise(resolve => setTimeout(resolve, delay));
    try { await refreshSnapshot(); loaded = true; break; }
    catch (error) { lastError = error; }
  }
  if (!loaded) { showToast(`Could not load saved accounts: ${String(lastError)}`); return; }
  if (!state.accounts.length) return;
  await Promise.allSettled(state.accounts.map(account => invoke('sync_account', { accountId: account.id })));
  try { await refreshSnapshot(); }
  catch (error) { showToast(`Could not refresh inbox: ${String(error)}`); }
}
loadSavedMail();
